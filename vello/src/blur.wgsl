// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// A single-pass separable Gaussian blur, run once horizontally and once
// vertically by `BlurPipeline`. Samples are taken along `direction` (a unit
// vector in normalized texel units) using precomputed weights.

const MAX_TAPS: u32 = 65u;

struct BlurParams {
    // (dx, dy) offset between adjacent taps, in UV space.
    direction: vec2<f32>,
    // Number of taps actually used (<= MAX_TAPS), packed with padding.
    tap_count: u32,
    _padding: u32,
    // Gaussian weights, one per tap, symmetric kernel centered at tap 0.
    weights: array<vec4<f32>, MAX_TAPS>,
}

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> params: BlurParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Fullscreen triangle: covers the viewport with a single triangle so no
// vertex buffer is needed.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Tap 0 is the center weight.
    var sum = textureSample(src_texture, src_sampler, in.uv) * params.weights[0].x;
    let tap_count = params.tap_count;
    for (var i: u32 = 1u; i < tap_count; i += 1u) {
        let offset = params.direction * f32(i);
        let weight = params.weights[i].x;
        let sample_pos = textureSample(src_texture, src_sampler, in.uv + offset);
        let sample_neg = textureSample(src_texture, src_sampler, in.uv - offset);
        sum += (sample_pos + sample_neg) * weight;
    }
    return sum;
}
