//! open-eyes-splat — Gaussian splatting for 3D scene rendering
//!
//! Takes the accumulated SceneState (point cloud + normals + colors)
//! and converts it into a Gaussian splat representation that can be
//! rendered from any viewpoint in real time. The navigable 3D view
//! is the key differentiator vs flat multi-camera grid displays.
//!
//! Uses wgpu for GPU-accelerated splat rendering. The splat scene
//! is updated incrementally as new camera frames arrive — no full
//! reconstruction needed per frame.

pub struct SplatScene {
    pub num_gaussians: usize,
}
