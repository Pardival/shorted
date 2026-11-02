struct Layer {
    // Maps the frame's 0..1 into the source's 0..1, once rotation is applied.
    scale: vec2<f32>,
    offset: vec2<f32>,
    quarter_turns: u32,
    opacity: f32,
    // Moves the whole picture across the frame, in frame widths and heights.
    shift: vec2<f32>,
    // Colour to mix in, with its amount in alpha.
    tint: vec4<f32>,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    // Enlarges the whole picture around the middle of the frame.
    frame_zoom: f32,
};
@group(0) @binding(0) var<uniform> layer: Layer;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Out {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0), vec2<f32>(corner.x, 1.0 - corner.y));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let moved = (in.uv - 0.5) / layer.frame_zoom + 0.5 - layer.shift;
    if any(moved < vec2<f32>(0.0)) || any(moved > vec2<f32>(1.0)) {
        discard;
    }
    let framed = (moved - 0.5) * layer.scale + layer.offset;
    var uv = framed;
    // The stored picture turned clockwise gives the displayed one: go back the other way.
    switch layer.quarter_turns {
        case 1u: { uv = vec2<f32>(framed.y, 1.0 - framed.x); }
        case 2u: { uv = vec2<f32>(1.0 - framed.x, 1.0 - framed.y); }
        case 3u: { uv = vec2<f32>(1.0 - framed.y, framed.x); }
        default: {}
    }
    var color = textureSample(source, source_sampler, uv).rgb;
    color = (color + layer.brightness - 0.5) * layer.contrast + 0.5;
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, layer.saturation);
    color = mix(color, layer.tint.rgb, layer.tint.a);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), layer.opacity);
}
