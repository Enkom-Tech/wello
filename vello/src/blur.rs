// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A self-contained, separable Gaussian blur post-process utility.
//!
//! This module is independent of Vello's main compute-based rendering
//! pipeline: it is a small pair of fullscreen-triangle render passes,
//! intended for use cases like CSS `backdrop-filter: blur(...)` /
//! `filter: blur(...)`, where a caller has already rendered some content
//! (e.g. a sub-[`Scene`](crate::Scene)) into an offscreen texture and wants
//! to blur it before compositing the result back in, for example via
//! [`Renderer::override_image`](crate::Renderer::override_image).
//!
//! Typical usage:
//!
//! ```ignore
//! let mut blur = BlurPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm);
//! let mut encoder = device.create_command_encoder(&Default::default());
//! blur.blur(&device, &mut encoder, &src_view, &dst_texture, width, height, 8.0);
//! queue.submit(Some(encoder.finish()));
//! ```
//!
//! The blur is performed as two passes (horizontal, then vertical) using an
//! internal intermediate texture that is cached and only reallocated when
//! the requested size changes.

use wgpu::util::DeviceExt as _;
use wgpu::{BindGroupLayout, Device, RenderPipeline, Sampler, Texture, TextureFormat, TextureView};

/// Maximum number of one-sided taps (including the center tap) supported by
/// the blur shader. This bounds the uniform buffer size and the per-pixel
/// cost of the blur; larger sigmas are handled by capping the kernel radius,
/// which trades a small amount of accuracy for a bounded cost.
const MAX_TAPS: usize = 65;

/// A cached separable-Gaussian-blur post-process pipeline.
///
/// Create one `BlurPipeline` per output texture format you need to blur into
/// and reuse it across frames; it internally caches the intermediate texture
/// used between the horizontal and vertical passes.
pub struct BlurPipeline {
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
    format: TextureFormat,
    /// Cached intermediate texture (horizontal-pass output / vertical-pass
    /// input), reallocated on demand when the requested size grows or the
    /// format changes.
    intermediate: Option<(Texture, TextureView, u32, u32)>,
}

/// GPU-side representation of the blur kernel, matching `BlurParams` in
/// `blur.wgsl`.
///
/// `weights` stores one Gaussian weight per tap in the `x` component of each
/// `[f32; 4]` entry; the rest is padding required to satisfy WGSL's uniform
/// buffer array-stride alignment rules (16 bytes per array element).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParamsGpu {
    direction: [f32; 2],
    tap_count: u32,
    _padding: u32,
    weights: [[f32; 4]; MAX_TAPS],
}

