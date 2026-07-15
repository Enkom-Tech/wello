// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for [`vello::blur::BlurPipeline`].
//!
//! This is a standalone `#[test]` (rather than living in `vello_tests`) so it
//! doesn't depend on that crate's `scenes`-based snapshot machinery: it just
//! creates a `wgpu` device via [`vello::util::RenderContext`] and skips
//! gracefully if no compatible GPU adapter is available.

use vello::blur::BlurPipeline;
use vello::util::RenderContext;
use vello::wgpu::{
    self, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, TexelCopyBufferInfo,
    TextureDescriptor, TextureFormat, TextureUsages,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

/// Renders a sharp vertical black/white edge into a source texture, blurs it,
/// and reads the result back to the CPU.
///
/// Returns `None` if no compatible GPU adapter is available, so the calling
/// test can skip gracefully rather than failing in GPU-less environments.
async fn render_edge_and_blur(sigma: f32) -> Option<Vec<[u8; 4]>> {
    let mut context = RenderContext::new();
    let device_id = context.device(None).await?;
    let device_handle = &context.devices[device_id];
    let device = &device_handle.device;
    let queue = &device_handle.queue;

    let format = TextureFormat::Rgba8Unorm;

    // Left half opaque black, right half opaque white, with a sharp edge at
    // the midline.
    let mut pixels = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let idx = ((y * WIDTH + x) * 4) as usize;
            let value = if x < WIDTH / 2 { 0_u8 } else { 255_u8 };
            pixels[idx] = value;
            pixels[idx + 1] = value;
            pixels[idx + 2] = value;
            pixels[idx + 3] = 255;
        }
    }

    let extent = Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };

    let src = device.create_texture(&TextureDescriptor {
        label: Some("blur test src"),
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
        label: Some("blur test dst"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let mut blur = BlurPipeline::new(device, format);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("blur test encoder"),
    });
    blur.blur(device, &mut encoder, &src_view, &dst, WIDTH, HEIGHT, sigma);

    let padded_byte_width = (WIDTH * 4).next_multiple_of(256);
    let buffer_size = padded_byte_width as u64 * HEIGHT as u64;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("blur test readback"),
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

fn pixel(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
    pixels[(y * WIDTH + x) as usize]
}

#[test]
fn gaussian_blur_smooths_sharp_edge() {
    let Some(pixels) = pollster::block_on(render_edge_and_blur(4.0)) else {
        eprintln!("Skipping gaussian_blur_smooths_sharp_edge: no compatible GPU adapter");
        return;
    };

    let mid_y = HEIGHT / 2;

    // Far from the edge, the blur shouldn't meaningfully change the color:
    // still close to black on the left, close to white on the right.
    let far_left = pixel(&pixels, 2, mid_y);
    let far_right = pixel(&pixels, WIDTH - 3, mid_y);
    assert!(
        far_left[0] < 40,
        "expected far-left pixel to stay dark, got {far_left:?}"
    );
    assert!(
        far_right[0] > 215,
        "expected far-right pixel to stay light, got {far_right:?}"
    );

    // Right at the edge, blurring should produce an intermediate gray value,
    // clearly between the two extremes.
    let at_edge = pixel(&pixels, WIDTH / 2, mid_y);
    assert!(
        at_edge[0] > 40 && at_edge[0] < 215,
        "expected edge pixel to be smoothed to an intermediate value, got {at_edge:?}"
    );

    // Alpha should remain fully opaque everywhere (all 4 channels are
    // blurred consistently; no channel is dropped or left unprocessed).
    assert_eq!(at_edge[3], 255);
    assert_eq!(far_left[3], 255);
    assert_eq!(far_right[3], 255);
}

#[test]
fn zero_sigma_is_identity() {
    let Some(pixels) = pollster::block_on(render_edge_and_blur(0.0)) else {
        eprintln!("Skipping zero_sigma_is_identity: no compatible GPU adapter");
        return;
    };

    let mid_y = HEIGHT / 2;
    // With sigma <= 0, the edge should remain sharp: the last pixel before
    // the midline is still black, the first pixel at/after it is white.
    let last_black = pixel(&pixels, WIDTH / 2 - 1, mid_y);
    let first_white = pixel(&pixels, WIDTH / 2, mid_y);
    assert!(
        last_black[0] < 10,
        "expected identity copy to preserve sharp edge, got {last_black:?}"
    );
    assert!(
        first_white[0] > 245,
        "expected identity copy to preserve sharp edge, got {first_white:?}"
    );
}
