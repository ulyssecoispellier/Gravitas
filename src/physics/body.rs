use glam::DVec3;

#[derive(Debug, Clone, Copy)]
pub struct Body {
    pub(super) pos: DVec3,
    pub(super) vel: DVec3,
    pub(super) mass: f64,
    pub(super) radius: f64,
}

impl Body {
    pub fn new(pos: DVec3, vel: DVec3, mass: f64, radius: f64) -> Self {
        Body {
            pos,
            vel,
            mass,
            radius,
        }
    }

    pub fn pos(&self) -> DVec3 {
        self.pos
    }

    pub fn radius(&self) -> f64 {
        self.radius
    }
}
