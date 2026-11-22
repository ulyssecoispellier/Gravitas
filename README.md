# Gravitas 🪐

A small 3D gravity simulator written in **Rust**, rendered with **Vulkan**.

Gravitas simulates a star system (a star, a gas giant and a rocky planet) using Newtonian gravity, and will render it in real time with a free-flying camera, in the spirit of *Universe Sandbox*.

It is also a learning project: the goal is to understand Rust and Vulkan from the ground up, writing every part by hand: physics, windowing, the Vulkan renderer and the shaders.

---

## Status

| Part | State |
|---|---|
| N-body gravity (with softening) | ✅ Done |
| Fixed-timestep integrator (semi-implicit Euler) | ✅ Done |
| Window + game loop (`winit`) | ✅ Done |
| Vulkan instance | ✅ Done |
| Validation layers (debug builds) | ✅ Done |
| GPU selection | ✅ Done |
| Logical device + graphics queue | ✅ Done |
| Window surface + presentation support | ✅ Done |
| Swapchain + image views | ✅ Done |
| Command buffers + synchronization | ✅ Done |
| Clear the window to a colour (first frame) | ✅ Done |
| First triangle (shaders + graphics pipeline) | ✅ Done |
| Automatic shader compilation (`build.rs`) | ✅ Done |
| Vertex buffers (geometry sent from Rust) | ✅ Done |
| Index buffers + generic buffer helper | ✅ Done |
| Push constants: send per-frame data to shaders | ✅ Done |
| Show the orbiting bodies on screen | ✅ Done |
| Transform matrices + orthographic camera (correct aspect ratio) | ✅ Done |
| 3D perspective camera (`look_at`) | ✅ Done |
| Spheres (3D vertices, generated UV-sphere mesh) | ✅ Done |
| Depth buffer (spheres correctly hide each other) | ✅ Done |
| Orbit camera (drag to rotate, scroll to zoom, arrows for FOV) | ✅ Done |
| Window resizing (swapchain + depth image rebuilt) | ✅ Done |
| Rocky planet (mesh, textures) | ⬜ Planned |
| Star + gas giant, ray-traced in shaders | ⬜ Planned |
| Atmospheric scattering | ⬜ Planned |
| Bloom + HDR post-processing | ⬜ Planned |

Right now, running the program opens a window, runs the physics in real time, and shows the two bodies orbiting each other as solid coloured spheres. Each sphere is a UV sphere generated in Rust and uploaded to the GPU through vertex and index buffers. It is drawn once per body, and each body's model × view × projection matrix (a perspective camera looking at the orbit from an angle) is sent with push constants. A depth buffer makes the closest surface win, so the spheres hide each other correctly. The camera orbits the system with the mouse, and the window can be resized freely: the swapchain and depth image are rebuilt at the new size. Under the hood, it sets up Vulkan: it connects Vulkan to the window (surface), picks the best GPU that can draw and present, opens a logical device, creates a swapchain, and prepares command buffers and synchronization objects. Each frame, it waits for the previous one, takes a swapchain image, records the drawing commands (dynamic rendering, Vulkan 1.3), submits them and presents the image.

---

## How it works

### Physics
- Each body has a position, a velocity, a mass and a radius.
- Gravity: `a = G · m · r / (|r|² + ε²)^(3/2)`. The softening term `ε` prevents infinite accelerations when two bodies get very close.
- Time advances in fixed steps of `0.001 s` using an accumulator, so the simulation runs at the same speed whatever the screen's refresh rate.

### Rendering (planned)
A hybrid renderer:
- **Rasterized:** the rocky planet, as a textured mesh with a depth buffer.
- **Ray-traced in shaders:** the star and the gas giant, using analytic ray–sphere intersection and ray-marched atmospheric scattering. This needs no hardware ray tracing, so it runs on any Vulkan GPU, including Apple Silicon through MoltenVK.
- **Post-processing:** bloom and tone mapping.

---

## Project structure

```
src/
├── main.rs              # entry point: creates the event loop and runs the app
├── app.rs               # window, input, game loop, fixed timestep
├── renderer.rs          # Vulkan: setup, swapchain, pipeline, buffers, per-frame drawing
├── mesh.rs              # Vertex type, its GPU layout, and mesh generation (UV sphere)
├── camera.rs            # orbit camera: angles + distance → view and projection matrices
├── physics.rs           # physics module (re-exports)
└── physics/
    ├── body.rs          # Body: position, velocity, mass, radius
    ├── gravity.rs       # gravitational acceleration (with softening)
    └── integrator.rs    # time stepping
```

```
shaders_uncompiled/      # GLSL shader sources (triangle.vert, triangle.frag)
shaders/                 # compiled SPIR-V (.spv), embedded into the binary
```

---

## Building and running

### Requirements
- [Rust](https://rustup.rs) (edition 2024)
- A Vulkan **1.3** driver

### macOS (Apple Silicon)
Install Vulkan through Homebrew:

```bash
brew install vulkan-loader molten-vk vulkan-tools vulkan-validationlayers shaderc
```

Check that your GPU is detected:

```bash
vulkaninfo --summary
```

The project's `.cargo/config.toml` tells the program where Homebrew installed Vulkan, so no extra setup is needed.

### Shaders
Vulkan only reads SPIR-V. `build.rs` compiles every shader in `shaders_uncompiled/` with `glslc` (from the `shaderc` package) automatically, whenever a shader changes. Nothing to do by hand.

### Run

```bash
cargo run              # debug build, Vulkan validation layers ON
cargo run --release    # optimized build, validation layers OFF
```

In debug builds, Vulkan validation messages are printed to the terminal (🟡 warnings, 🔴 errors).

### Controls
| Key | Action |
|---|---|
| `Esc` | Quit |
| Left click + drag | Rotate the camera around the system |
| Scroll | Zoom in / out |
| `↑` / `↓` | Widen / narrow the field of view |

---

## Built with
- [`winit`](https://github.com/rust-windowing/winit): windowing and input
- [`vulkanalia`](https://github.com/KyleMayes/vulkanalia): Vulkan bindings for Rust
- [`glam`](https://github.com/bitshifter/glam-rs): vector maths
- [`anyhow`](https://github.com/dtolnay/anyhow): error handling
