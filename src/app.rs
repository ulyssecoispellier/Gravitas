//! The application: owns the window, the simulation and the renderer, and reacts to events.

use glam::{DVec3, Mat4, Vec3};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::camera::Camera;
use crate::physics::{Body, orbit_speed, step_euler};
use crate::renderer::{PushConstants, Renderer};

const DT: f64 = 0.001; // size of one physics step, in seconds

// The Solar System (G = 1, Sun mass = 1).
// Masses: real, relative to the Sun. Distances: compressed (2·√AU) so the outer planets fit on screen.
// Radii: exaggerated for visibility (real ones would be invisible dots)
const SUN_MASS: f64 = 1.0;
const SUN_RADIUS: f64 = 0.5;
const SUN_COLOUR: [f32; 3] = [1.0, 0.85, 0.4];

/// One planet's starting data.
struct PlanetData {
    /// Distance from the Sun
    distance: f64,
    /// Mass, relative to the Sun
    mass: f64,
    /// Radius on screen
    radius: f64,
    /// Base colour (r, g, b)
    colour: [f32; 3],
}

const PLANETS: [PlanetData; 8] = [
    // Mercury
    PlanetData {
        distance: 1.25,
        mass: 1.66e-7,
        radius: 0.035,
        colour: [0.6, 0.58, 0.55],
    },
    // Venus
    PlanetData {
        distance: 1.70,
        mass: 2.45e-6,
        radius: 0.075,
        colour: [0.9, 0.8, 0.55],
    },
    // Earth (textured)
    PlanetData {
        distance: 2.00,
        mass: 3.00e-6,
        radius: 0.08,
        colour: [0.3, 0.5, 0.9],
    },
    // Mars
    PlanetData {
        distance: 2.47,
        mass: 3.23e-7,
        radius: 0.05,
        colour: [0.8, 0.4, 0.25],
    },
    // Jupiter
    PlanetData {
        distance: 4.56,
        mass: 9.55e-4,
        radius: 0.25,
        colour: [0.85, 0.7, 0.5],
    },
    // Saturn
    PlanetData {
        distance: 6.19,
        mass: 2.86e-4,
        radius: 0.21,
        colour: [0.9, 0.8, 0.55],
    },
    // Uranus
    PlanetData {
        distance: 8.76,
        mass: 4.37e-5,
        radius: 0.12,
        colour: [0.6, 0.85, 0.9],
    },
    // Neptune
    PlanetData {
        distance: 10.97,
        mass: 5.15e-5,
        radius: 0.12,
        colour: [0.3, 0.45, 0.95],
    },
];

/// Index of the textured body in `bodies` (Sun = 0, Mercury = 1, Venus = 2, Earth = 3)
const EARTH: usize = 3;

/// Everything the program keeps between two events.
///
/// Field order matters: Rust drops fields from top to bottom, and the renderer
/// must be destroyed **before** the window it draws into.
pub struct App {
    /// The Vulkan renderer. `None` until the window exists (see `resumed`)
    renderer: Option<Renderer>,
    /// The OS window. `None` until `resumed`
    window: Option<Window>,
    /// All the bodies of the simulation (Sun first)
    bodies: Vec<Body>,
    /// Base colour of each body, same order as `bodies`
    colours: Vec<[f32; 3]>,
    /// Number of frames drawn so far
    frame: u64,
    /// When the previous frame started, to measure how much time passed
    last_frame: Instant,
    /// Real time not simulated yet (see the fixed timestep loop)
    accumulator: f64,
    /// The orbit camera, controlled with the mouse
    camera: Camera,
    /// Is the left mouse button held down?
    dragging: bool,
    /// Mouse position at the previous `CursorMoved` event
    last_cursor: (f64, f64),
}

// winit calls these methods when something happens (the app starts, a key is pressed...)
impl ApplicationHandler for App {
    /// Called when the app is ready to show a window: create it, then the renderer.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes().with_title("Gravitas");

        let window = event_loop.create_window(attributes).unwrap();

        // The renderer needs the window, to draw into it
        self.renderer = Some(Renderer::new(&window).unwrap());
        self.window = Some(window);

