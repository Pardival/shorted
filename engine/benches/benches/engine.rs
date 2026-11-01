//! Render cost of the spike scene. Skipped on machines without a GPU.

#![allow(clippy::expect_used)] // Benchmark tool: failing loudly is the right behaviour.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use shorted_render::spike::{RenderError, SpikeRenderer};

const WIDTH: u32 = 1080;
const HEIGHT: u32 = 1920;
const BYTES_PER_PIXEL: usize = 4;

fn render_scene(c: &mut Criterion) {
    let mut renderer = match SpikeRenderer::new(WIDTH, HEIGHT) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter(_)) => return,
        Err(error) => panic!("{error}"),
    };
    let stride = WIDTH as usize * BYTES_PER_PIXEL;
    let mut pixels = vec![0; stride * HEIGHT as usize];
    let mut time = 0.0;
    c.bench_function("render_1080p_scene_pixel_copy", |b| {
        b.iter(|| {
            time += 1.0 / 60.0;
            renderer
                .render_to_bytes(time, &mut pixels, stride)
                .expect("render");
            black_box(&pixels);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = render_scene
}
criterion_main!(benches);
