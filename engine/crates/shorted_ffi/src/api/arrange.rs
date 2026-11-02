//! Smart arrange (docs/07): the local plan is made in the background and waits for review.

use crate::derush::{pending, plan_locally};
use crate::error::EngineError;
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
