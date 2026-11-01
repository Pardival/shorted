//! Transport of the preview. These calls only touch a small shared state, so they are synchronous
//! and safe on the UI isolate: the frames themselves are made on the texture's own queue.

use crate::transport::transport;

#[flutter_rust_bridge::frb(sync)]
pub fn preview_seek(time_us: i64) {
    transport().set_position(time_us);
}

#[flutter_rust_bridge::frb(sync)]
pub fn preview_play() {
    transport().play();
}

#[flutter_rust_bridge::frb(sync)]
pub fn preview_pause() {
    transport().pause();
}

#[flutter_rust_bridge::frb(sync)]
pub fn preview_position_us() -> i64 {
    transport().position_us()
}

#[flutter_rust_bridge::frb(sync)]
pub fn preview_is_playing() -> bool {
    transport().playing()
}
