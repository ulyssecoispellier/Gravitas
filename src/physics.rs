//! Physics: bodies, Newtonian gravity, and the integrator that moves them over time.

pub mod body;
pub mod gravity;
pub mod integrator;

// Re-exports: the rest of the code can write `physics::Body` instead of `physics::body::Body`
pub use body::Body;
pub use gravity::{acceleration_on, orbit_speed};
pub use integrator::step_euler;
