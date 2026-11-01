//! Public API exposed to Dart. Generated bindings live in `app/lib/bridge/`.

use std::thread;
use std::time::Duration;

use crate::frb_generated::StreamSink;

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
