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

### Textures (not in git, ~65 MB once resized)
One folder per planet, its moons in `satellites/`, and the same file names everywhere:

```
textures/
├── sky.exr                       # NASA Deep Star Maps 2020 (Milky Way + stars, HDR)
├── earth/
│   ├── albedo.jpg                # surface colour
│   ├── normal.tif                # surface slopes
│   ├── specular.tif              # shininess (oceans)
│   └── satellites/
│       └── moon/
│           └── albedo.jpg
├── jupiter/
│   ├── albedo.jpg
│   └── satellites/
│       ├── io/  europa/  ganymede/  callisto/
└── …
```

```bash
SSS=https://www.solarsystemscope.com/textures/download
curl -L -o textures/sky.exr https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004851/starmap_2020_4k.exr
mkdir -p textures/{mercury,venus,earth/satellites/moon,mars,jupiter,saturn,uranus,neptune}
curl -L -o textures/mercury/albedo.jpg            $SSS/8k_mercury.jpg
curl -L -o textures/venus/albedo.jpg              $SSS/4k_venus_atmosphere.jpg
curl -L -o textures/earth/albedo.jpg              $SSS/8k_earth_daymap.jpg
curl -L -o textures/earth/normal.tif              $SSS/8k_earth_normal_map.tif
curl -L -o textures/earth/specular.tif            $SSS/8k_earth_specular_map.tif
curl -L -o textures/earth/satellites/moon/albedo.jpg $SSS/8k_moon.jpg
curl -L -o textures/mars/albedo.jpg               $SSS/8k_mars.jpg
curl -L -o textures/jupiter/albedo.jpg            $SSS/8k_jupiter.jpg
curl -L -o textures/saturn/albedo.jpg             $SSS/8k_saturn.jpg
curl -L -o textures/uranus/albedo.jpg             $SSS/2k_uranus.jpg
curl -L -o textures/neptune/albedo.jpg            $SSS/2k_neptune.jpg
```
Moons of Jupiter, Saturn and Neptune, from [Celestia Content](https://github.com/CelestiaProject/CelestiaContent) (served by GitHub):
```bash
CEL=https://raw.githubusercontent.com/CelestiaProject/CelestiaContent/master/textures/hires
mkdir -p textures/jupiter/satellites/{io,europa,ganymede,callisto} textures/saturn/satellites/titan textures/neptune/satellites/triton
curl -L -o textures/jupiter/satellites/io/albedo.png        $CEL/io.png
curl -L -o textures/jupiter/satellites/europa/albedo.jpg    $CEL/europa.jpg
curl -L -o textures/jupiter/satellites/ganymede/albedo.jpg  $CEL/ganymede.jpg
curl -L -o textures/jupiter/satellites/callisto/albedo.jpg  $CEL/callisto.jpg
curl -L -o textures/saturn/satellites/titan/albedo.png      $CEL/titan.png
curl -L -o textures/saturn/satellites/titan/clouds.jpg      $CEL/titan-clouds.jpg
curl -L -o textures/neptune/satellites/triton/albedo.jpg    $CEL/triton.jpg
```
Then bring **every** map to the same size, 4096 × 2048 (the texture array needs equal layers), and PNGs to JPG:
```bash
python3 - <<'PY'
from PIL import Image; import glob, os
Image.MAX_IMAGE_PIXELS = None
for p in glob.glob('textures/**/*.[jpt][pni][gf]', recursive=True):
    im = Image.open(p); im.load()
    if im.size != (4096, 2048): im = im.resize((4096, 2048), Image.LANCZOS)
    if p.endswith('.png'):
        im.convert('RGB').save(p[:-4] + '.jpg', quality=92); os.remove(p)
    elif p.endswith('.jpg'): im.convert('RGB').save(p, quality=92)
    else: im.save(p, compression='tiff_deflate')
PY
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

**Order:** 5 (atmospheres) ✅ → moons (so bounced light has something to show) → 3 + 4 (bounces + accumulation).

| Step | | Status |
|---|---|---|
| 1 | HDR scene image + tone mapping | ✅ |
| 2 | All bodies in the ray pass as analytic spheres (lit, coloured, Earth textured, soft eclipses) | ✅ |
| 3 | Bounces (moonlight, earthshine, twilight lighting the ground): one diffuse bounce, rays aimed at each body's cone (importance sampling), 8 stratified samples per body: live and nearly clean | ✅ |
| 4 | Noise: light temporal blend (works live) · full accumulation as a paused "photo mode" | ⬜ |
| 5 | Atmospheres: rays scatter in the air and pick up its colour (blue sky, orange sunsets, hazy gas giants) — single scattering: Rayleigh (molecules) + Mie (dust, droplets), per-planet β, density, planet shadow, eclipses from other bodies, extinction, phase functions, jitter | ✅ |
| — | Atmospheres at real scale: real thicknesses (Earth ≈ 100 km); soft twilight edge for a planet's own shadow in its air (Sun's disc + multiple scattering) | ⬜ |
| — | Sub-pixel bodies drawn as points with conserved energy (the Sun seen from the outer planets, distant planets as star-like dots) | ⬜ |
| 6 | Bloom on top of the HDR image — v1 ✅ (13×13 blur in the post pass), v2: downsampled blur chain for wide halos | 🚧 |
| 7 | Auto-exposure: the image adapts to dark scenes like an eye or a camera (night sides become visible) | ⬜ |

### 🌍 The Solar System
| | Status |
|---|---|
| Earth: real 8k day map in the ray pass (UV from the hit normal) | ✅ |
| Earth: normal map + specular (ocean glints) | ⬜ |
| Earth: city lights on the night side (night map, shown where sunlight ≈ 0) | ⬜ |
| Earth: cloud layer (8k clouds map) on top of the day map, so its average albedo is a realistic ~0.3 | ⬜ |
| Real average albedos for every body's base colour (Moon ≈ 0.12, Venus ≈ 0.75, Europa ≈ 0.65…), needed for physical bounces | ⬜ |
| Texture array: one layer per body (planets + moons), `colours.w` = layer index; base colour kept as albedo for bounces | ⬜ |
| Real maps for every body (Solar System Scope; NASA/USGS for Galilean moons, Titan, Triton) + normal maps for rocky bodies | ⬜ |
| Axial tilt + rotation per planet (Earth 23.4°, Venus upside down, Uranus on its side; real day lengths), textures turning with them | ⬜ |
| Major moons on rails: Moon, Io, Europa, Ganymede, Callisto, Titan, Triton (retrograde) | ✅ |
| More moons (Phobos & Deimos, …): needs more than 16 body slots | ⬜ |
| Tilted moon orbits: an inclination per moon (Moon ≈ 5° to the ecliptic, so eclipses are rare; Triton ≈ 157°), plus each planet's axial tilt for its equatorial moons | ⬜ |
| Moons spin too: each has its own rotation, and its texture turns with it | ⬜ |
| Tidal locking: the Moon, the Galilean moons, Titan and Triton spin exactly once per orbit, so they always show the same face to their planet | ⬜ |
| Elliptical moon orbits (eccentricity) + the direction the tilt points (ascending node), from real orbital elements | ⬜ |
| Real orbital elements for the planets too (eccentricity, inclination), so they start where they really are on a given date | ⬜ |
| Saturn's rings: flat disk mesh + transparent striped texture | ⬜ |
| Asteroid belt + Kuiper belt: thousands of rocks in one draw call (GPU instancing) | ⬜ |
| Skybox: starfield / Milky Way background (NASA Deep Star Maps, `textures/sky.exr`) | ✅ |
| Rotate the sky from galactic to ecliptic coordinates (real Milky Way orientation) | ⬜ |
| Soft shadows: the Sun as a disc (umbra cone, penumbra, annular eclipses), on surfaces and in atmospheres | ✅ |

### 🔭 Real scale & navigation
| | Status |
|---|---|
| Real sizes and distances for every body, in AU / years / Sun masses (G = 4π²) | ✅ |
| Moons with real gravity instead of rails (now possible: they sit inside their planet's Hill sphere) | ⬜ |
| Precision at real scale: camera-relative positions (f64 on the CPU, small f32 offsets on the GPU) + reversed infinite depth | ✅ |
| Focus a body: the camera follows it (Tab / Shift+Tab), pan offset kept around it | ✅ |
| Collapsible *Celestial Bodies* panel (egui) listing every planet and its satellites; click one to fly there | 🚧 |
| Camera flight between bodies: constant-speed zoom out, cruise, constant-speed zoom in, following both moving bodies | ✅ |
| Time speed that adapts to the focused body, so fast moons stay watchable | ⬜ |

### ⚡ Performance (real scale: planets often fill the screen)
Measured: ~70 ms per frame (≈14 fps) with Earth filling the view, on a Retina screen (~3.6 million pixels).

| | Gain | Status |
|---|---|---|
| Measure first: toggle bloom / atmospheres / sample counts to see where the GPU time goes | — | ⬜ |
| Separable bloom: blur horizontally, then vertically (66 reads per pixel instead of 169: radius 16, spacing 1.5), with one extra image and pass | ~2.5× on bloom, no grain | ✅ |
| Atmosphere transmittance LUT: precompute the air towards the Sun (height × Sun angle) in a small 2D texture, instead of an 8-step march per sample (Bruneton / Hillaire) | ~5–8× on atmospheres | ⬜ |
| Eclipse culling: the CPU lists, for each body, the few bodies that can shadow it (usually 0 or 1), instead of 16 bodies with asin/acos per pixel and per atmosphere sample | big on every lit pixel | ⬜ |
| Eclipse quick-reject: skip a body with one dot product when it's too far from the Sun in the sky, before any asin/acos | cheap first step before culling | ✅ |
| Bounce phase skip: skip a source body's 8 bounce samples when we only see its night side (lit fraction from the Sun–body–point angle), and when it's entirely below our horizon | full bounce cost, about half the orbit | ⬜ |
| Render scale: draw the 3D scene at half the Retina resolution, upscale in the post pass (H key; drawn into the top-left corner of the full-size images, so no recreation) | ~4× on the scene at 0.5, ~1.8× at 0.75 (H uses 0.75) | ✅ |
| Temporal upscaling (DLSS / FSR 2 style): render at low resolution with a different sub-pixel jitter each frame, then rebuild a sharp full-resolution image from the previous frames (reprojected with the camera motion, rejected where they don't match). Same pass also cleans the atmosphere / bounce noise and allows dithered bloom | near-native sharpness at 25% of the pixels | ⬜ |
| Bloom v2: blur a downsized copy (wide halos, even cheaper) | | ⬜ |

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
- Star map: [NASA SVS Deep Star Maps 2020](https://svs.gsfc.nasa.gov/4851) (public domain).
- Planet and Moon textures: [Solar System Scope](https://www.solarsystemscope.com/textures/) (CC BY 4.0, based on NASA imagery).
- Io, Europa, Ganymede, Callisto, Titan, Triton textures: [Celestia Content](https://github.com/CelestiaProject/CelestiaContent) (`textures/hires/`), built from NASA/JPL/USGS data by John van Vliet, Askaniy Anpilogov, Björn Jónsson, Kevin M. Gill and others (CC BY 3.0 / CC BY 4.0; see each file's `.license` there).
