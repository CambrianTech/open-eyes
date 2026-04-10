//! open-eyes-core — 3D scene reconstruction from multi-camera feeds
//!
//! Rust adaptation of the CBAR (Cambrian AR) layer from react-home-ar.
//! The original TypeScript/C++ implementation proved real-time 3D scene
//! understanding on iPhone 7 at 30-60fps. This Rust port targets:
//!
//! - Multi-camera fusion (N stationary cameras → unified 3D scene)
//! - Gaussian splat scene representation (navigable from any viewpoint)
//! - Surface normal estimation for lighting-aware understanding
//! - Feature tracking across cameras (ORB/optical flow)
//! - Point cloud accumulation and temporal interpolation
//!
//! The core crate is compute-only — no I/O, no networking, no camera
//! drivers. It takes frames in and produces scene state out. Camera
//! integration lives in open-eyes-camera, grid integration in
//! open-eyes-grid, and detection/tracking in open-eyes-detect.

pub mod geometry;
pub mod scene;
pub mod features;
pub mod fusion;
pub mod frame;

/// A 3D point in world coordinates.
pub type Point3 = nalgebra::Point3<f64>;

/// A 3D vector.
pub type Vector3 = nalgebra::Vector3<f64>;

/// A 4x4 transformation matrix (camera pose, world transforms).
pub type Transform = nalgebra::Matrix4<f64>;

/// Camera intrinsic parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraIntrinsics {
    /// Focal length in pixels (x)
    pub fx: f64,
    /// Focal length in pixels (y)
    pub fy: f64,
    /// Principal point x
    pub cx: f64,
    /// Principal point y
    pub cy: f64,
    /// Image width
    pub width: u32,
    /// Image height
    pub height: u32,
    /// Radial distortion coefficients [k1, k2, k3]
    pub distortion: [f64; 3],
}

/// A timestamped camera frame with its extrinsic pose.
#[derive(Debug, Clone)]
pub struct CameraFrame {
    /// Which camera produced this frame
    pub camera_id: String,
    /// Camera intrinsics (may be shared across frames from same camera)
    pub intrinsics: CameraIntrinsics,
    /// Camera-to-world transform (extrinsic pose)
    pub pose: Transform,
    /// Frame timestamp (monotonic, seconds)
    pub timestamp: f64,
    /// Raw image data (RGB, row-major)
    pub image: image::RgbImage,
}

/// The accumulated 3D scene state — the output of multi-camera fusion.
#[derive(Debug, Clone, Default)]
pub struct SceneState {
    /// Accumulated 3D points with normals and colors
    pub points: Vec<SurfacePoint>,
    /// Per-camera last-known pose
    pub camera_poses: std::collections::HashMap<String, Transform>,
    /// Scene bounding box [min, max]
    pub bounds: Option<(Point3, Point3)>,
    /// Total frames processed
    pub frames_processed: u64,
    /// Last update timestamp
    pub last_update: f64,
}

/// A 3D surface point with normal and color.
#[derive(Debug, Clone, Copy)]
pub struct SurfacePoint {
    pub position: Point3,
    pub normal: Vector3,
    pub color: [u8; 3],
    /// Confidence / weight (accumulated over observations)
    pub confidence: f32,
}
