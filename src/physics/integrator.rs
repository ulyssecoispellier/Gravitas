use crate::physics::{Body, acceleration_on};

pub fn step_euler(bodies: &mut [Body], dt: f64) {
    for i in 0..bodies.len() {
        bodies[i].vel += acceleration_on(i, bodies) * dt;
    }
    for body in bodies.iter_mut() {
        body.pos += body.vel * dt;
    }
}
