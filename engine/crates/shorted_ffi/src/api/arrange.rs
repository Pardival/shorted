//! Smart arrange (docs/07): the local plan is made in the background and waits for review.

use std::sync::{Mutex, MutexGuard, PoisonError};

use shorted_ai::plan::ChangeKind;
use shorted_core::command::Command;
use shorted_core::model::{Id, Project};

use crate::api::projects::{on_command_thread, view_of, with_active, ProjectView};
use std::path::Path;

use crate::ai_access::{DeviceHttp, SystemKeychain};
use crate::ai_arrange::{last_outcome, prepare, prepared, run, Outcome};
use crate::derush::{pending, plan_locally};
use crate::error::{EngineError, EngineFailure};
use crate::frb_generated::StreamSink;
use crate::runtime::{self, Priority, TaskEvent};
use crate::transport::transport;

/// Analyses the open project in the background; the plan then waits for review (T-0803).
pub fn analyse_for_arrange(events: StreamSink<TaskEvent>) {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    runtime::global()
        .background
        .spawn(Priority::Low, events, move |task| {
            let project = project.ok_or_else(|| EngineError::Internal("no project".into()))?;
            let plan = plan_locally(&project, &folder, Some(task))?;
            *pending() = Some(plan);
            Ok::<(), EngineError>(())
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DecisionKindView {
    Trimmed,
    Cut,
    Dropped,
    Split,
    Moved,
}

/// One line of the review (S12).
pub struct DecisionView {
    pub clip_id: String,
    /// The clip's place before arranging, from 1, as the user knew it.
    pub clip_number: u32,
    pub kind: DecisionKindView,
    /// A code from the local analysis (`silence`, `blur`…) or the AI's own words.
    pub reason: Option<String>,
    pub removed_us: i64,
}

pub struct UncertainView {
    pub clip_id: String,
    pub clip_number: u32,
    pub reason: String,
}

pub struct ArrangeReviewView {
    pub before_us: i64,
    pub after_us: i64,
    pub summary: String,
    pub decisions: Vec<DecisionView>,
    pub uncertain: Vec<UncertainView>,
    pub project: ProjectView,
}

/// The project as it was before the last arrangement, for "Undo all".
static BEFORE: Mutex<Option<Project>> = Mutex::new(None);

fn before() -> MutexGuard<'static, Option<Project>> {
    BEFORE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether a plan is ready for review.
#[flutter_rust_bridge::frb(sync)]
pub fn has_pending_arrangement() -> bool {
    pending().is_some()
}

/// Applies the waiting plan as one edit and describes it for the review screen.
pub async fn apply_arrangement() -> Result<ArrangeReviewView, EngineFailure> {
    on_command_thread(|| {
        let plan = pending()
            .take()
            .ok_or_else(|| EngineError::Internal("no arrangement waiting".into()))?;
        with_active(|session| {
            let original = session.project().clone();
            session.execute(&plan.command)?;
            let number = |id: Id| {
                original
                    .timeline
                    .clips
                    .iter()
                    .position(|clip| clip.id == id)
                    .map_or(0, |index| index as u32 + 1)
            };
            let review = ArrangeReviewView {
                before_us: original.duration_us(),
                after_us: session.project().duration_us(),
                summary: plan.summary.clone(),
                decisions: plan
                    .changes
                    .iter()
                    .map(|change| DecisionView {
                        clip_id: change.clip.to_string(),
                        clip_number: number(change.clip),
                        kind: match change.kind {
                            ChangeKind::Trimmed => DecisionKindView::Trimmed,
                            ChangeKind::Cut => DecisionKindView::Cut,
                            ChangeKind::Dropped => DecisionKindView::Dropped,
                            ChangeKind::Split => DecisionKindView::Split,
                            ChangeKind::Moved => DecisionKindView::Moved,
                        },
                        reason: change.reason.clone(),
                        removed_us: change.removed_us,
                    })
                    .collect(),
                uncertain: plan
                    .uncertain
                    .iter()
                    .map(|(id, reason)| UncertainView {
                        clip_id: id.to_string(),
                        clip_number: number(*id),
                        reason: reason.clone(),
                    })
                    .collect(),
                project: view_of(session),
            };
            *before() = Some(original);
            Ok(review)
        })
    })
}

/// The edit that puts back the clips, media and captions of `original` exactly.
fn restore_command(original: &Project) -> Command {
    Command::Group {
        label: "undo arrangement".into(),
        commands: vec![
            Command::RestoreMedia {
                media: original.media.clone(),
            },
            Command::RestoreTimeline {
                timeline: Box::new(original.timeline.clone()),
                captions: Box::new(original.captions.clone()),
            },
        ],
    }
}

/// "Undo all": back to the project from before the arrangement, even after later edits. It is
/// itself one edit, which Undo takes back.
pub async fn undo_arrangement() -> Result<ProjectView, EngineFailure> {
    on_command_thread(|| {
        let original = before()
            .take()
            .ok_or_else(|| EngineError::Internal("no arrangement to undo".into()))?;
        with_active(|session| {
            session.execute(&restore_command(&original))?;
            Ok(view_of(session))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shorted_ai::plan::{parse, validate, Aliases};
    use uuid::Uuid;

    #[test]
    fn undo_all_gives_back_the_exact_project_even_after_more_edits() {
        let original =
            Project::from_json(include_str!("../../../../fixtures/projects/v1_full.json")).unwrap();
        let plan =
            parse(r#"{"version":1,"source":"local","ops":[{"op":"drop","clip":"c2"}]}"#).unwrap();
        let valid = validate(&plan, &original, &Aliases::of(&original), &mut Uuid::now_v7).unwrap();
        let mut project = original.clone();
        valid.command.apply(&mut project).unwrap();
        Command::Rename {
            title: "Later".into(),
        }
        .apply(&mut project)
        .unwrap();
        assert_ne!(project.timeline, original.timeline);
        restore_command(&original).apply(&mut project).unwrap();
        assert_eq!(project.timeline, original.timeline);
        assert_eq!(project.media, original.media);
        assert_eq!(project.captions, original.captions);
        assert_eq!(project.title, "Later", "only the arrangement is undone");
    }
}

/// What the consent sheet shows: who receives the request, and its exact body (docs/07 §4).
pub struct AiPayloadView {
    pub provider: String,
    pub model: String,
    pub url: String,
    pub body: String,
}

/// Builds the AI request for the open project, without sending it.
pub async fn prepare_ai_arrangement(
    root: String,
    language: String,
) -> Result<AiPayloadView, EngineFailure> {
    on_command_thread(move || {
        let project = transport()
            .project
            .clone()
            .ok_or_else(|| EngineError::Internal("no project".into()))?;
        let ready = prepare(Path::new(&root), &SystemKeychain, &project, &language)?;
        let view = AiPayloadView {
            provider: ready.provider.config.kind.id().into(),
            model: ready.provider.config.model.clone(),
            url: ready.request.url.clone(),
            body: ready.request.body.clone().unwrap_or_default(),
        };
        *prepared() = Some(ready);
        Ok(view)
    })
}

/// Sends exactly the request the user saw; the plan then waits for review like a local one.
pub fn send_ai_arrangement(events: StreamSink<TaskEvent>) {
    let (project, folder) = {
        let state = transport();
        (state.project.clone(), state.folder.clone())
    };
    let ready = prepared().take();
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            let ready = ready.ok_or_else(|| EngineError::Internal("nothing prepared".into()))?;
            let project = project.ok_or_else(|| EngineError::Internal("no project".into()))?;
            if ready.project != project.id {
                return Err(EngineError::Internal("another project is open".into()));
            }
            let (plan, outcome) = run(&ready, &DeviceHttp, &project, &folder, Some(task))?;
            *pending() = Some(plan);
            *last_outcome() = outcome;
            Ok(())
        });
}

/// True when the AI's plan stayed invalid and the local plan stands in.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_arrangement_fell_back() -> bool {
    *last_outcome() == Outcome::Local
}

const CONSENT_FILE: &str = "ai-consent.json";

fn consents(root: &Path) -> Vec<String> {
    std::fs::read(root.join(CONSENT_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Whether the user ticked "Remember for this project" before.
#[flutter_rust_bridge::frb(sync)]
pub fn ai_consent_remembered(root: String, project_id: String) -> bool {
    consents(Path::new(&root)).contains(&project_id)
}

pub fn remember_ai_consent(root: String, project_id: String) -> Result<(), EngineFailure> {
    let root = Path::new(&root);
    let mut known = consents(root);
    if !known.contains(&project_id) {
        known.push(project_id);
    }
    let bytes = serde_json::to_vec(&known)
        .map_err(|_| EngineError::Internal("consent not written".into()))?;
    std::fs::write(root.join(CONSENT_FILE), bytes).map_err(EngineError::from)?;
    Ok(())
}
