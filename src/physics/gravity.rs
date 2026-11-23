//! Newton's law of universal gravitation.

use glam::DVec3;

use crate::physics::Body;

/// Gravitational constant in AU, years and Sun masses: 4π² (Kepler: an orbit at 1 AU takes 1 year)
pub const G: f64 = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
/// Softening length: stops the force from exploding when two bodies get very close.
/// 1e-6 AU ≈ 150 km: far below any real gap between bodies, so it never weakens real gravity.
pub const EPSILON: f64 = 1e-6;

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

/// Speed needed for a circular orbit at `distance` from a body of mass `central_mass`.
///
/// Gravity must equal the force that bends the path into a circle:
/// `G·M·m / r² = m·v² / r`, so `v = √(G·M / r)`. The orbiting body's own mass cancels out.
pub fn orbit_speed(central_mass: f64, distance: f64) -> f64 {
    ((G * central_mass) / distance).sqrt()
}