impl BlurPipeline {
    /// Creates a new blur pipeline for textures of the given `format`.
    ///
    /// `format` must be usable both as a texture binding (sampled) and as a
    /// render attachment (e.g. `Rgba8Unorm`, `Bgra8Unorm`).
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vello::blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vello::blur bind group layout"),
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
            label: Some("vello::blur pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vello::blur pipeline"),
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
            label: Some("vello::blur sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            format,
            intermediate: None,
        }
    }

    /// Performs a two-pass separable Gaussian blur of `src` into `dst`.
    ///
    /// - `src` is sampled as the input; it is not modified.
    /// - `dst` receives the fully blurred result and must have
    ///   [`wgpu::TextureUsages::RENDER_ATTACHMENT`] usage and a format
    ///   matching the one passed to [`Self::new`].
    /// - `width`/`height` are the pixel dimensions to process; they should
    ///   match both textures' sizes.
    /// - `sigma` is the Gaussian standard deviation, in pixels. `sigma <= 0.0`
    ///   is treated as an identity copy of `src` into `dst`.
    ///
    /// All four channels (including alpha) are blurred, which is the
    /// correct behavior for blurring content with transparent backgrounds
    /// (e.g. an offscreen sub-scene that will be composited afterwards).
    pub fn blur(
        &mut self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &TextureView,
        dst: &Texture,
        width: u32,
        height: u32,
        sigma: f32,
    ) {
        if width == 0 || height == 0 {
            return;
        }

        if sigma <= 0.0 {
            self.copy(device, encoder, src, dst, width, height);
            return;
        }

        let (weights, tap_count) = gaussian_weights(sigma);

        let intermediate_view = self.ensure_intermediate(device, width, height);

        // Horizontal pass: src -> intermediate.
        let h_params = BlurParamsGpu {
            direction: [1.0 / width as f32, 0.0],
            tap_count: tap_count as u32,
            _padding: 0,
            weights,
        };
        self.run_pass(
            device,
            encoder,
            src,
            &intermediate_view,
            &h_params,
            "horizontal",
        );

        // Vertical pass: intermediate -> dst.
        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let v_params = BlurParamsGpu {
            direction: [0.0, 1.0 / height as f32],
            tap_count: tap_count as u32,
            _padding: 0,
            weights,
        };
        self.run_pass(
            device,
            encoder,
            &intermediate_view,
            &dst_view,
            &v_params,
            "vertical",
        );
    }

    /// Ensures the cached intermediate texture is at least `width`x`height`,
    /// (re)allocating it if necessary, and returns its view.
    fn ensure_intermediate(&mut self, device: &Device, width: u32, height: u32) -> TextureView {
        let needs_new = match &self.intermediate {
            Some((_, _, w, h)) => *w != width || *h != height,
            None => true,
        };
        if needs_new {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vello::blur intermediate texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.intermediate = Some((texture, view, width, height));
        }
        self.intermediate.as_ref().unwrap().1.clone()
    }

    /// Runs a single directional blur pass, rendering `src` into `dst_view`
    /// using `params`.
    fn run_pass(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &TextureView,
        dst_view: &TextureView,
        params: &BlurParamsGpu,
        label: &str,
    ) {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vello::blur params"),
            contents: bytemuck::bytes_of(params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("vello::blur bind group"),
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

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("vello::blur {label} pass")),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
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

    /// Copies `src` into `dst` unchanged (used for the `sigma <= 0.0` case).
    fn copy(
        &self,
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &TextureView,
        dst: &Texture,
        width: u32,
        height: u32,
    ) {
        // A single-tap "blur" pass is a simple, format-agnostic way to copy
        // `src` (a `TextureView`, which may not directly correspond to a
        // whole `Texture` we could pass to `copy_texture_to_texture`) into
        // `dst`, reusing the existing pipeline instead of a second code path.
        let identity = BlurParamsGpu {
            direction: [0.0, 0.0],
            tap_count: 1,
            _padding: 0,
            weights: {
                let mut weights = [[0.0_f32; 4]; MAX_TAPS];
                weights[0] = [1.0, 0.0, 0.0, 0.0];
                weights
            },
        };
        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let _ = device;
        self.run_pass(device, encoder, src, &dst_view, &identity, "copy");
        let _ = (width, height);
    }
}

/// Computes normalized Gaussian weights for one side of a symmetric kernel
/// (including the center tap at index 0), for the given standard deviation.
///
/// The kernel radius is capped at `3 * sigma`, rounded up, and further
/// capped at `MAX_TAPS - 1` taps on each side to bound shader cost. Weights
/// are renormalized so the (conceptually two-sided) kernel sums to 1,
/// keeping overall brightness constant even when the radius is capped.
fn gaussian_weights(sigma: f32) -> ([[f32; 4]; MAX_TAPS], usize) {
    debug_assert!(sigma > 0.0);
    let radius = ((3.0 * sigma).ceil() as usize).clamp(1, MAX_TAPS - 1);
    let tap_count = radius + 1;

    let mut weights = [[0.0_f32; 4]; MAX_TAPS];
    let two_sigma_sq = 2.0 * sigma * sigma;
    let mut sum = 0.0_f32;
    for (i, weight) in weights.iter_mut().enumerate().take(tap_count) {
        let x = i as f32;
        let w = (-x * x / two_sigma_sq).exp();
        weight[0] = w;
        // Center tap (i == 0) counted once; all others counted twice since
        // the shader samples symmetrically at +offset and -offset.
        sum += if i == 0 { w } else { 2.0 * w };
    }
    if sum > 0.0 {
        for weight in weights.iter_mut().take(tap_count) {
            weight[0] /= sum;
        }
    }
    (weights, tap_count)
}
