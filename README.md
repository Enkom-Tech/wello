## About this repository

**[Enkom-Tech/wello](https://github.com/Enkom-Tech/wello)** is Enkom’s maintained fork of [linebender/vello](https://github.com/linebender/vello). We merge upstream selectively, keep Enkom-specific changes here when needed, and target the **WAPP** viewer/browser stack with **wgpu / naga 29.x** (see [WAPP-SDK](https://github.com/Enkom-Tech/wapp-sdk)). This is not a mirror of upstream. Upstream design discussion belongs on [Linebender’s repo](https://github.com/linebender/vello) and [Zulip](https://xi.zulipchat.com/#narrow/channel/197075-vello); use this repo for fork-specific issues and PRs.

The published Rust crate name remains **`vello`** (workspace version **0.8.0**). API-focused notes also live in [`vello/README.md`](vello/README.md).

### Workspace layout (high level)

| Area | Role |
|------|------|
| `vello`, `vello_encoding`, `vello_shaders` | Core GPU renderer and supporting crates |
| `examples/` | `with_winit`, `simple`, `headless`, WASM helpers, shared `scenes` |
| `sparse_strips/` | Experimental CPU / hybrid / sparse-strip pipelines (`vello_cpu`, `vello_hybrid`, …) |
| `glifo` | Glyph-related utilities used by the workspace |
| `image_filters/vello_filters_cpu` | CPU-side image filters |
| `vello_tests`, `xtask` | Tests and maintenance tooling |

### Use as a Cargo `git` dependency (no `[patch.crates-io]`)

Depend on the `vello` package from this repository. Cargo checks out the full workspace, so **`vello_encoding` and `vello_shaders` resolve from the same revision**—you do not need separate patches for those crate names.

```toml
[dependencies]
vello = { git = "https://github.com/Enkom-Tech/wello", package = "vello", rev = "<full-commit-sha>" }
# Or, for a moving target: branch = "main"  (not recommended for releases)
```

Pin **`rev`** to a commit SHA or use **`tag = "..."`** for reproducible builds. Align your own **`wgpu`** dependency with the version declared in this repo’s root `Cargo.toml` under `[workspace.dependencies]` (currently **29.0.1**) so types match across your app and Vello.

---

<div align="center">

# Wello (**vello** fork)

**A GPU compute-centric 2D renderer**

[![Linebender Zulip](https://img.shields.io/badge/Linebender-%23vello-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/197075-vello)
[![Upstream Vello (deps.rs)](https://deps.rs/repo/github/linebender/vello/status.svg)](https://deps.rs/repo/github/linebender/vello)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![wgpu version](https://img.shields.io/badge/wgpu-v29.0.1-orange.svg)](https://crates.io/crates/wgpu)

[![Crates.io](https://img.shields.io/crates/v/vello.svg)](https://crates.io/crates/vello)
[![Docs](https://docs.rs/vello/badge.svg)](https://docs.rs/vello)
[![Build status](https://github.com/Enkom-Tech/wello/workflows/CI/badge.svg)](https://github.com/Enkom-Tech/wello/actions)

</div>

**Wello** is a 2D graphics engine written in Rust, centered on GPU compute. It can draw large 2D scenes at interactive or near-interactive frame rates using [`wgpu`]. Behavior and goals follow upstream Vello; this fork adds the workspace above and Enkom integration work.

Quickstart to run an example program:

```shell
cargo run -p with_winit
```

![image](https://github.com/linebender/vello/assets/8573618/cc2b742e-2135-4b70-8051-c49aeddb5d19)

It is used as the rendering backend for [Xilem], a Rust GUI toolkit.

> [!WARNING]
> Like upstream Vello, Wello should be treated as **alpha-quality** for many production uses. Notable gaps include:
>
> - [Implementing blur and filter effects](https://github.com/linebender/vello/issues/476).
> - [Conflations artifacts](https://github.com/linebender/vello/issues/49).
> - [GPU memory allocation strategy](https://github.com/linebender/vello/issues/366)
> - [Glyph caching](https://github.com/linebender/vello/issues/204)

Significant changes are documented in [the changelog].

## Motivation

Wello (like Vello) sits in the same part of the stack as renderers such as [Skia](https://skia.org/), [Cairo](https://www.cairographics.org/), and [Piet](https://github.com/linebender/piet): shapes, images, gradients, text, and similar features through a PostScript-style API familiar from SVG and [the `<canvas>` 2D context](https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D).

The design favors GPU throughput: stages that are often CPU-bound or need extra textures in classic renderers are parallelized with prefix-style algorithms so more work stays on the GPU with fewer temporaries.

A capable GPU with **compute shader** support is required for the main (GPU) pipeline.

## Getting started

Wello is intended to sit deep in UI or document stacks. Building a [`Scene`](https://docs.rs/vello/latest/vello/struct.Scene.html) is straightforward; wiring [`wgpu`](https://wgpu.rs/) (device, queue, targets) is the heavier part.

A minimal sketch of rendering to a texture:

```rust
use vello::kurbo::{Affine, Circle};
use vello::peniko::{Color, Fill};
use vello::peniko::color::palette;
use vello::wgpu::{
    Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureViewDescriptor,
};
use vello::{AaConfig, Renderer, RendererOptions, RenderParams, Scene};

// Obtain `device` and `queue` from your wgpu setup (adapter, instance, etc.)
let (width, height) = (800_u32, 600_u32);
let device: vello::wgpu::Device = todo!();
let queue: vello::wgpu::Queue = todo!();

let mut renderer = Renderer::new(&device, RendererOptions::default())
    .expect("Failed to create renderer");

let mut scene = Scene::new();
scene.fill(
    Fill::NonZero,
    Affine::IDENTITY,
    Color::from_rgb8(242, 140, 168),
    None,
    &Circle::new((420.0, 200.0), 120.0),
);

let size = Extent3d {
    width,
    height,
    depth_or_array_layers: 1,
};
let target = device.create_texture(&TextureDescriptor {
    label: Some("Vello render target"),
    size,
    mip_level_count: 1,
    sample_count: 1,
    dimension: TextureDimension::D2,
    format: TextureFormat::Rgba8Unorm,
    usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
    view_formats: &[],
});
let target_view = target.create_view(&TextureViewDescriptor::default());

renderer
    .render_to_texture(
        &device,
        &queue,
        &scene,
        &target_view,
        &RenderParams {
            base_color: palette::css::BLACK,
            width,
            height,
            antialiasing_method: AaConfig::Msaa16,
        },
    )
    .expect("Failed to render to a texture");
// Read back or sample `target` / `target_view` as needed (see `examples/headless`)
```

Working examples live under [`examples/`](https://github.com/Enkom-Tech/wello/tree/main/examples) (winit, simple surface setup, headless, WASM).

## Performance

Upstream has reported on the order of **177 fps** for the paris-30k test scene on an M1 Max at 1600×1600 as a favorable case. Treat that as indicative, not a guarantee on your hardware or this fork’s exact revision.

## Integrations

### SVG

A separate Linebender integration for rendering SVG files is available through [`vello_svg`](https://github.com/linebender/vello_svg).

### Lottie

A separate Linebender integration for playing Lottie animations is available through [`velato`](https://github.com/linebender/velato).

### Bevy

A separate Linebender integration for rendering raw scenes or Lottie and SVG files in [Bevy] through [`bevy_vello`](https://github.com/linebender/bevy_vello).

## Examples

Examples are separate workspace packages under [`examples/`](https://github.com/Enkom-Tech/wello/tree/main/examples) so they can carry their own dependencies. Run them with Cargo’s `--package` / `-p` flag.

### Winit

The [winit] example ([`examples/with_winit`](https://github.com/Enkom-Tech/wello/tree/main/examples/with_winit)) renders to a native window. By default it shows the [GhostScript Tiger] and any SVGs you drop in [`examples/assets/downloads`](https://github.com/Enkom-Tech/wello/tree/main/examples/assets/downloads).
A custom list of SVG file paths (and directories to render all SVG files from) can be provided as arguments instead.
It also includes a collection of test scenes showing the capabilities of `vello`, which can be shown with `--test-scenes`.

```shell
cargo run -p with_winit
```

<!-- ### Headless -->

## Platforms

We aim to target all environments which can support WebGPU with the [default limits](https://www.w3.org/TR/webgpu/#limits).
We defer to [`wgpu`] for this support.
Other platforms are more tricky, and may require special building/running procedures.

### Web

Because the GPU pipeline relies on compute shaders, the web path depends on **WebGPU**. Support in Chrome is the most mature; Firefox and Safari are still catching up, so you may need preview builds or flags.

The following command builds and runs a web version of the [winit demo](#winit).
This uses [`cargo-run-wasm`](https://github.com/rukai/cargo-run-wasm) to build the example for web, and host a local server for it

```shell
# Make sure the Rust toolchain supports the wasm32 target
rustup target add wasm32-unknown-unknown

# The binary name must also be explicitly provided as it differs from the package name
cargo run_wasm -p with_winit --bin with_winit_bin
```

There is also a web demo [available here](https://linebender.github.io/vello) on supporting web browsers.

> [!WARNING]
> The web is not a primary target for upstream Vello; WebGPU implementations vary, so the WASM example may be fragile.

### Android

The [`with_winit`](#winit) example supports running on Android, using [cargo apk](https://crates.io/crates/cargo-apk).

```shell
cargo apk run -p with_winit --lib
```

> [!TIP]
> cargo apk doesn't support running in release mode without configuration.
> See [their crates page docs](https://crates.io/crates/cargo-apk) (around `package.metadata.android.signing.<profile>`).
>
> See also [cargo-apk#16](https://github.com/rust-mobile/cargo-apk/issues/16).
> To run in release mode, you must add the following to `examples/with_winit/Cargo.toml` (changing `$HOME` to your home directory):

```toml
[package.metadata.android.signing.release]
path = "$HOME/.android/debug.keystore"
keystore_password = "android"
```

> [!NOTE]
> As `cargo apk` does not allow passing command line arguments or environment variables to the app when ran, these can be embedded into the
> program at compile time (currently for Android only)
> `with_winit` currently supports the environment variables:
>
> - `VELLO_STATIC_LOG`, which is equivalent to `RUST_LOG`
> - `VELLO_STATIC_ARGS`, which is equivalent to passing in command line arguments

For example (with unix shell environment variable syntax):

```sh
VELLO_STATIC_LOG="vello=trace" VELLO_STATIC_ARGS="--test-scenes" cargo apk run -p with_winit --lib
```

## Minimum supported Rust Version (MSRV)

This workspace is verified against **Rust 1.92** and later (see `rust-version` in the root `Cargo.toml` and `RUST_MIN_VER` in [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

Future toolchain bumps may land without a semver-major release, consistent with upstream policy.

<details>
<summary>If compilation fails on an older toolchain</summary>

A dependency may have raised its own MSRV. If you cannot upgrade Rust, try pinning that dependency:

```sh
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

Upstream Vello is discussed on [Linebender Zulip](https://xi.zulipchat.com/) in [#vello](https://xi.zulipchat.com/#narrow/channel/197075-vello) (readable without an account).

For **Wello**-specific bugs, features, and PRs, use [Enkom-Tech/wello](https://github.com/Enkom-Tech/wello) on GitHub. The [Rust code of conduct] applies.

Unless you state otherwise, contributions you submit for inclusion are licensed as described under [License](#license).

## History

Upstream **Vello** was previously `piet-gpu`, built on a custom HAL (`piet-gpu-hal`) rather than [`wgpu`].

An archive of this version can be found in the branches [`custom-hal-archive-with-shaders`] and [`custom-hal-archive`].
This succeeded the previous prototype, [piet-metal], and included work adapted from [piet-dx12].

The decision to lay down `piet-gpu-hal` in favor of WebGPU is discussed in detail in the blog post [Requiem for piet-gpu-hal].

A [vision](https://github.com/linebender/vello/tree/main/doc/vision.md) document dated December 2020 explained the longer-term goals of the project, and how we might get there.
Many of these items are out-of-date or completed, but it still may provide some useful background.

## Related projects

The Vello lineage draws on ideas from projects such as:

- [Pathfinder](https://github.com/servo/pathfinder)
- [Spinel](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/master/src/graphics/lib/compute/spinel/)
- [Forma](https://github.com/google/forma)
- [Massively Parallel Vector Graphics](https://w3.impa.br/~diego/projects/GanEtAl14/)
- [Random-access rendering of general vector graphics](https://hhoppe.com/proj/ravg/)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

In addition, all files in [`vello_shaders/shader`](https://github.com/Enkom-Tech/wello/tree/main/vello_shaders/shader) and [`vello_shaders/src/cpu`](https://github.com/Enkom-Tech/wello/tree/main/vello_shaders/src/cpu) (and subdirectories) are alternatively licensed under the Unlicense ([`vello_shaders/shader/UNLICENSE`](https://github.com/Enkom-Tech/wello/tree/main/vello_shaders/shader/UNLICENSE) or <http://unlicense.org/>). Those files are also available under the Apache-2.0/MIT dual license above.

Assets under [`examples/assets`](https://github.com/Enkom-Tech/wello/tree/main/examples/assets) keep their own per-directory `LICENSE` files where present.

[piet-metal]: https://github.com/linebender/piet-metal
[`wgpu`]: https://wgpu.rs/
[Xilem]: https://github.com/linebender/xilem/
[Rust code of conduct]: https://www.rust-lang.org/policies/code-of-conduct
[`custom-hal-archive-with-shaders`]: https://github.com/linebender/piet-gpu/tree/custom-hal-archive-with-shaders
[`custom-hal-archive`]: https://github.com/linebender/piet-gpu/tree/custom-hal-archive
[piet-dx12]: https://github.com/bzm3r/piet-dx12
[GhostScript tiger]: https://commons.wikimedia.org/wiki/File:Ghostscript_Tiger.svg
[winit]: https://github.com/rust-windowing/winit
[Bevy]: https://bevyengine.org/
[Requiem for piet-gpu-hal]: https://raphlinus.github.io/rust/gpu/2023/01/07/requiem-piet-gpu-hal.html
[the changelog]: https://github.com/Enkom-Tech/wello/blob/main/CHANGELOG.md
