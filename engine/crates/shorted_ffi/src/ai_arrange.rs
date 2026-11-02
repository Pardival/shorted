//! AI arrangement (T-0806): the request is prepared first so the consent sheet shows its exact
//! bytes, then sent unchanged. An invalid plan gets one repair round, then the local plan
//! stands in (docs/07 §5).

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use shorted_ai::plan::{validate, Aliases, EditPlan, PlanSource, ValidPlan, SCHEMA_V1};
use shorted_ai::prompt::{arrange_messages, language_name, REPAIR_INSTRUCTION};
use shorted_ai::providers::{Http, HttpRequest, Provider};
use shorted_core::model::{Id, Project};
use uuid::Uuid;

use crate::ai_access::{configured, KeyStore};
use crate::derush::plan_locally;
use crate::error::EngineError;
use crate::runtime::TaskContext;

/// A request shown to the user and waiting for their consent.
pub(crate) struct Prepared {
    pub provider: Provider,
    pub project: Id,
    pub system: String,
    pub user: String,
    pub request: HttpRequest,
}

static PREPARED: Mutex<Option<Prepared>> = Mutex::new(None);

pub(crate) fn prepared() -> MutexGuard<'static, Option<Prepared>> {
    PREPARED.lock().unwrap_or_else(PoisonError::into_inner)
}

static LAST_OUTCOME: Mutex<Outcome> = Mutex::new(Outcome::Ai);

pub(crate) fn last_outcome() -> MutexGuard<'static, Outcome> {
    LAST_OUTCOME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How the plan came about, for the review screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Ai,
    /// The first answer was refused and the model fixed it.
    Repaired,
    /// The model's plan stayed invalid: the local plan stands in.
    Local,
}

fn schema() -> serde_json::Value {
    serde_json::from_str(SCHEMA_V1).unwrap_or_default()
}

pub(crate) fn prepare(
    root: &Path,
    keys: &dyn KeyStore,
    project: &Project,
    language: &str,
) -> Result<Prepared, EngineError> {
    let provider = configured(root, keys)?;
    let aliases = Aliases::of(project);
    let (system, user) = arrange_messages(project, &aliases, language_name(language));
    let request = provider.completion_request(&system, &user, &schema());
    Ok(Prepared {
        provider,
        project: project.id,
        system,
        user,
        request,
    })
}

/// A plan from the model's JSON, checked against the project; its source is always "ai".
fn check(answer: &serde_json::Value, project: &Project) -> Result<ValidPlan, Vec<String>> {
    let mut plan: EditPlan = serde_json::from_value(answer.clone())
        .map_err(|error| vec![format!("not a valid edit plan: {error}")])?;
    plan.source = PlanSource::Ai;
    validate(&plan, project, &Aliases::of(project), &mut Uuid::now_v7)
}

