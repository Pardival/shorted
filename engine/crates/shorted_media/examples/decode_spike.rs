//! T-0006 measurement: decode speed and scrub latency. Prints JSON.
//! `cargo run --release -p shorted_media --example decode_spike -- <clip>`

#![allow(clippy::expect_used)] // Measurement tool: failing loudly is the right behaviour.

#[cfg(target_vendor = "apple")]
fn main() {
    use std::time::Instant;

    use shorted_media::apple::AppleBackend;
    use shorted_media::{MediaBackend, VideoDecoder};

    const SEEKS: i64 = 40;

    let path = std::env::args().nth(1).expect("usage: decode_spike <clip>");
    let path = std::path::Path::new(&path);
    let info = AppleBackend.probe(path).expect("probe");
    let mut decoder = AppleBackend.open_decoder(path).expect("open");

    let start = Instant::now();
    let mut frames = 0u32;
    let mut slowest_ms: f64 = 0.0;
    loop {
        let begin = Instant::now();
        if decoder.next_frame().expect("decode").is_none() {
            break;
        }
        slowest_ms = slowest_ms.max(begin.elapsed().as_secs_f64() * 1000.0);
        frames += 1;
    }
    let fps = f64::from(frames) / start.elapsed().as_secs_f64();

    // Pseudo-random targets, so that seeks are not served by the previous position.
    let mut latencies: Vec<f64> = (0..SEEKS)
        .map(|index| {
            let target = (index * 7_919_000 + 1_000_000) % info.duration_us.max(1);
            let begin = Instant::now();
            decoder.seek(target).expect("seek");
            decoder
                .next_frame()
                .expect("decode")
                .expect("frame after seek");
            begin.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    latencies.sort_by(f64::total_cmp);
    let percentile =
        |fraction: f64| latencies[((latencies.len() - 1) as f64 * fraction).round() as usize];
    println!(
        "{{\"frames\":{frames},\"decodeFps\":{fps:.1},\"slowestFrameMs\":{slowest_ms:.2},\"seekMsP50\":{:.2},\"seekMsP95\":{:.2},\"seekMsMax\":{:.2}}}",
        percentile(0.5),
        percentile(0.95),
        percentile(1.0)
    );
}

#[cfg(not(target_vendor = "apple"))]
fn main() {}
