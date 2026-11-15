use glam::DVec3;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};
use std::time::Instant;

use crate::physics::{Body, step_euler};

const ORBIT_SPEED: f64 = 0.5; // speed for a circular orbit

pub struct App {
    window: Option<Window>,
    bodies: Vec<Body>,
    frame: u64,
    last_frame: Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes().with_title("Gravitas");

        self.window = Some(event_loop.create_window(attributes).unwrap());
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
                
                step_euler(&mut self.bodies, 0.001);
                self.frame += 1;

                if self.frame % 60 == 0 {
                    println!("{}", (self.bodies[1].pos() - self.bodies[0].pos()).length());
                }

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
        }
    }
}
