//! fusion — N-camera fusion into a unified 3D scene
//!
//! This is the ONE new piece vs the single-camera react-home-ar pipeline.
//! Everything else (feature tracking, surface normals, pose estimation)
//! exists and is proven. Fusion takes N independent camera pipelines and
//! registers them in a shared world coordinate system.
//!
//! The architecture mirrors react-home-ar's thread-event pipeline:
//! each camera runs its own processing thread. The fusion module
//! subscribes to events from all cameras and merges their outputs.
//!
//! Key insight for stationary cameras: the cross-camera registration
//! is a ONE-TIME calibration (with self-regulating updates when cameras
//! shift). The per-frame work is just "update the dynamic entities in
//! the already-calibrated scene."

use crate::{CameraFrame, CameraIntrinsics, Point3, SceneState, SurfacePoint, Transform, Vector3};
use crate::geometry::{Plane, triangulate};
use std::collections::HashMap;

/// A registered camera in the multi-camera system.
#[derive(Debug, Clone)]
pub struct RegisteredCamera {
    pub id: String,
    pub intrinsics: CameraIntrinsics,
    /// Camera-to-world transform (extrinsic pose).
    /// Nearly static for mounted cameras — updated only when drift is detected.
    pub pose: Transform,
    /// Feature descriptors from the last processed frame (for cross-camera matching)
    pub last_features: Vec<FeaturePoint>,
    /// Whether this camera's pose needs recalibration
    pub needs_recalibration: bool,
    /// Timestamp of last processed frame
    pub last_frame_time: f64,
}

/// A detected feature in a camera frame.
#[derive(Debug, Clone)]
pub struct FeaturePoint {
    /// 2D position in the image
    pub pixel: (f64, f64),
    /// Feature descriptor (e.g., ORB: 32 bytes)
    pub descriptor: Vec<u8>,
    /// Tracking ID (persistent across frames within one camera)
    pub track_id: Option<u64>,
}

/// The multi-camera fusion engine.
///
/// Holds all registered cameras and the accumulated scene state.
/// Processes frames from any camera and updates the unified scene.
pub struct FusionEngine {
    cameras: HashMap<String, RegisteredCamera>,
    scene: SceneState,
    /// Detected planes in the scene (cached, updated rarely)
    planes: Vec<Plane>,
    /// Cross-camera feature matches used for registration
    registration_matches: Vec<CrossCameraMatch>,
    /// Whether initial calibration has been performed
    calibrated: bool,
}

/// A feature match between two cameras (used for cross-camera registration).
#[derive(Debug, Clone)]
pub struct CrossCameraMatch {
    pub camera_a: String,
    pub camera_b: String,
    pub pixel_a: (f64, f64),
    pub pixel_b: (f64, f64),
    /// Triangulated 3D position (if cameras are calibrated)
    pub point_3d: Option<Point3>,
}

impl FusionEngine {
    pub fn new() -> Self {
        Self {
            cameras: HashMap::new(),
            scene: SceneState::default(),
            planes: Vec::new(),
            registration_matches: Vec::new(),
            calibrated: false,
        }
    }

    /// Register a camera with known intrinsics.
    ///
    /// The initial pose can be identity (unknown) — the calibration step
    /// will solve it from cross-camera feature matches. Or it can be
    /// provided if the camera's position is known (e.g., from a previous
    /// calibration session loaded from disk).
    pub fn register_camera(
        &mut self,
        id: &str,
        intrinsics: CameraIntrinsics,
        initial_pose: Option<Transform>,
    ) {
        self.cameras.insert(id.to_string(), RegisteredCamera {
            id: id.to_string(),
            intrinsics,
            pose: initial_pose.unwrap_or_else(Transform::identity),
            last_features: Vec::new(),
            needs_recalibration: initial_pose.is_none(),
            last_frame_time: 0.0,
        });
        // New camera invalidates calibration unless it came with a known pose
        if initial_pose.is_none() {
            self.calibrated = false;
        }
    }

    /// Process a frame from one camera.
    ///
    /// This is the main entry point called by each camera's processing
    /// thread. The fusion engine:
    /// 1. Updates the camera's feature set
    /// 2. Checks for global feature drift (camera moved?)
    /// 3. Cross-matches features with overlapping cameras
    /// 4. Updates the 3D scene with new observations
    ///
    /// Returns a list of events for downstream consumers (new entities,
    /// updated tracks, drift alerts, etc.)
    pub fn process_frame(
        &mut self,
        camera_id: &str,
        features: Vec<FeaturePoint>,
        timestamp: f64,
    ) -> Vec<FusionEvent> {
        let mut events = Vec::new();

        let camera = match self.cameras.get_mut(camera_id) {
            Some(c) => c,
            None => {
                events.push(FusionEvent::Warning(
                    format!("unknown camera: {camera_id}")
                ));
                return events;
            }
        };

        // Drift detection: compare current features to previous frame's
        // features. If the global motion is above threshold, the camera
        // moved and needs recalibration.
        if !camera.last_features.is_empty() {
            let drift = Self::estimate_global_drift(&camera.last_features, &features);
            if drift > 5.0 {
                // > 5 pixels of global feature drift → camera moved
                camera.needs_recalibration = true;
                events.push(FusionEvent::CameraDrift {
                    camera_id: camera_id.to_string(),
                    drift_pixels: drift,
                });
            }
        }

        camera.last_features = features;
        camera.last_frame_time = timestamp;
        self.scene.frames_processed += 1;
        self.scene.last_update = timestamp;

        events
    }

