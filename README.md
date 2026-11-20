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
| Aspect ratio + 3D camera (matrices) | 🚧 Next |
| Rocky planet (mesh, textures, depth buffer) | ⬜ Planned |
| Orbit camera (mouse) | ⬜ Planned |
| Star + gas giant, ray-traced in shaders | ⬜ Planned |
| Atmospheric scattering | ⬜ Planned |
| Bloom + HDR post-processing | ⬜ Planned |

Right now, running the program opens a window, runs the physics in real time, and shows the two bodies orbiting each other as small squares (one draw call per body, positioned with push constants). It renders every frame with Vulkan: the window is cleared to a solid colour and a colour-blended square, whose 4 vertices and 6 indices come from Rust through vertex and index buffers, is drawn by the GPU and moved every frame with push constants, using a vertex shader and a fragment shader. Under the hood, it sets up Vulkan: it connects Vulkan to the window (surface), lists the available GPUs, picks the best one that can draw and present to the window, opens a logical device on it, creates a swapchain (the images that will be shown in the window), and prepares a command buffer and the synchronization objects needed to draw a frame. Each frame, it waits for the previous one, takes a swapchain image, records the commands that clear it (dynamic rendering, Vulkan 1.3), submits them and presents the image.

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
├── mesh.rs              # Vertex type, its GPU layout, and mesh data
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

---

## Built with
- [`winit`](https://github.com/rust-windowing/winit): windowing and input
- [`vulkanalia`](https://github.com/KyleMayes/vulkanalia): Vulkan bindings for Rust
- [`glam`](https://github.com/bitshifter/glam-rs): vector maths
- [`anyhow`](https://github.com/dtolnay/anyhow): error handling
