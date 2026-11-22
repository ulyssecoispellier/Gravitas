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

// The star system (G = 1, so the units are arbitrary)
const STAR_MASS: f64 = 1.0;
const ROCKY_DISTANCE: f64 = 1.0; // from the star
const GIANT_DISTANCE: f64 = 2.0; // from the star
const GIANT_MASS: f64 = 0.02;
// From the giant. Must stay well inside its Hill sphere (≈ 0.38 here), or the star steals the moon
const MOON_DISTANCE: f64 = 0.25;
const MOON_TILT: f64 = 30.0 * std::f64::consts::PI / 180.0; // 10°, in radians

/// Everything the program keeps between two events.
///
/// Field order matters: Rust drops fields from top to bottom, and the renderer
/// must be destroyed **before** the window it draws into.
pub struct App {
    /// The Vulkan renderer. `None` until the window exists (see `resumed`)
    renderer: Option<Renderer>,
    /// The OS window. `None` until `resumed`
    window: Option<Window>,
    /// All the bodies of the simulation
    bodies: Vec<Body>,
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
                                    // Glow flag: body 0 is the star
                                    if i == 0 { 1.0 } else { 0.0 },
                                ],
                                light_pos: [
                                    star_pos.x as f32,
                                    star_pos.y as f32,
                                    star_pos.z as f32,
                                    0.0,
                                ],
                                info: [
                                    body.radius() as f32,
                                    if i == 0 { 1.0 } else { 0.0 },
                                    0.0,
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
            // A heavy star at the centre, 2 planets on circular orbits, and a moon around the gas giant.
            // The star must stay first: body 0 is the light source.
            bodies: vec![
                // Star
                Body::new(DVec3::ZERO, DVec3::ZERO, STAR_MASS, 0.25),
                // Rocky (telluric) planet
                Body::new(
                    DVec3::new(ROCKY_DISTANCE, 0.0, 0.0),
                    DVec3::new(0.0, orbit_speed(STAR_MASS, ROCKY_DISTANCE), 0.0),
                    0.00001,
                    0.06,
                ),
                // Gas giant
                Body::new(
                    DVec3::new(GIANT_DISTANCE, 0.0, 0.0),
                    DVec3::new(0.0, orbit_speed(STAR_MASS, GIANT_DISTANCE), 0.0),
                    GIANT_MASS,
                    0.1,
                ),
                // Gas giant's moon: next to the giant, moving with it + orbiting it on a tilted plane
                Body::new(
                    DVec3::new(GIANT_DISTANCE + MOON_DISTANCE, 0.0, 0.0),
                    {
                        let moon_speed = orbit_speed(GIANT_MASS, MOON_DISTANCE);

                        // Its own speed is split between y and z by the tilt, so the orbit plane leans
                        DVec3::new(
                            0.0,
                            orbit_speed(STAR_MASS, GIANT_DISTANCE) + moon_speed * MOON_TILT.cos(),
                            moon_speed * MOON_TILT.sin(),
                        )
                    },
                    0.000001,
                    0.02,
                ),
            ],
            frame: 0,
            last_frame: Instant::now(),
            accumulator: 0.0,
            renderer: None,
            // Looking at the star from 5 units away, slightly from above
            camera: Camera::new(
                Vec3::new(0.0, 0.0, 0.0),
                -std::f32::consts::FRAC_PI_2,
                (3.0f32 / 5.0).asin(),
                5.0,
                45f32.to_radians(),
            ),
            dragging: false,
            last_cursor: (0.0, 0.0),
        }
    }
}
