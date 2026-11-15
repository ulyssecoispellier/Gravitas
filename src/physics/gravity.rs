use glam::DVec3;

use crate::physics::Body;

pub const G: f64 = 1.0; // Gravitational constant
pub const EPSILON: f64 = 0.05; // Softening constant

pub fn acceleration_on(i: usize, bodies: &[Body]) -> DVec3 {
    let mut acc = DVec3::ZERO;

    for (j, other) in bodies.iter().enumerate() {
        if j == i {
            continue;
        }

        let r = other.pos - bodies[i].pos;
        let dist_squ = r.length_squared();
        let soft = dist_squ + EPSILON * EPSILON;
        acc += G * other.mass * r / soft.powf(1.5);
    }

    acc
}
