//! The orbit camera: circles around a target point, controlled with the mouse.

use glam::{Mat4, Vec3};

/// A camera orbiting a target, described by angles and a distance (spherical coordinates)
/// rather than by a raw position. That makes mouse control easy:
/// drag = change the angles, scroll = change the distance.
pub struct Camera {
    /// The point the camera looks at and orbits around
    target: Vec3,
    /// Horizontal angle around the vertical (Z) axis, in radians
    yaw: f32,
    /// Vertical angle above the horizon, in radians
    pitch: f32,
    /// Distance from the target
    distance: f32,
    /// Vertical field of view of the lens, in radians
    fov: f32,
}

impl Camera {
    /// Creates a camera. Angles are in radians.
    pub fn new(target: Vec3, yaw: f32, pitch: f32, distance: f32, fov: f32) -> Self {
        Self {
            target,
            yaw,
            pitch,
            distance,
            fov,
        }
    }

    /// Camera position in the world: a point on a sphere around the target.
    pub fn eye(&self) -> Vec3 {
        // Spherical → Cartesian. Pitch is measured from the horizon: cos for x/y, sin for z
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.cos() * self.yaw.sin();
        let z = self.distance * self.pitch.sin();

        // The sphere is centred on the target, not on the origin
        Vec3 { x, y, z } + self.target
    }

    /// The view matrix: moves the world so it's seen from the camera.
    ///
    /// The camera's position is a point on a sphere around the target, computed from the angles.
    pub fn view(&self) -> Mat4 {
        // Look from `eye` at the target, with Z as "up"
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Z)
    }

    /// Turns the camera around the target. `dx`/`dy` = mouse movement in pixels.
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.005;
        self.pitch += dy * 0.005;

        // Stay just under ±90°: there, `look_at` with Z as "up" breaks and the view flips
        self.pitch = self.pitch.clamp(-1.5, 1.5);
    }

    /// Moves the camera closer (positive `amount`) or further away.
    pub fn zoom(&mut self, amount: f32) {
        // Multiply rather than subtract: each step = the same % closer, at any distance
        self.distance *= 1.0 - amount * 0.1;

        // Not inside the target, not lost in space
        self.distance = self.distance.clamp(0.5, 50.0);
    }

    /// Widens (`up = true`) or narrows the lens: changes the field of view.
    pub fn fov_ctrl(&mut self, up: bool) {
        // About 4° per key press (1 radian ≈ 57°)
        self.fov += if up {
            4f32.to_radians()
        } else {
            -4f32.to_radians()
        };

        // Keep the lens between 0° and about 121°
        self.fov = self.fov.clamp(0.17, 2.11);
    }

    /// Slides the target along the screen's right/up directions. `dx`/`dy` = mouse movement in pixels.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.cos() * self.yaw.sin();
        let z = self.distance * self.pitch.sin();

        // Camera axes: back (target → camera), right, up
        let back = Vec3 { x, y, z }.normalize();
        let right = Vec3::Z.cross(back).normalize();
        let up = back.cross(right);

        self.target += (-right * dx + up * dy) * 0.0015 * self.distance;
    }

    /// Puts the target back on the Sun.
    pub fn reset(&mut self) {
        self.target = Vec3::ZERO;
    }

    /// The projection matrix: the camera's lens (perspective, so far things look smaller).
    ///
    /// `aspect` = window width / height. It belongs to the window, so it's passed in, not stored.
    pub fn projection(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::vulkan::perspective(self.fov, aspect, 0.1, 100.0)
    }
}
