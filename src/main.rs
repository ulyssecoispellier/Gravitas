//! Gravitas: a small 3D gravity simulator, written to learn Rust and Vulkan.
//!
//! `main` only creates the event loop and hands control to [`App`].

use winit::event_loop::EventLoop;

use crate::app::App;

// The modules (one file each) that make up the program
mod app;
mod camera;
mod mesh;
mod physics;
mod renderer;

/// Entry point: create the event loop and run the app until it exits.
fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    // Blocks here: winit calls our `App` methods for every event, until we exit
    event_loop.run_app(&mut app).unwrap();
}
