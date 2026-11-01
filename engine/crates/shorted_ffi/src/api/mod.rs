//! Public API exposed to Dart. Generated bindings live in `app/lib/bridge/`.

pub mod clips;
pub mod export;
pub mod media;
pub mod preview;
pub mod projects;

use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::error::{EngineError, EngineFailure};
use crate::frb_generated::StreamSink;
use crate::runtime::{self, Priority, TaskEvent};

const DEMO_TASK_STEPS: u32 = 100;

/// Version of the engine.
#[flutter_rust_bridge::frb(sync)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Adds two numbers off the UI isolate; proves async calls work end to end.
pub async fn add(left: i64, right: i64) -> i64 {
    left + right
}

/// Streams 0, 1, 2… every `interval_ms`, until Dart cancels the subscription.
pub fn counter_stream(interval_ms: u32, sink: StreamSink<u64>) {
    thread::spawn(move || {
        let mut value = 0;
        while sink.add(value).is_ok() {
            value += 1;
            thread::sleep(Duration::from_millis(u64::from(interval_ms)));
        }
    });
}

/// Starts file logging in `log_dir`. Calling it again has no effect.
pub fn init_engine(log_dir: String) -> Result<(), EngineFailure> {
    crate::logging::init(Path::new(&log_dir)).map_err(EngineFailure::from)
}

/// Runs a cancellable background task that sleeps through `duration_ms` in steps, emitting
/// progress. Stands in for real long tasks (analysis, export) until they exist. The first event
/// carries the id to pass to [`cancel_task`].
pub fn start_demo_task(duration_ms: u32, events: StreamSink<TaskEvent>) {
    let step = Duration::from_millis(u64::from(duration_ms / DEMO_TASK_STEPS));
    runtime::global()
        .background
        .spawn(Priority::Normal, events, move |task| {
            for done in 1..=DEMO_TASK_STEPS {
                task.check_cancelled()?;
                thread::sleep(step);
                task.progress(f64::from(done) / f64::from(DEMO_TASK_STEPS));
            }
            Ok::<(), EngineError>(())
        });
}

/// Requests cancellation of a background task. Unknown ids are ignored: the task may have ended.
#[flutter_rust_bridge::frb(sync)]
pub fn cancel_task(task_id: u64) {
    runtime::global().background.cancel(task_id);
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_version_is_the_package_version() {
        assert_eq!(engine_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn add_sums_on_the_async_path() {
        let result = flutter_rust_bridge::for_generated::futures::executor::block_on(add(2, 3));
        assert_eq!(result, 5);
    }
}
