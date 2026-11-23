<div align="center">

# 🪐 Gravitas

**A real-time Solar System simulator, written from scratch in Rust and Vulkan.**

N-body gravity · ray-traced Sun and planets · eclipses · HDR · a free-flying camera

![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)
![Vulkan](https://img.shields.io/badge/Vulkan-1.3-red?logo=vulkan)
![Platform](https://img.shields.io/badge/platform-macOS%20(tested)%20·%20Windows%20·%20Linux%20(planned)-lightgrey)
![Status](https://img.shields.io/badge/status-learning%20project-blue)

</div>

---

## ✨ What it is

Gravitas simulates the **Sun and its 8 planets** with real Newtonian gravity, and renders them in real time with a camera you can fly around, in the spirit of *Universe Sandbox*.

It is also a **learning project**: the goal is to understand Rust and Vulkan from the ground up. Every part is written by hand: the physics, the window, the Vulkan renderer and the shaders. No game engine, no rendering library.

## 🌌 Features

| | |
|---|---|
| 🌍 **Physics** | N-body gravity between every body (real mass ratios), fixed time step, adjustable time speed |
| ☀️ **Ray-traced Sun** | a perfect sphere computed per pixel: real depth, limb darkening, glow |
| 🪐 **Ray-traced planets** | every body is an analytic sphere in a full-screen ray-tracing pass: coloured and lit by the Sun |
| 🌑 **Eclipses** | shadow rays: each pixel checks whether another body hides the Sun |
| 🔆 **HDR** | 16-bit float scene image + ACES tone mapping, so the Sun can be much brighter than white |
| 🎥 **Camera** | orbit, pan, zoom, field of view, re-centre, resizable window (fullscreen ready, commented out in `app.rs`) |
| 🎮 **Retro mode** | ordered (Bayer) dithering post-effect, toggled with `C` |
| 🧪 **Tests** | ray–sphere intersection unit-tested on the CPU (`cargo test`) |

## 🎮 Controls

| Input | Action |
|---|---|
| Left click + drag | Rotate the camera around the target |
| Right click + drag | Move around (pan) |
| Scroll | Zoom in / out |
| `↑` / `↓` | Widen / narrow the field of view |
| `←` / `→` | Slow down / speed up time (×0 to ×10) |
| `R` | Re-centre the camera on the Sun |
| `C` | Retro dithering on / off |
| `M` | Show / hide the old mesh planets (compare with the ray-traced ones) |
| `Esc` | Quit |

---

## 🚀 Getting started

### Requirements
- [Rust](https://rustup.rs) (edition 2024)
- A **Vulkan 1.3** driver
- `glslc` (from `shaderc`) to compile the shaders

### macOS (Apple Silicon)
```bash
brew install vulkan-loader molten-vk vulkan-tools vulkan-validationlayers shaderc
vulkaninfo --summary   # check that your GPU is detected
```
The project's `.cargo/config.toml` tells the program where Homebrew installed Vulkan, so there's nothing else to set up.

### Sky texture (not in git, 36 MB)
```bash
curl -L -o textures/sky.exr https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004851/starmap_2020_4k.exr
```

### Earth textures (not in git, from [Solar System Scope](https://www.solarsystemscope.com/textures/), CC BY 4.0)
```bash
mkdir -p textures/earth && cd textures/earth
curl -L -O https://www.solarsystemscope.com/textures/download/8k_earth_daymap.jpg
curl -L -O https://www.solarsystemscope.com/textures/download/8k_earth_normal_map.tif
curl -L -O https://www.solarsystemscope.com/textures/download/8k_earth_specular_map.tif
cd ../..
```

### Run
```bash
cargo run              # debug build, Vulkan validation layers ON (🟡 warnings, 🔴 errors in the terminal)
cargo run --release    # optimised build, validation layers OFF
cargo test             # unit tests
```
Shaders are compiled automatically: `build.rs` runs `glslc` on every file in `shaders_uncompiled/` whenever one changes.

---

## 🔭 How it works

### Physics
- Each body has a position, a velocity, a mass and a radius.
- Gravity, with softening `ε` to avoid infinite forces at very short range:
  `a = G · m · r / (|r|² + ε²)^(3/2)`
- Semi-implicit Euler integration in fixed steps of `0.001 s`, driven by an accumulator, so the simulation runs the same on any screen. Time speed changes how much time the accumulator receives, never the step size.
- Scale: real masses (relative to the Sun), distances compressed as `2·√AU`, and radii exaggerated so everything is visible.

### Rendering, one frame
```
① scene pass ──▶ scene image (HDR, 16-bit float)
   ├─ ray pass: a full-screen triangle; each pixel shoots a ray from the camera,
   │            finds the closest body and shades it (Sun, planets, Earth texture);
   │            rays that miss read the NASA star map (skybox)
   └─ mesh pass (optional, M): the old rasterized planets, with texture + eclipses
② post pass ──▶ swapchain image ──▶ screen
   └─ bloom (bright pixels leak onto neighbours), ACES tone mapping (HDR → 0..1),
      optional retro dithering
```
- **Shader data**: small per-draw data travels as **push constants**; data shared by every pixel (all bodies' positions, radii and colours) lives in a **uniform buffer** updated every frame.
- **No hardware ray tracing needed**: ray–sphere intersection is plain maths in a fragment shader, so it runs on any Vulkan GPU, including Apple Silicon through MoltenVK.

---

## 🗺️ Roadmap

### 🎯 Main direction: a path-traced renderer
Physically based light with multiple bounces.

**Order:** 5 (atmospheres) first → moons (so bounced light has something to show) → 3 + 4 (bounces + accumulation).

| Step | | Status |
|---|---|---|
| 1 | HDR scene image + tone mapping | ✅ |
| 2 | All bodies in the ray pass as analytic spheres (lit, coloured, Earth textured, soft eclipses) | ✅ |
| 3 | Bounces: random rays, albedo per body (moonlight, earthshine, twilight lighting the ground, soft shadows) | ⬜ |
| 4 | Progressive accumulation to remove noise while the camera is still | ⬜ |
| 5 | Atmospheres: rays scatter in the air and pick up its colour (blue sky, orange sunsets, hazy gas giants) — Rayleigh single scattering ✅ (per-planet β, density, planet shadow, extinction, phase, jitter) · Mie (dust, haze) next | 🚧 |
| 6 | Bloom on top of the HDR image — v1 ✅ (13×13 blur in the post pass), v2: downsampled blur chain for wide halos | 🚧 |
| 7 | Auto-exposure: the image adapts to dark scenes like an eye or a camera (night sides become visible) | ⬜ |

### 🌍 The Solar System
| | Status |
|---|---|
| Earth: real 8k day map in the ray pass (UV from the hit normal) | ✅ |
| Earth: normal map + specular (ocean glints) | ⬜ |
| Earth: city lights on the night side (night map, shown where sunlight ≈ 0) | ⬜ |
| Real colour textures + normal maps for the other rocky planets (texture array) | ⬜ |
| Axial tilt + rotation per planet (Venus upside down, Uranus on its side, both spinning backwards) | ⬜ |
| Major moons on "on rails" orbits (Moon, Phobos & Deimos, Galilean moons, Titan…) + their textures | ⬜ |
| Saturn's rings: flat disk mesh + transparent striped texture | ⬜ |
| Asteroid belt + Kuiper belt: thousands of rocks in one draw call (GPU instancing) | ⬜ |
| Skybox: starfield / Milky Way background (NASA Deep Star Maps, `textures/sky.exr`) | ✅ |
| Rotate the sky from galactic to ecliptic coordinates (real Milky Way orientation) | ⬜ |
| Soft shadows: the Sun as a disc (umbra cone, penumbra, annular eclipses) | ⬜ |

### 💻 Cross-platform (Windows · Linux · macOS)
| | Status |
|---|---|
| Handle `ERROR_OUT_OF_DATE_KHR` / `SUBOPTIMAL_KHR` in `draw_frame` (acquire + present): rebuild the swapchain and skip the frame instead of crashing on resize/minimize | ⬜ |
| Don't draw while the window is minimized (size 0) | ⬜ |
| Pick the swapchain format and present mode from what the driver offers, with fallbacks | ⬜ |
| Load textures relative to the executable / project, not the current folder | ⬜ |
| Setup instructions for Windows (LunarG Vulkan SDK) and Linux (distro packages) | ⬜ |
| Test on a Windows or Linux machine (NVIDIA / AMD / Intel) | ⬜ |

### 🧪 Physics & ideas
| | Status |
|---|---|
| Test particles: small bodies feel gravity but don't pull (N × planets instead of N × N) | ⬜ |
| Collisions: a body that hits a bigger one merges into it | 💡 |
| GPU physics with a compute shader | 💡 |
| Wireframe toggle at runtime (2 pipelines) | 💡 |
| Click on a planet to select it (mouse ray + `ray_sphere`) | 💡 |
| Airglow + aurorae: the upper air glowing faintly on its own (green/red layers) | 💡 |

<details>
<summary><b>📚 Learning log: everything built so far</b></summary>

| Milestone | |
|---|---|
| N-body gravity (with softening), fixed-timestep semi-implicit Euler | ✅ |
| Window + game loop (`winit`) | ✅ |
| Vulkan instance, validation layers, GPU selection, logical device, surface | ✅ |
| Swapchain + image views, command buffers + synchronisation | ✅ |
| First clear colour, first triangle (shaders + graphics pipeline) | ✅ |
| Automatic shader compilation (`build.rs`) | ✅ |
| Vertex + index buffers, generic buffer helper | ✅ |
| Push constants, transform matrices, orthographic then perspective camera | ✅ |
| Generated UV-sphere mesh, depth buffer | ✅ |
| Orbit camera (rotate, pan, zoom, FOV, reset), window resizing | ✅ |
| Per-pixel diffuse lighting from the star | ✅ |
| Star system, then the full Solar System (real masses, `orbit_speed`) | ✅ |
| Textured planet: staging upload, barriers, sampler, descriptor sets | ✅ |
| Renderer split into modules | ✅ |
| Full-screen ray pass, ray–sphere intersection (Rust + tests, then GLSL) | ✅ |
| Ray-traced Sun: real depth (`gl_FragDepth`), limb darkening, glow, dithering | ✅ |
| Uniform buffer of all bodies, shadow rays (eclipses) | ✅ |
| Post-processing pass + retro dithering | ✅ |
| HDR (16-bit float scene) + ACES tone mapping | ✅ |
| Time speed control, ray-traced planets (closest hit, lighting, colours) | ✅ |

</details>

---

## 📁 Project structure

```
src/
├── main.rs              # entry point: event loop + app
├── app.rs               # window, input, game loop, Solar System data
├── camera.rs            # orbit camera: angles + distance → view / projection matrices
├── mesh.rs              # Vertex type and UV-sphere generation
├── ray.rs               # ray–sphere intersection (CPU, unit-tested)
├── physics.rs           # physics module (re-exports)
├── physics/
│   ├── body.rs          # Body: position, velocity, mass, radius
│   ├── gravity.rs       # gravitational acceleration, orbit speed
│   └── integrator.rs    # time stepping
├── renderer.rs          # the Renderer: setup, per-frame drawing, cleanup
└── renderer/
    ├── instance.rs      # Vulkan instance + validation messages
    ├── device.rs        # GPU choice + logical device
    ├── swapchain.rs     # the images shown in the window
    ├── pipeline.rs      # shaders + graphics pipelines
    └── resources.rs     # GPU memory: buffers, images, descriptors

shaders_uncompiled/      # GLSL sources
├── raytrace.vert/.frag  # full-screen ray pass (Sun, planets, glow)
├── triangle.vert/.frag  # mesh planets (texture, eclipses)
└── post.frag            # tone mapping + retro dithering
shaders/                 # compiled SPIR-V, generated by build.rs (not in git)
textures/                # planet textures
```

---

## 🛠️ Built with
- [`winit`](https://github.com/rust-windowing/winit): windowing and input
- [`vulkanalia`](https://github.com/KyleMayes/vulkanalia): Vulkan bindings for Rust
- [`glam`](https://github.com/bitshifter/glam-rs): vector maths
- [`bytemuck`](https://github.com/Lokathor/bytemuck): structs → raw bytes for the GPU
- [`image`](https://github.com/image-rs/image): texture loading
- [`anyhow`](https://github.com/dtolnay/anyhow): error handling

## 🙏 Credits
- Planet data: NASA.
- Planned planet textures: [Solar System Scope](https://www.solarsystemscope.com/textures/) (CC BY 4.0, based on NASA imagery) and NASA / USGS elevation maps.
