// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// A single fullscreen-triangle render pass that applies a CSS/SVG-style 4x5
// color matrix to unpremultiplied (straight-alpha) RGBA input, matching the
// convention used by `blur.wgsl`.

struct ColorMatrixParams {
    // Row-major 4x5 matrix, one vec4<f32> per output channel (R, G, B, A)
    // holding that channel's [r, g, b, a] weights.
    row_r: vec4<f32>,
    row_g: vec4<f32>,
    row_b: vec4<f32>,
    row_a: vec4<f32>,
    // Per-channel offsets (added after the matrix multiply), in [r, g, b, a]
    // order.
    offset: vec4<f32>,
}

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> params: ColorMatrixParams;

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
    // The source texture is stored as straight (unpremultiplied) alpha, the
    // same convention `blur.wgsl` reads and writes, so the matrix can be
    // applied directly to the sampled value with no premultiply conversion.
    let straight = textureSample(src_texture, src_sampler, in.uv);

    let result = vec4<f32>(
        dot(params.row_r, straight) + params.offset.x,
        dot(params.row_g, straight) + params.offset.y,
        dot(params.row_b, straight) + params.offset.z,
        dot(params.row_a, straight) + params.offset.w,
    );
    return clamp(result, vec4<f32>(0.0), vec4<f32>(1.0));
}
