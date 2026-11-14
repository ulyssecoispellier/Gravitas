use glam::DVec3;

use crate::physics::{Body, integrator::step_euler};

mod physics;

fn main() {
    let mut bodies = vec![
        Body::new(DVec3::new(0.0, 0.0, 0.0), DVec3::ZERO, 1.0, 0.1),
        Body::new(DVec3::new(2.0, 0.0, 0.0), DVec3::ZERO, 1.0, 0.1),
    ];

    let dt = 0.001;
    for step in 1..=2500 {        
        step_euler(&mut bodies, dt);

        if step % 250 == 0 {
            let t = step as f64 * dt;
            println!("t = {:.4}   d = {:.4}", t, (bodies[1].pos() - bodies[0].pos()).length());
        }
    }
}
