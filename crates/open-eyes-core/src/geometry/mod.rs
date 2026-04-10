//! geometry — 3D math primitives for camera calibration and scene reconstruction
//!
//! Adapted from react-home-ar's CBARCameraIntrinsics + CBARSceneGeometry.
//! Stationary cameras simplify this dramatically vs mobile AR:
//! - Intrinsics are fixed per camera (calibrated once)
//! - Extrinsics are nearly fixed (self-regulates via feature drift detection)
//! - Plane detection is the primary scene representation (not point clouds)
//! - Cross-camera registration is the ONE new piece vs single-camera AR

use crate::{CameraIntrinsics, Point3, Transform, Vector3};

/// A detected 3D plane in the scene (floor, wall, fence, etc.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Plane {
    /// Plane normal (unit vector)
    pub normal: [f64; 3],
    /// Distance from origin along normal (Hessian form: n·x + d = 0)
    pub distance: f64,
    /// Semantic label (floor, wall, ceiling, fence, ground, unknown)
    pub label: PlaneLabel,
    /// Confidence [0, 1] — increases with more observations
    pub confidence: f32,
    /// Bounding polygon vertices (3D points on the plane)
    pub boundary: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlaneLabel {
    Floor,
    Wall,
    Ceiling,
    Ground,
    Fence,
    Door,
    Window,
    Table,
    Unknown,
}

/// Project a 3D world point into a camera's 2D image coordinates.
pub fn project_point(
    point: &Point3,
    camera_pose: &Transform,
    intrinsics: &CameraIntrinsics,
) -> Option<(f64, f64)> {
    // Transform world point to camera frame
    let p_world = nalgebra::Vector4::new(point.x, point.y, point.z, 1.0);
    let p_cam = camera_pose.try_inverse()? * p_world;

    // Behind the camera
    if p_cam.z <= 0.0 {
        return None;
    }

    // Perspective projection
    let x = intrinsics.fx * (p_cam.x / p_cam.z) + intrinsics.cx;
    let y = intrinsics.fy * (p_cam.y / p_cam.z) + intrinsics.cy;

    // Bounds check
    if x < 0.0 || x >= intrinsics.width as f64 || y < 0.0 || y >= intrinsics.height as f64 {
        return None;
    }

    Some((x, y))
}

/// Unproject a 2D image point to a 3D ray in world coordinates.
pub fn unproject_ray(
    pixel: (f64, f64),
    camera_pose: &Transform,
    intrinsics: &CameraIntrinsics,
) -> (Point3, Vector3) {
    // Pixel to normalized camera coordinates
    let x = (pixel.0 - intrinsics.cx) / intrinsics.fx;
    let y = (pixel.1 - intrinsics.cy) / intrinsics.fy;

    // Ray in camera frame
    let ray_cam = nalgebra::Vector4::new(x, y, 1.0, 0.0);

    // Transform to world frame
    let ray_world = camera_pose * ray_cam;
    let direction = Vector3::new(ray_world.x, ray_world.y, ray_world.z).normalize();

    // Camera position in world frame (4th column of pose matrix)
    let origin = Point3::new(
        camera_pose[(0, 3)],
        camera_pose[(1, 3)],
        camera_pose[(2, 3)],
    );

    (origin, direction)
}

/// Triangulate a 3D point from two camera observations.
///
/// Given a matched feature visible in camera A at pixel_a and camera B
/// at pixel_b, compute the 3D world position via ray intersection.
/// Returns None if rays are nearly parallel (degenerate geometry).
pub fn triangulate(
    pixel_a: (f64, f64),
    pose_a: &Transform,
    intrinsics_a: &CameraIntrinsics,
    pixel_b: (f64, f64),
    pose_b: &Transform,
    intrinsics_b: &CameraIntrinsics,
) -> Option<Point3> {
    let (origin_a, dir_a) = unproject_ray(pixel_a, pose_a, intrinsics_a);
    let (origin_b, dir_b) = unproject_ray(pixel_b, pose_b, intrinsics_b);

    // Closest point between two rays (midpoint method)
    let w0 = origin_a - origin_b;
    let a = dir_a.dot(&dir_a);
    let b = dir_a.dot(&dir_b);
    let c = dir_b.dot(&dir_b);
    let d = dir_a.dot(&w0);
    let e = dir_b.dot(&w0);

    let denom = a * c - b * b;
    if denom.abs() < 1e-10 {
        return None; // Rays are parallel
    }

    let s = (b * e - c * d) / denom;
    let t = (a * e - b * d) / denom;

    let p_a = origin_a + dir_a * s;
    let p_b = origin_b + dir_b * t;

    // Midpoint of closest approach
    let midpoint = Point3::new(
        (p_a.x + p_b.x) / 2.0,
        (p_a.y + p_b.y) / 2.0,
        (p_a.z + p_b.z) / 2.0,
    );

    // Reject if rays are too far apart at closest approach (bad match)
    let separation = nalgebra::distance(&p_a, &p_b);
    if separation > 0.5 {
        return None;
    }

    Some(midpoint)
}

