//! Moves the bodies forward in time.

use crate::physics::{Body, acceleration_on};

/// Advances every body by one time step `dt` (semi-implicit Euler).
///
/// Velocities are updated first, then positions use the **new** velocities.
/// This small detail keeps orbits stable much longer than plain Euler.
pub fn step_euler(bodies: &mut [Body], dt: f64) {
    // 1. Update every velocity from the current positions
    for i in 0..bodies.len() {
        bodies[i].vel += acceleration_on(i, bodies) * dt;
    }
    // 2. Move every body with its new velocity
    for body in bodies.iter_mut() {
        body.pos += body.vel * dt;
    }
}