/// Sends the prepared request as it was shown, then repairs once, then falls back.
pub(crate) fn run(
    prepared: &Prepared,
    http: &dyn Http,
    project: &Project,
    folder: &Path,
    task: Option<&TaskContext<'_>>,
) -> Result<(ValidPlan, Outcome), EngineError> {
    let provider = &prepared.provider;
    let answer = provider
        .complete(http, &prepared.request)
        .map_err(EngineError::Ai)?;
    let errors = match check(&answer, project) {
        Ok(plan) => return Ok((plan, Outcome::Ai)),
        Err(errors) => errors,
    };
    if let Some(task) = task {
        task.check_cancelled()?;
        task.progress(0.5);
    }
    tracing::warn!(count = errors.len(), "AI plan refused, asking for a fix");
    let repair = format!(
        "{}\n\nYour plan:\n{answer}\n\n{REPAIR_INSTRUCTION}\n- {}",
        prepared.user,
        errors.join("\n- ")
    );
    let request = provider.completion_request(&prepared.system, &repair, &schema());
    let fixed = provider.complete(http, &request).map_err(EngineError::Ai)?;
    if let Ok(plan) = check(&fixed, project) {
        return Ok((plan, Outcome::Repaired));
    }
    tracing::warn!("AI plan still invalid, using the local plan");
    Ok((plan_locally(project, folder, task)?, Outcome::Local))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use shorted_ai::providers::{AiError, HttpResponse, Secret};

    use super::*;
    use crate::ai_access::tests::MemoryKeys;
    use crate::ai_access::{save_settings, AiSettings};

    struct Mock {
        answers: RefCell<Vec<String>>,
        sent: RefCell<Vec<HttpRequest>>,
    }

    impl Http for Mock {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, AiError> {
            self.sent.borrow_mut().push(request.clone());
            let plan = self.answers.borrow_mut().remove(0);
            let body = serde_json::json!({
                "choices": [{"message": {"content": plan}}]
            });
            Ok(HttpResponse {
                status: 200,
                body: body.to_string(),
            })
        }
    }

    fn setup() -> (tempfile::TempDir, MemoryKeys, Project) {
        let root = tempfile::tempdir().unwrap();
        save_settings(
            root.path(),
            &AiSettings {
                provider: Some("openai".into()),
                model: "gpt-test".into(),
                base_url: String::new(),
            },
        )
        .unwrap();
        let keys = MemoryKeys::default();
        keys.set("openai", &Secret::new("sk-mock".into())).unwrap();
        let project =
            Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap();
        (root, keys, project)
    }

    fn mock(answers: &[&str]) -> Mock {
        Mock {
            answers: RefCell::new(answers.iter().map(|a| (*a).to_string()).collect()),
            sent: RefCell::new(Vec::new()),
        }
    }

    const DROP_C2: &str = r#"{"version":1,"source":"ai","summary":"Shorter.","ops":[{"op":"drop","clip":"c2","reason":"retake"}]}"#;

    #[test]
    fn the_payload_shown_is_the_payload_sent() {
        let (root, keys, project) = setup();
        let prepared = prepare(root.path(), &keys, &project, "fr").unwrap();
        let shown = prepared.request.body.clone().unwrap();
        assert!(shown.contains("c1"), "{shown}");
        assert!(shown.contains("French"));
        assert!(!shown.contains("sk-mock"));
        for media in &project.media {
            if let shorted_core::model::MediaLocation::External { abs_path, .. } = &media.location {
                assert!(!shown.contains(abs_path.as_str()), "a path is sent");
            }
            assert!(!shown.contains(&media.id.to_string()), "an id is sent");
        }
        let http = mock(&[DROP_C2]);
        let (plan, outcome) = run(&prepared, &http, &project, root.path(), None).unwrap();
        assert_eq!(outcome, Outcome::Ai);
        assert_eq!(plan.summary, "Shorter.");
        let sent = http.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].body.as_deref(), Some(shown.as_str()));
        assert_eq!(sent[0].url, prepared.request.url);
    }

    #[test]
    fn an_invalid_plan_is_repaired_once_then_the_local_plan_stands_in() {
        let (root, keys, project) = setup();
        let prepared = prepare(root.path(), &keys, &project, "en").unwrap();
        let bad = r#"{"version":1,"source":"ai","ops":[{"op":"drop","clip":"c99"}]}"#;

        let fixed = mock(&[bad, DROP_C2]);
        let (_, outcome) = run(&prepared, &fixed, &project, root.path(), None).unwrap();
        assert_eq!(outcome, Outcome::Repaired);
        let repair = fixed.sent.borrow()[1].body.clone().unwrap();
        assert!(
            repair.contains("unknown clip"),
            "the errors go back to the model"
        );

        let stubborn = mock(&[bad, bad]);
        let (plan, outcome) = run(&prepared, &stubborn, &project, root.path(), None).unwrap();
        assert_eq!(outcome, Outcome::Local);
        let shorted_core::command::Command::Group { label, .. } = &plan.command else {
            panic!("not a group");
        };
        assert_eq!(label, shorted_ai::plan::LOCAL_LABEL);
        assert_eq!(stubborn.sent.borrow().len(), 2);
    }
}
