//! A body: anything with a mass that gravity acts on (a star, a planet...).

use glam::DVec3;

/// A point mass moving in space.
///
/// The fields are `pub(super)`: usable inside the `physics` module only.
/// Code outside it reads them through the getters below.
pub struct Body {
    /// Position, in simulation units (`f64` for precision)
    pub(super) pos: DVec3,
    /// Velocity, in units per second
    pub(super) vel: DVec3,
    /// Mass (units are arbitrary since `G = 1`)
    pub(super) mass: f64,
    /// Radius: sets the size of the body's sphere on screen (not used by the physics)
    pub(super) radius: f64,
}

impl Body {
    /// Creates a body from its position, velocity, mass and radius.
    pub fn new(pos: DVec3, vel: DVec3, mass: f64, radius: f64) -> Self {
        Body {
            pos,
            vel,
            mass,
            radius,
        }
    }

    /// Current position.
    pub fn pos(&self) -> DVec3 {
        self.pos
    }

    /// Radius of the body.
    pub fn radius(&self) -> f64 {
        self.radius
    }
}
