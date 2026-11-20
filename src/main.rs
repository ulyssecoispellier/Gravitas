use winit::event_loop::EventLoop;

use crate::app::App;

mod app;
mod mesh;
mod physics;
mod renderer;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
