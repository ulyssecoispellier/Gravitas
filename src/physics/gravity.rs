//! Newton's law of universal gravitation.

use glam::DVec3;

use crate::physics::Body;

/// Gravitational constant. 1.0 instead of 6.674e-11, because our units are arbitrary.
pub const G: f64 = 1.0;
/// Softening length: stops the force from exploding when two bodies get very close.
pub const EPSILON: f64 = 0.05;

/// Total gravitational acceleration on body `i`, caused by every other body.
///
/// Softened Newton formula: `a = G·m·r / (|r|² + ε²)^1.5`
pub fn acceleration_on(i: usize, bodies: &[Body]) -> DVec3 {
    let mut acc = DVec3::ZERO;

    for (j, other) in bodies.iter().enumerate() {
        // A body doesn't pull on itself
        if j == i {
            continue;
        }

        // Vector from body i to the other body
        let r = other.pos - bodies[i].pos;
        let dist_squ = r.length_squared();
        // Adding ε² avoids dividing by ~0 when the bodies overlap
        let soft = dist_squ + EPSILON * EPSILON;
        // G·m/d² in the direction of r (r/d³ = unit direction/d²)
        acc += G * other.mass * r / soft.powf(1.5);
    }

    acc
}