    /// Estimate global feature drift between two frames (same camera).
    ///
    /// Matches features by descriptor similarity, computes the median
    /// pixel displacement. High displacement = camera moved.
    fn estimate_global_drift(prev: &[FeaturePoint], curr: &[FeaturePoint]) -> f64 {
        // Simple brute-force matching by descriptor Hamming distance
        // TODO: use a proper matcher (FLANN, BFMatcher equivalent)
        let mut displacements = Vec::new();

        for p in prev.iter().take(50) {
            // Descriptor matching is placeholder
            if let Some(best) = curr.iter().min_by_key(|c| {
                hamming_distance(&p.descriptor, &c.descriptor)
            }) {
                let dx = best.pixel.0 - p.pixel.0;
                let dy = best.pixel.1 - p.pixel.1;
                displacements.push((dx * dx + dy * dy).sqrt());
            }
        }

        if displacements.is_empty() {
            return 0.0;
        }

        // Median displacement
        displacements.sort_by(|a, b| a.partial_cmp(b).unwrap());
        displacements[displacements.len() / 2]
    }

    /// Calibrate cross-camera registration from overlapping features.
    ///
    /// Called once at startup (or when cameras move). Finds feature
    /// matches between cameras with overlapping fields of view and
    /// solves the relative transforms.
    pub fn calibrate(&mut self) -> bool {
        // Need at least 2 cameras
        if self.cameras.len() < 2 {
            return false;
        }

        // Find cross-camera matches
        let camera_ids: Vec<String> = self.cameras.keys().cloned().collect();
        self.registration_matches.clear();

        for i in 0..camera_ids.len() {
            for j in (i + 1)..camera_ids.len() {
                let matches = self.find_cross_camera_matches(
                    &camera_ids[i],
                    &camera_ids[j],
                );
                self.registration_matches.extend(matches);
            }
        }

        // Need enough matches for a robust solve
        if self.registration_matches.len() < 8 {
            return false;
        }

        // TODO: solve relative poses from matches using essential matrix
        // decomposition or PnP. For now, mark as calibrated if we have
        // enough matches — the actual solver is the next implementation step.
        self.calibrated = true;
        true
    }

    /// Find feature matches between two cameras' current feature sets.
    fn find_cross_camera_matches(
        &self,
        cam_a: &str,
        cam_b: &str,
    ) -> Vec<CrossCameraMatch> {
        let a = match self.cameras.get(cam_a) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let b = match self.cameras.get(cam_b) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut matches = Vec::new();

        for feat_a in &a.last_features {
            let mut best_dist = u32::MAX;
            let mut best_b = None;

            for feat_b in &b.last_features {
                let dist = hamming_distance(&feat_a.descriptor, &feat_b.descriptor);
                if dist < best_dist {
                    best_dist = dist;
                    best_b = Some(feat_b);
                }
            }

            // Lowe's ratio test threshold (adapted for cross-camera)
            if best_dist < 64 {
                if let Some(fb) = best_b {
                    let point_3d = if self.calibrated {
                        triangulate(
                            feat_a.pixel, &a.pose, &a.intrinsics,
                            fb.pixel, &b.pose, &b.intrinsics,
                        )
                    } else {
                        None
                    };

                    matches.push(CrossCameraMatch {
                        camera_a: cam_a.to_string(),
                        camera_b: cam_b.to_string(),
                        pixel_a: feat_a.pixel,
                        pixel_b: fb.pixel,
                        point_3d,
                    });
                }
            }
        }

        matches
    }

    /// Get the current scene state.
    pub fn scene(&self) -> &SceneState {
        &self.scene
    }

    /// Get all registered cameras.
    pub fn cameras(&self) -> &HashMap<String, RegisteredCamera> {
        &self.cameras
    }

    /// Is the system calibrated (cross-camera registration solved)?
    pub fn is_calibrated(&self) -> bool {
        self.calibrated
    }
}

/// Events emitted by the fusion engine.
#[derive(Debug, Clone)]
pub enum FusionEvent {
    /// Camera moved — needs recalibration
    CameraDrift { camera_id: String, drift_pixels: f64 },
    /// New 3D point added to scene
    PointAdded(Point3),
    /// Entity detected across cameras
    EntityTracked { entity_id: String, position: Point3, cameras: Vec<String> },
    /// Calibration completed
    Calibrated { num_cameras: usize, num_matches: usize },
    /// Warning (non-fatal)
    Warning(String),
}

/// Hamming distance between two binary descriptors.
fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
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
    fn register_cameras() {
        let mut engine = FusionEngine::new();
        engine.register_camera("cam1", test_intrinsics(), None);
        engine.register_camera("cam2", test_intrinsics(), None);
        assert_eq!(engine.cameras().len(), 2);
        assert!(!engine.is_calibrated());
    }

    #[test]
    fn process_frame_unknown_camera() {
        let mut engine = FusionEngine::new();
        let events = engine.process_frame("unknown", Vec::new(), 0.0);
        assert!(matches!(events[0], FusionEvent::Warning(_)));
    }

    #[test]
    fn hamming_distance_identical() {
        let a = vec![0xFF, 0x00, 0xAA];
        assert_eq!(hamming_distance(&a, &a), 0);
    }

    #[test]
    fn hamming_distance_opposite() {
        let a = vec![0xFF];
        let b = vec![0x00];
        assert_eq!(hamming_distance(&a, &b), 8);
    }
}