/// Estimate a plane from 3D points via RANSAC.
pub fn fit_plane_ransac(
    points: &[Point3],
    inlier_threshold: f64,
    min_inliers_fraction: f64,
    max_iterations: usize,
) -> Option<Plane> {
    let n = points.len();
    if n < 3 {
        return None;
    }

    let mut best_inliers = 0;
    let mut best_normal = [0.0f64; 3];
    let mut best_distance = 0.0;

    // Simple deterministic sampling for reproducibility
    for i in 0..max_iterations.min(n) {
        let j = (i + n / 3) % n;
        let k = (i + 2 * n / 3) % n;
        if i == j || j == k || i == k {
            continue;
        }

        let v1 = points[j] - points[i];
        let v2 = points[k] - points[i];
        let normal = v1.cross(&v2);
        let len = normal.norm();
        if len < 1e-10 {
            continue;
        }
        let normal = normal / len;
        let distance = -normal.dot(&points[i].coords);

        let inliers = points
            .iter()
            .filter(|p| (normal.dot(&p.coords) + distance).abs() < inlier_threshold)
            .count();

        if inliers > best_inliers {
            best_inliers = inliers;
            best_normal = [normal.x, normal.y, normal.z];
            best_distance = distance;
        }
    }

    if (best_inliers as f64) < min_inliers_fraction * n as f64 {
        return None;
    }

    Some(Plane {
        normal: best_normal,
        distance: best_distance,
        label: PlaneLabel::Unknown,
        confidence: best_inliers as f32 / n as f32,
        boundary: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_intrinsics() -> CameraIntrinsics {
        CameraIntrinsics {
            fx: 500.0, fy: 500.0, cx: 320.0, cy: 240.0,
            width: 640, height: 480, distortion: [0.0; 3],
        }
    }

    #[test]
    fn project_unproject_roundtrip() {
        let intrinsics = test_intrinsics();
        let pose = Transform::identity();
        let point = Point3::new(0.5, 0.3, 2.0);

        let pixel = project_point(&point, &pose, &intrinsics).unwrap();
        let (origin, dir) = unproject_ray(pixel, &pose, &intrinsics);

        let t = point.z / dir.z;
        let reconstructed = origin + dir * t;
        assert!((reconstructed.x - point.x).abs() < 0.01);
        assert!((reconstructed.y - point.y).abs() < 0.01);
    }

    #[test]
    fn triangulate_simple() {
        let intrinsics = test_intrinsics();
        let pose_a = Transform::identity();
        let mut pose_b = Transform::identity();
        pose_b[(0, 3)] = 1.0; // 1m to the right

        let true_point = Point3::new(0.5, 0.0, 3.0);
        let pixel_a = project_point(&true_point, &pose_a, &intrinsics).unwrap();
        let pixel_b = project_point(&true_point, &pose_b, &intrinsics).unwrap();

        let result = triangulate(
            pixel_a, &pose_a, &intrinsics,
            pixel_b, &pose_b, &intrinsics,
        ).unwrap();

        assert!((result.x - true_point.x).abs() < 0.05);
        assert!((result.y - true_point.y).abs() < 0.05);
        assert!((result.z - true_point.z).abs() < 0.05);
    }

    #[test]
    fn behind_camera_returns_none() {
        let intrinsics = test_intrinsics();
        let pose = Transform::identity();
        let behind = Point3::new(0.0, 0.0, -1.0);
        assert!(project_point(&behind, &pose, &intrinsics).is_none());
    }
}
