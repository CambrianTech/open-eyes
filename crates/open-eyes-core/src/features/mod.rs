//! features — Feature extraction, tracking, and optical flow
//!
//! The architecture has TWO tiers:
//!
//! **Tier 1: Optical flow (synchronous, every frame, GPU, low-res)**
//! The ONE process that runs at full frame rate. Computes at quarter
//! resolution on GPU textures (160x120 from 640x480) because that's
//! all you need for motion vectors. This is the heartbeat — if flow
//! says nothing's moving, everything else sleeps.
//!
//! **Tier 2: Feature extraction (lazy, on-demand, CPU or GPU)**
//! ORB/FAST keypoints + descriptors, computed only when the pipeline
//! needs cross-camera matching or entity tracking. Triggered by
//! optical flow detecting motion, not by every frame.
//!
//! Platform adapters: if the device provides pose/tracking natively
//! (ARKit, ARCore, Tango), wrap it as an adapter. Same principle as
//! continuum's "adapters not branches" — use what the platform gives
//! you for free, supplement with pure-CV only where needed.

use crate::frame::FlowField;

/// A detected feature point with descriptor.
#[derive(Debug, Clone)]
pub struct FeaturePoint {
    /// 2D position in the image
    pub pixel: (f64, f64),
    /// Binary descriptor (ORB: 32 bytes = 256 bits)
    pub descriptor: Vec<u8>,
    /// Persistent track ID (assigned by the tracker across frames)
    pub track_id: Option<u64>,
}

/// Optical flow configuration.
///
/// Flow runs at reduced resolution on GPU for speed. The resolution
/// is the only tunable that matters — lower = faster but coarser
/// motion detection. For security cameras, quarter-res is plenty.
#[derive(Debug, Clone)]
pub struct FlowConfig {
    /// Scale factor for the flow computation (0.25 = quarter res)
    pub scale: f32,
    /// Motion magnitude threshold — below this, consider "no motion"
    pub motion_threshold: f32,
    /// Whether to use GPU compute (wgpu) or CPU fallback
    pub use_gpu: bool,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            scale: 0.25,           // quarter resolution
            motion_threshold: 2.0, // pixels of flow to count as motion
            use_gpu: true,         // prefer GPU
        }
    }
}

/// Summarize a flow field into a single motion magnitude.
///
/// Used by the pipeline to decide whether anything is happening
/// in a camera's field of view. If the motion magnitude is below
/// the threshold, downstream nodes don't need to wake up.
pub fn flow_motion_magnitude(flow: &FlowField) -> f64 {
    if flow.vectors.is_empty() {
        return 0.0;
    }

    // Median magnitude — robust to outliers (a single hot pixel
    // doesn't trigger false motion detection)
    let mut magnitudes: Vec<f32> = flow.vectors
        .iter()
        .map(|(dx, dy)| (dx * dx + dy * dy).sqrt())
        .collect();
    magnitudes.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // 75th percentile — captures "most of the frame is moving"
    // without being dominated by the maximum outlier
    let idx = magnitudes.len() * 3 / 4;
    magnitudes[idx] as f64
}

/// Detect whether a camera has moved globally (drift detection).
///
/// Compares the flow field's GLOBAL motion pattern. If the entire
/// frame shifts uniformly (all vectors point the same direction),
/// the camera itself moved — not the scene content. Returns the
/// estimated camera motion in pixels.
pub fn detect_camera_drift(flow: &FlowField) -> f64 {
    if flow.vectors.is_empty() {
        return 0.0;
    }

    // Mean flow vector — if the camera moved, this is non-zero
    // and roughly uniform across the frame
    let n = flow.vectors.len() as f64;
    let mean_dx: f64 = flow.vectors.iter().map(|(dx, _)| *dx as f64).sum::<f64>() / n;
    let mean_dy: f64 = flow.vectors.iter().map(|(_, dy)| *dy as f64).sum::<f64>() / n;

    (mean_dx * mean_dx + mean_dy * mean_dy).sqrt()
}

/// Hamming distance between two binary descriptors.
///
/// Used for ORB feature matching. Each XOR bit is a mismatch;
/// count_ones() gives total mismatches.
pub fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

/// Match features between two sets by descriptor distance.
///
/// Returns matched pairs as (index_a, index_b, distance).
/// Uses Lowe's ratio test to reject ambiguous matches.
pub fn match_features(
    features_a: &[FeaturePoint],
    features_b: &[FeaturePoint],
    max_distance: u32,
) -> Vec<(usize, usize, u32)> {
    let mut matches = Vec::new();

    for (i, fa) in features_a.iter().enumerate() {
        let mut best_dist = u32::MAX;
        let mut second_dist = u32::MAX;
        let mut best_j = 0;

        for (j, fb) in features_b.iter().enumerate() {
            let dist = hamming_distance(&fa.descriptor, &fb.descriptor);
            if dist < best_dist {
                second_dist = best_dist;
                best_dist = dist;
                best_j = j;
            } else if dist < second_dist {
                second_dist = dist;
            }
        }

        // Lowe's ratio test: best match must be significantly better
        // than second-best to be considered a good match
        if best_dist < max_distance && (best_dist as f64) < 0.75 * (second_dist as f64) {
            matches.push((i, best_j, best_dist));
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hamming_identical() {
        let a = vec![0xFF, 0x00, 0xAA];
        assert_eq!(hamming_distance(&a, &a), 0);
    }

    #[test]
    fn hamming_opposite() {
        assert_eq!(hamming_distance(&[0xFF], &[0x00]), 8);
    }

    #[test]
    fn flow_motion_zero_for_static() {
        let flow = FlowField {
            width: 4, height: 4,
            vectors: vec![(0.0, 0.0); 16],
        };
        assert_eq!(flow_motion_magnitude(&flow), 0.0);
    }

    #[test]
    fn flow_motion_detects_movement() {
        let flow = FlowField {
            width: 4, height: 4,
            vectors: vec![(5.0, 5.0); 16], // everything moving
        };
        assert!(flow_motion_magnitude(&flow) > 5.0);
    }

    #[test]
    fn camera_drift_zero_for_static() {
        let flow = FlowField {
            width: 4, height: 4,
            vectors: vec![(0.0, 0.0); 16],
        };
        assert_eq!(detect_camera_drift(&flow), 0.0);
    }

    #[test]
    fn camera_drift_detects_uniform_shift() {
        let flow = FlowField {
            width: 4, height: 4,
            vectors: vec![(10.0, 0.0); 16], // whole frame shifted right
        };
        assert!((detect_camera_drift(&flow) - 10.0).abs() < 0.1);
    }

    #[test]
    fn match_features_basic() {
        let a = vec![
            FeaturePoint { pixel: (0.0, 0.0), descriptor: vec![0xFF; 32], track_id: None },
            FeaturePoint { pixel: (1.0, 1.0), descriptor: vec![0x00; 32], track_id: None },
        ];
        let b = vec![
            FeaturePoint { pixel: (2.0, 2.0), descriptor: vec![0x00; 32], track_id: None },
            FeaturePoint { pixel: (3.0, 3.0), descriptor: vec![0xFE; 32], track_id: None },
        ];
        let matches = match_features(&a, &b, 64);
        // a[0] (0xFF) should match b[1] (0xFE) — 8 bit diff per byte × 32 bytes = close
        // a[1] (0x00) should match b[0] (0x00) — exact match
        assert!(!matches.is_empty());
    }
}
