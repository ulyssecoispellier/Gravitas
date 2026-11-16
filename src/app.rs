use glam::DVec3;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::physics::{Body, step_euler};
use crate::renderer::Renderer;

const ORBIT_SPEED: f64 = 0.5; // speed for a circular orbit
const DT: f64 = 0.001; // size of one physics step, in seconds

pub struct App {
    renderer: Option<Renderer>,
    window: Option<Window>,
    bodies: Vec<Body>,
    frame: u64,
    last_frame: Instant,
    accumulator: f64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes().with_title("Gravitas");

        let window = event_loop.create_window(attributes).unwrap();

        self.renderer = Some(Renderer::new(&window).unwrap());
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.logical_key == Key::Named(NamedKey::Escape)
                    && event.state == ElementState::Pressed
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let frame_time = (now - self.last_frame).as_secs_f64();
                self.last_frame = now;

                self.accumulator += frame_time;
                while self.accumulator >= DT {
                    step_euler(&mut self.bodies, DT);
                    self.accumulator -= DT;
                }

                self.frame += 1;

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            bodies: vec![
                Body::new(
                    DVec3::new(0.0, 0.0, 0.0),
                    DVec3::new(0.0, -ORBIT_SPEED, 0.0),
                    1.0,
                    0.1,
                ),
                Body::new(
                    DVec3::new(2.0, 0.0, 0.0),
                    DVec3::new(0.0, ORBIT_SPEED, 0.0),
                    1.0,
                    0.1,
                ),
            ],
            frame: 0,
            last_frame: Instant::now(),
            accumulator: 0.0,
            renderer: None,
        }
    }
}