        // Ask for the first frame. Each frame then asks for the next one (macOS won't do it for us)
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Called for every window event (input, resize, redraw...).
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            // The window's red button
            WindowEvent::CloseRequested => event_loop.exit(),
            // Keys: Escape quits, up/down arrows change the field of view
            WindowEvent::KeyboardInput { event, .. } => {
                // Match the key and its state together, as a tuple
                match (event.logical_key, event.state) {
                    (Key::Named(NamedKey::Escape), ElementState::Pressed) => event_loop.exit(),
                    (Key::Named(NamedKey::ArrowUp), ElementState::Pressed) => {
                        self.camera.fov_ctrl(true)
                    }
                    (Key::Named(NamedKey::ArrowDown), ElementState::Pressed) => {
                        self.camera.fov_ctrl(false)
                    }
                    // Any other key (or a key release): do nothing
                    (_, _) => (),
                }
            }
            // Time to draw a frame: advance the physics, then render
            WindowEvent::RedrawRequested => {
                // How much real time passed since the last frame?
                let now = Instant::now();
                let frame_time = (now - self.last_frame).as_secs_f64();
                self.last_frame = now;

                // Fixed timestep: run as many DT-sized physics steps as fit in that time.
                // The leftover stays in the accumulator for the next frame.
                self.accumulator += frame_time;
                while self.accumulator >= DT {
                    step_euler(&mut self.bodies, DT);
                    self.accumulator -= DT;
                }

                self.frame += 1;

                if let Some(renderer) = &mut self.renderer {
                    let aspect = renderer.aspect();

                    // The camera's lens, then its position
                    let projection = self.camera.projection(aspect);

                    let view = self.camera.view();

                    // Body 0 plays the star: everything is lit from its position
                    let star_pos = self.bodies[0].pos();

                    // One box of push constants per body: its matrix, its position, the light's position
                    let pushes: Vec<PushConstants> = self
                        .bodies
                        .iter()
                        .enumerate()
                        .map(|(i, body)| {
                            let pos = body.pos();
                            // Right to left: scale the unit sphere to the body's radius, then move it into place
                            let model = Mat4::from_translation(Vec3 {
                                x: pos.x as f32,
                                y: pos.y as f32,
                                z: pos.z as f32,
                            }) * Mat4::from_scale(Vec3::splat(body.radius() as f32));

                            PushConstants {
                                transform: (projection * view * model).to_cols_array(),
                                body_pos: [
                                    pos.x as f32,
                                    pos.y as f32,
                                    pos.z as f32,
                                    0.0, // unused (padding)
                                ],
                                light_pos: [
                                    star_pos.x as f32,
                                    star_pos.y as f32,
                                    star_pos.z as f32,
                                    0.0,
                                ],
                                info: [
                                    body.radius() as f32,
                                    // Glow flag (star)
                                    if i == 0 { 1.0 } else { 0.0 },
                                    // Texture flag (Earth)
                                    if i == EARTH { 1.0 } else { 0.0 },
                                    0.0,
                                ],
                                colour: [
                                    self.colours[i][0],
                                    self.colours[i][1],
                                    self.colours[i][2],
                                    0.0,
                                ],
                            }
                        })
                        .collect();

                    renderer.draw_frame(&pushes).unwrap();
                }

                // Ask for the next frame: this keeps the loop going
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            // Left button: start / stop dragging
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.dragging = state == ElementState::Pressed;
                }
            }
            // Mouse moved: while dragging, rotate by the movement since the last event
            WindowEvent::CursorMoved { position, .. } => {
                if self.dragging {
                    self.camera.rotate(
                        (position.x - self.last_cursor.0) as f32,
                        (position.y - self.last_cursor.1) as f32,
                    );
                }
                // Always remember the position, otherwise the next drag would jump
                self.last_cursor = (position.x, position.y);
            }
            // Scroll: zoom in / out
            WindowEvent::MouseWheel { delta, .. } => {
                // Mouse wheels count in "lines", trackpads in pixels: bring both to the same scale
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.05,
                };

                self.camera.zoom(amount);
            }
            // Window resized: rebuild the images that depend on its size (not when minimized: size 0)
            WindowEvent::Resized(size) => match (size.width, size.height) {
                (0, _) | (_, 0) => (),
                (_, _) => {
                    if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                        renderer.recreate_swapchain(window).unwrap();
                    }
                }
            },
            // Ignore every other event
            _ => {}
        }
    }
}

impl App {
    /// Creates the app with its starting bodies and camera.
    /// The window and the renderer come later, in `resumed`.
    pub fn new() -> Self {
        Self {
            window: None,
            // The Sun first (body 0 = the light source), then each planet on a circular orbit
            bodies: std::iter::once(Body::new(DVec3::ZERO, DVec3::ZERO, SUN_MASS, SUN_RADIUS))
                .chain(PLANETS.iter().map(|p| {
                    Body::new(
                        DVec3::new(p.distance, 0.0, 0.0),
                        DVec3::new(0.0, orbit_speed(SUN_MASS, p.distance), 0.0),
                        p.mass,
                        p.radius,
                    )
                }))
                .collect(),
            colours: std::iter::once(SUN_COLOUR)
                .chain(PLANETS.iter().map(|p| p.colour))
                .collect(),
            frame: 0,
            last_frame: Instant::now(),
            accumulator: 0.0,
            renderer: None,
            // Looking at the Sun from 14 units away, slightly from above
            camera: Camera::new(
                Vec3::new(0.0, 0.0, 0.0),
                -std::f32::consts::FRAC_PI_2,
                (3.0f32 / 5.0).asin(),
                14.0,
                45f32.to_radians(),
            ),
            dragging: false,
            last_cursor: (0.0, 0.0),
        }
    }
}
