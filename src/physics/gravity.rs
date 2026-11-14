use glam::DVec3;

use crate::physics::Body;

pub const G: f64 = 1.0;

pub fn acceleration_on(i: usize, bodies: &[Body]) -> DVec3 {
    let mut acc = DVec3::ZERO;

    for (j, other) in bodies.iter().enumerate() {
        if j == i {
            continue;
        }

        let r = other.pos - bodies[i].pos;
        let dist = r.length();
        acc += G * other.mass * r / dist.powi(3);
    }

    acc
}
