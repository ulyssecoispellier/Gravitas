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
| Lighting: per-pixel diffuse light from the star (normals, push constants struct) | ✅ Done |
| Star system: star, rocky planet, gas giant + moon (`orbit_speed`, per-body size) | ✅ Done |
| The Solar System: Sun + 8 planets (real masses, compressed distances, per-body colours) | ✅ Done |
| Rocky planet texture (staging upload, sampler, descriptors) | ✅ Done |
| Star + gas giant, ray-traced in shaders | ⬜ Planned |
| Ray-traced Sun: depth (`gl_FragDepth`), limb darkening, glow | ✅ Done |
| Shadow rays: eclipses (ray from each planet pixel towards the Sun, bodies in a uniform buffer) | ✅ Done |
| Soft shadows: the Sun as a disc, not a point (umbra cone, penumbra, annular eclipses) | ⬜ Planned |
| Post-processing pass (scene drawn offscreen, then a full-screen pass to the screen) | ✅ Done |
| **Main direction: a path-traced renderer** (physically based light, multiple bounces) | |
| ↳ 1. HDR: 16-bit float scene image + tone mapping (needed for bright Sun vs dim bounced light) | ✅ Done |
| ↳ 2. All bodies in the ray pass as analytic spheres (textures from the hit point's UV) | 🚧 In progress (lit + coloured ✅, texture + shadows next) |
| ↳ 3. Bounces: random rays per pixel, N bounces, albedo per body (moonlight, earthshine, soft shadows) | ⬜ Planned |
| ↳ 4. Progressive accumulation (average frames while the camera is still) to remove noise | ⬜ Planned |
| ↳ 5. Atmospheres in the path tracer: rays scatter inside the air and pick up its colour (blue sky, orange sunsets, hazy gas giants) | ⬜ Planned |
| ↳ 6. Bloom on top of the HDR image | ⬜ Planned |
| Skybox: a starfield / Milky Way background (cube map or equirectangular texture, sampled with the ray direction) | ⬜ Planned |
| Retro dithering mode, toggled with `C` (ordered / Bayer dithering, few colour levels) | ✅ Done |
| Planetary ring: flat disk mesh with a transparent striped texture | ⬜ Planned |
| Asteroid belt (between Mars and Jupiter) + Kuiper belt (beyond Neptune): thousands of rocks in one draw call (GPU instancing, `VertexInputRate::INSTANCE`) | ⬜ Planned |
| Test-particle physics: small bodies feel gravity but don't pull (N×planets instead of N×N) | ⬜ Planned |
| GPU physics with a compute shader (for very large particle counts) | ⬜ Idea |
| Toggle wireframe at runtime (2 pipelines, only if `fill_mode_non_solid` is supported) | ⬜ Idea |
| Collisions: a body that hits a bigger one merges into it (mass added, removed with `Vec::retain`) | ⬜ Idea |
| Moons: the major ones (Moon, Phobos & Deimos, the 4 Galilean moons, Titan…), on fixed "on rails" orbits around their planet, drawn at an exaggerated distance to match the exaggerated sizes | ⬜ Planned |
| Moon texture: 2nd texture + 2nd descriptor set, bind the right set before each body | ⬜ Planned |
| Real textures + normal maps for the rocky planets (Mercury, Venus clouds, Earth, Mars): texture array (one image, one layer per planet), normals computed in Rust from NASA/USGS elevation maps, colour maps from Solar System Scope (CC BY 4.0) | ⬜ Planned |
| Axial tilt + rotation per planet (added to `BodyData`): the texture spins around a tilted axis. Real values: tilt Mercury 0°, Venus 177° (upside down), Earth 23.4°, Mars 25.2°, Jupiter 3.1°, Saturn 26.7°, Uranus 97.8° (on its side), Neptune 28.3°; Venus and Uranus spin backwards. Rotation needs its own time scale: at the sim's speed, a real Earth day would last ~0.05 s | ⬜ Planned |

Right now, running the program opens a window, runs the physics in real time, and shows the Solar System (the Sun and its 8 planets, Earth textured) as solid spheres, each with its own size and colour. A single unit UV sphere is generated in Rust and uploaded to the GPU through vertex and index buffers. It is drawn once per body, scaled to the body's radius, and each body's model × view × projection matrix (a perspective camera looking at the orbit from an angle) is sent with push constants. A depth buffer makes the closest surface win, so the spheres hide each other correctly. The Sun (body 0) glows, and lights the planets per pixel from its position (diffuse lighting with vertex normals). The camera orbits the system with the mouse, and the window can be resized freely: the swapchain and depth image are rebuilt at the new size. Under the hood, it sets up Vulkan: it connects Vulkan to the window (surface), picks the best GPU that can draw and present, opens a logical device, creates a swapchain, and prepares command buffers and synchronization objects. Each frame, it waits for the previous one, takes a swapchain image, records the drawing commands (dynamic rendering, Vulkan 1.3), submits them and presents the image.

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
├── renderer.rs          # Vulkan: the Renderer (setup order, per-frame drawing, cleanup)
├── renderer/
│   ├── instance.rs      # Vulkan instance + validation messages
│   ├── device.rs        # GPU choice + logical device
│   ├── swapchain.rs     # the images shown in the window
│   ├── pipeline.rs      # shaders + graphics pipeline
│   └── resources.rs     # GPU memory: buffers and images
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
shaders/                 # compiled SPIR-V (.spv), generated by build.rs, not in git
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
| `←` / `→` | Slow down / speed up time (×0 to ×10) |
| Right click + drag | Move around (pan) |
| `R` | Re-centre the camera on the Sun |
| `C` | Retro dithering on / off |
| `M` | Show / hide the old mesh planets (compare with the ray-traced ones) |

---

## Built with
- [`winit`](https://github.com/rust-windowing/winit): windowing and input
- [`vulkanalia`](https://github.com/KyleMayes/vulkanalia): Vulkan bindings for Rust
- [`glam`](https://github.com/bitshifter/glam-rs): vector maths
- [`anyhow`](https://github.com/dtolnay/anyhow): error handling
