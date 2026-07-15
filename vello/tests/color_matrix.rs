// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for [`vello::color_matrix::ColorMatrixPipeline`].
//!
//! This is a standalone `#[test]` (rather than living in `vello_tests`) so it
//! doesn't depend on that crate's `scenes`-based snapshot machinery: it just
//! creates a `wgpu` device via [`vello::util::RenderContext`] and skips
//! gracefully if no compatible GPU adapter is available.

use vello::color_matrix::ColorMatrixPipeline;
use vello::util::RenderContext;
use vello::wgpu::{
    self, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TexelCopyBufferInfo,
    TextureDescriptor, TextureFormat, TextureUsages,
};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 8;

/// Renders a solid pure-red texture, applies `matrix` via
/// [`ColorMatrixPipeline`], and reads the result back to the CPU.
///
/// Returns `None` if no compatible GPU adapter is available, so the calling
/// test can skip gracefully rather than failing in GPU-less environments.
async fn render_red_and_apply(matrix: &[f32; 20]) -> Option<Vec<[u8; 4]>> {
    let mut context = RenderContext::new();
    let device_id = context.device(None).await?;
    let device_handle = &context.devices[device_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    let format = TextureFormat::Rgba8Unorm;

    // Solid opaque pure red.
    let mut pixels = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH + x) * 4) as usize;
            pixels[idx] = 255;
            pixels[idx + 1] = 0;
            pixels[idx + 2] = 0;
            pixels[idx + 3] = 255;
        }
    }

    let extent = Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };

    let src = device.create_texture(&TextureDescriptor {
        label: Some("color_matrix test src"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        src.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * 4),
            rows_per_image: None,
        },
        extent,
    );
    let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());

    let dst = device.create_texture(&TextureDescriptor {
        label: Some("color_matrix test dst"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let color_matrix = ColorMatrixPipeline::new(device, format);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("color_matrix test encoder"),
    });
    color_matrix.apply(device, &mut encoder, &src_view, &dst, WIDTH, HEIGHT, matrix);

    let padded_byte_width = (WIDTH * 4).next_multiple_of(256);
    let buffer_size = padded_byte_width as u64 * HEIGHT as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("color_matrix test readback"),
        size: buffer_size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        dst.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_byte_width),
                rows_per_image: None,
            },
        },
        extent,
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        tx.send(res).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();

    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for y in 0..HEIGHT {
        let row_start = (y * padded_byte_width) as usize;
        for x in 0..WIDTH {
            let idx = row_start + (x * 4) as usize;
            out.push([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]);
        }
    }
    drop(data);
    buffer.unmap();

    Some(out)
}

const IDENTITY: [f32; 20] = [
    1.0, 0.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0, //
];

// Standard Rec. 709 luma weights, replicated into each output row so R, G,
// and B all end up equal to the luma value (a standard grayscale matrix).
const GRAYSCALE: [f32; 20] = [
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0, //
];

// Inverts each color channel (scale -1, offset 1) and leaves alpha alone.
const INVERT: [f32; 20] = [
    -1.0, 0.0, 0.0, 0.0, 1.0, //
    0.0, -1.0, 0.0, 0.0, 1.0, //
    0.0, 0.0, -1.0, 0.0, 1.0, //
    0.0, 0.0, 0.0, 1.0, 0.0, //
];

fn pixel(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
    pixels[(y * WIDTH + x) as usize]
}

#[test]
fn identity_matrix_is_no_op() {
    let Some(pixels) = pollster::block_on(render_red_and_apply(&IDENTITY)) else {
        eprintln!("Skipping identity_matrix_is_no_op: no compatible GPU adapter");
        return;
    };

    let p = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
    assert_eq!(
        p,
        [255, 0, 0, 255],
        "expected identity matrix to leave pure red unchanged, got {p:?}"
    );
}

#[test]
fn grayscale_matrix_maps_red_to_luma_gray() {
    let Some(pixels) = pollster::block_on(render_red_and_apply(&GRAYSCALE)) else {
        eprintln!("Skipping grayscale_matrix_maps_red_to_luma_gray: no compatible GPU adapter");
        return;
    };

    let p = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
    // Pure red (1,0,0) through the luma matrix gives R=G=B=0.2126, which in
    // 8-bit sRGB-less linear unorm storage rounds to ~54.
    const EXPECTED: u8 = 54;
    assert!(
        p[0].abs_diff(EXPECTED) <= 2,
        "expected R channel near {EXPECTED}, got {p:?}"
    );
    assert_eq!(
        p[0], p[1],
        "expected grayscale output to have R == G, got {p:?}"
    );
    assert_eq!(
        p[1], p[2],
        "expected grayscale output to have G == B, got {p:?}"
    );
    assert_eq!(p[3], 255, "expected alpha to remain untouched, got {p:?}");
}

#[test]
fn invert_matrix_with_offset_inverts_color() {
    let Some(pixels) = pollster::block_on(render_red_and_apply(&INVERT)) else {
        eprintln!("Skipping invert_matrix_with_offset_inverts_color: no compatible GPU adapter");
        return;
    };

    let p = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
    // Pure red (1,0,0) inverted (scale -1, offset 1) becomes cyan (0,1,1).
    assert_eq!(
        p,
        [0, 255, 255, 255],
        "expected inverted red to be cyan, got {p:?}"
    );
}
