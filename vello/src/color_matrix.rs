// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A self-contained GPU color-matrix post-process utility.
//!
//! This module is independent of Vello's main compute-based rendering
//! pipeline: it is a single fullscreen-triangle render pass, intended for use
//! cases like CSS `filter:` color functions (`grayscale()`, `sepia()`,
//! `saturate()`, `hue-rotate()`, `brightness()`, `contrast()`, `invert()`,
//! `opacity()`) and SVG's `feColorMatrix`, where a caller has already
//! rendered some content (e.g. a sub-[`Scene`](crate::Scene)) into an
//! offscreen texture and wants to remap its colors before compositing the
//! result back in, for example via
//! [`Renderer::override_image`](crate::Renderer::override_image).
//!
//! # Premultiplication convention
//!
//! Like [`BlurPipeline`](crate::blur::BlurPipeline), this pipeline reads and
//! writes textures in **straight (unpremultiplied) alpha**. The matrix is
//! applied directly to the sampled RGBA value with no conversion, so the two
//! pipelines can be freely chained (e.g. blur then color-matrix, or vice
//! versa) without either one needing to premultiply or unpremultiply in
//! between.
//!
//! Typical usage:
//!
//! ```ignore
//! let color_matrix = ColorMatrixPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm);
//! let mut encoder = device.create_command_encoder(&Default::default());
//! // Standard luma-weighted grayscale matrix.
//! let matrix = [
//!     0.2126, 0.7152, 0.0722, 0.0, 0.0,
//!     0.2126, 0.7152, 0.0722, 0.0, 0.0,
//!     0.2126, 0.7152, 0.0722, 0.0, 0.0,
//!     0.0,    0.0,    0.0,    1.0, 0.0,
//! ];
//! color_matrix.apply(&device, &mut encoder, &src_view, &dst_texture, width, height, &matrix);
//! queue.submit(Some(encoder.finish()));
//! ```

use wgpu::util::DeviceExt as _;
use wgpu::{BindGroupLayout, Device, RenderPipeline, Sampler, Texture, TextureFormat, TextureView};

/// GPU-side representation of a 4x5 color matrix, matching `ColorMatrixParams`
/// in `color_matrix.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ColorMatrixParamsGpu {
    row_r: [f32; 4],
    row_g: [f32; 4],
    row_b: [f32; 4],
    row_a: [f32; 4],
    offset: [f32; 4],
}

/// A cached GPU color-matrix post-process pipeline.
///
/// Create one `ColorMatrixPipeline` per output texture format you need to
/// process and reuse it across frames.
///
/// See the [module-level docs](self) for the premultiplication convention.
pub struct ColorMatrixPipeline {
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
    #[expect(
        dead_code,
        reason = "kept for parity with BlurPipeline and possible future use \
                  (e.g. validating dst format against the format passed to `new`)"
    )]
    format: TextureFormat,
}

impl ColorMatrixPipeline {
    /// Creates a new color-matrix pipeline for textures of the given
    /// `format`.
    ///
    /// `format` must be usable both as a texture binding (sampled) and as a
    /// render attachment (e.g. `Rgba8Unorm`, `Bgra8Unorm`).
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vello::color_matrix shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color_matrix.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vello::color_matrix bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vello::color_matrix pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vello::color_matrix pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vello::color_matrix sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            format,
        }
    }

    /// Applies a CSS/SVG-style 4x5 color matrix to `src`, writing the result
    /// into `dst`.
    ///
    /// `matrix` is row-major: four rows (R, G, B, A output channels) of five
    /// values each, `[r, g, b, a, offset]`, i.e. the same layout as SVG's
    /// `feColorMatrix type="matrix"` and CSS Filter Effects' matrix
    /// representation of `grayscale()`/`sepia()`/`saturate()`/
    /// `hue-rotate()`/`brightness()`/`contrast()`/`invert()`/`opacity()`.
    /// Each output channel is `dot(row[0..4], input_rgba) + row[4]`.
    ///
    /// - `src` is sampled as the input; it is not modified.
    /// - `dst` receives the result and must have
    ///   [`wgpu::TextureUsages::RENDER_ATTACHMENT`] usage and a format
    ///   matching the one passed to [`Self::new`].
    /// - `width`/`height` are the pixel dimensions to process; they should
    ///   match both textures' sizes.
    /// - Output is clamped to `[0.0, 1.0]` per channel.
    ///
    /// Both `src` and `dst` are treated as straight (unpremultiplied) alpha;
    /// see the [module-level docs](self).
    pub fn apply(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &TextureView,
        dst: &Texture,
        width: u32,
        height: u32,
        matrix: &[f32; 20],
    ) {
        if width == 0 || height == 0 {
            return;
        }

        let params = ColorMatrixParamsGpu {
            row_r: [matrix[0], matrix[1], matrix[2], matrix[3]],
            row_g: [matrix[5], matrix[6], matrix[7], matrix[8]],
            row_b: [matrix[10], matrix[11], matrix[12], matrix[13]],
            row_a: [matrix[15], matrix[16], matrix[17], matrix[18]],
            offset: [matrix[4], matrix[9], matrix[14], matrix[19]],
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vello::color_matrix params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vello::color_matrix bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vello::color_matrix pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
