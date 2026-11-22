use glam::Vec3;

/// Distance along the ray to the nearest hit with the sphere, or None if it misses.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let ctr_to_origin = origin - center;
    let projection = dir.dot(ctr_to_origin);
    let gap = ctr_to_origin.dot(ctr_to_origin) - radius * radius;
    let discriminant = projection * projection - gap;

    if discriminant < 0.0 {
        return None;
    }

    let distance = -projection - discriminant.sqrt();

    if distance < 0.0 { None } else { Some(distance) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_sphere_in_front() {
        let hit = ray_sphere(Vec3::new(0.0, 0.0, -5.0), Vec3::Z, Vec3::ZERO, 1.0);
        assert_eq!(hit, Some(4.0));
    }

    #[test]
    fn misses_sphere_to_the_side() {
        let hit = ray_sphere(
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::Z,
            Vec3::new(5.0, 0.0, 0.0),
            1.0,
        );
        assert_eq!(hit, None);
    }

    #[test]
    fn sphere_behind_camera() {
        let hit = ray_sphere(Vec3::new(0.0, 0.0, -5.0), -Vec3::Z, Vec3::ZERO, 1.0);
        assert_eq!(hit, None);
    }
}
