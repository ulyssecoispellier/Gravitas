use glam::DVec3;

pub struct Body {
    pub(super) pos: DVec3,
    pub(super) vel: DVec3,
    pub(super) mass: f64,
    pub(super) _radius: f64,
}

impl Body {
    pub fn new(pos: DVec3, vel: DVec3, mass: f64, _radius: f64) -> Self {
        Body {
            pos,
            vel,
            mass,
            _radius,
        }
    }

    pub fn _pos(&self) -> DVec3 {
        self.pos
    }

    pub fn _radius(&self) -> f64 {
        self._radius
    }
}
