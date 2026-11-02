struct Globals { time: f32, aspect: f32, uv_scale: vec2<f32>, rotated: f32 };
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var text_texture: texture_2d<f32>;
@group(0) @binding(2) var text_sampler: sampler;

struct BackgroundOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn background_vs(@builtin(vertex_index) index: u32) -> BackgroundOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return BackgroundOut(vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0), uv);
}

// Cost stand-in for sampling and colour-converting a decoded video frame.
@fragment
fn background_fs(in: BackgroundOut) -> @location(0) vec4<f32> {
    let t = globals.time;
    let wave = sin(in.uv.x * 9.0 + t) * cos(in.uv.y * 13.0 - t * 0.7);
    let grain = fract(sin(dot(in.uv + t, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    let base = vec3<f32>(0.12 + 0.1 * wave, 0.16 + 0.12 * in.uv.y, 0.3 + 0.15 * wave);
    return vec4<f32>(base + grain * 0.04, 1.0);
}

@group(1) @binding(0) var video_texture: texture_2d<f32>;
@group(1) @binding(1) var video_sampler: sampler;

// Centre-crops the clip to fill the frame, applying its rotation metadata.
@fragment
fn video_fs(in: BackgroundOut) -> @location(0) vec4<f32> {
    var uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    if (globals.rotated > 0.5) {
        uv = vec2<f32>(uv.y, 1.0 - uv.x);
    }
    let cropped = (uv - 0.5) * globals.uv_scale + 0.5;
    return vec4<f32>(textureSample(video_texture, video_sampler, cropped).rgb, 1.0);
}

struct QuadInstance {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) rotation: f32,
    @location(3) textured: f32,
    @location(4) color: vec4<f32>,
};

struct QuadOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) textured: f32,
    @location(2) color: vec4<f32>,
};

@vertex
fn quad_vs(@builtin(vertex_index) index: u32, quad: QuadInstance) -> QuadOut {
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let local = (corner * 2.0 - 1.0) * quad.half_size;
    let c = cos(quad.rotation);
    let s = sin(quad.rotation);
    let rotated = vec2<f32>(local.x * c - local.y * s, (local.x * s + local.y * c) * globals.aspect);
    return QuadOut(
        vec4<f32>(quad.center + rotated, 0.0, 1.0),
        vec2<f32>(corner.x, 1.0 - corner.y),
        quad.textured,
        quad.color,
    );
}

@fragment
fn quad_fs(in: QuadOut) -> @location(0) vec4<f32> {
    if (in.textured > 0.5) {
        let coverage = textureSample(text_texture, text_sampler, in.uv).r;
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }
    return in.color;
}
