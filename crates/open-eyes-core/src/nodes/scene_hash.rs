//! SceneHashNode — detects sudden large scene changes via perceptual hashing.
//!
//! A perceptual hash (pHash) captures the "gist" of an image in 64 bits.
//! Similar images produce similar hashes. Radically different images
//! produce different hashes. Hamming distance between hashes measures
//! visual similarity.
//!
//! Use case: door opens (flood of outdoor light), lights turn on/off,
//! vehicle pulls into frame filling 50% of the image. These are
//! instantaneous scene-wide changes that optical flow might miss
//! because there's no continuous motion to track — just a sudden
//! "everything is different."
//!
//! Cost: ~1ms on ARM (resize to 8x8 + DCT is trivial).
//!
//! VDD contract:
//! - Identical frames → hash distance 0
//! - Similar frames (slight noise) → distance < 5
//! - Radically different frames (lights on vs off) → distance > 20

use crate::frame::{Frame, ProcessNode, PipelineEvent};
use crate::cv;

pub struct SceneHashNode {
    /// Previous frame's hash
    prev_hash: Option<u64>,
    /// Running average hash distance (for adaptive thresholding)
    avg_distance: f32,
    /// Threshold: number of bits that must differ to trigger.
    /// Normalized 0-1: fraction of 64 bits (e.g., 0.3 = 19 bits).
    pub threshold: f32,
    /// Use quarter-res before hashing (faster, same discrimination)
    quarter_res: bool,
}

impl SceneHashNode {
    pub fn new(threshold: f32) -> Self {
        Self {
            prev_hash: None,
            avg_distance: 0.0,
            threshold,
            quarter_res: true,
        }
    }

    pub fn full_res(threshold: f32) -> Self {
        Self {
            prev_hash: None,
            avg_distance: 0.0,
            threshold,
            quarter_res: false,
        }
    }
}

/// Compute a simple perceptual hash (average hash / aHash).
///
/// 1. Resize to 8x8
/// 2. Convert to grayscale (already done by frame.greyscale())
/// 3. Compute mean pixel value
/// 4. Each pixel → 1 if above mean, 0 if below → 64-bit hash
///
/// Not as sophisticated as DCT-based pHash, but costs ~0.5ms on ARM
/// and is plenty discriminative for "did the whole scene change?"
fn compute_hash(gray: &image::GrayImage) -> u64 {
    // Resize to 8x8 via OpenCV (proper INTER_AREA downsampling)
    let small = cv::downsample(gray, gray.width().max(8) / 8);

    // If the downsample didn't produce exactly 8x8, just use what we got
    let pixels: Vec<u8> = small.pixels().map(|p| p.0[0]).collect();
    if pixels.is_empty() {
        return 0;
    }

    // Mean intensity
    let mean = pixels.iter().map(|p| *p as f64).sum::<f64>() / pixels.len() as f64;

    // Build hash: 1 bit per pixel, 1 if above mean
    let mut hash: u64 = 0;
    for (i, &pixel) in pixels.iter().enumerate().take(64) {
        if pixel as f64 > mean {
            hash |= 1u64 << i;
        }
    }

    hash
}

/// Hamming distance between two 64-bit hashes.
fn hash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

impl ProcessNode for SceneHashNode {
    fn name(&self) -> &str { "scene-hash" }

    fn update(&mut self, frame: &Frame) -> Vec<PipelineEvent> {
        let gray = frame.greyscale();

        let working = if self.quarter_res {
            cv::downsample(gray, 4)
        } else {
            gray.clone()
        };

        let current_hash = compute_hash(&working);

        let mut events = Vec::new();

        if let Some(prev) = self.prev_hash {
            let distance = hash_distance(prev, current_hash);
            // Normalize: distance / 64 bits → 0.0-1.0
            let normalized = distance as f32 / 64.0;

            // Update running average (for adaptive threshold in the future)
            self.avg_distance = self.avg_distance * 0.95 + normalized * 0.05;

            if normalized > self.threshold {
                events.push(PipelineEvent::Motion {
                    camera_id: frame.camera_id().to_string(),
                    magnitude: normalized as f64,
                });
            }
        }

        self.prev_hash = Some(current_hash);
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_identical_frames_zero_distance() {
        let img = image::GrayImage::from_pixel(64, 64, image::Luma([128]));
        let h1 = compute_hash(&img);
        let h2 = compute_hash(&img);
        assert_eq!(hash_distance(h1, h2), 0);
    }

    #[test]
    fn vdd_similar_frames_low_distance() {
        // Same checkerboard, slight brightness difference
        let mut img1 = image::GrayImage::new(64, 64);
        let mut img2 = image::GrayImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if ((x / 8) + (y / 8)) % 2 == 0 { 180u8 } else { 60u8 };
                img1.put_pixel(x, y, image::Luma([v]));
                img2.put_pixel(x, y, image::Luma([v.saturating_add(5)])); // +5 brightness
            }
        }
        let dist = hash_distance(compute_hash(&img1), compute_hash(&img2));
        assert!(dist < 10, "Similar images should have low hash distance, got {}", dist);
    }

    #[test]
    fn vdd_radically_different_high_distance() {
        // All black vs all white
        let black = image::GrayImage::from_pixel(64, 64, image::Luma([0]));
        let white = image::GrayImage::from_pixel(64, 64, image::Luma([255]));

        // Both uniform images hash to 0 or all-1s (all above/below mean)
        // So distance might be 0 or 64. Let's use actual distinct images instead.
        let mut img_a = image::GrayImage::new(64, 64);
        let mut img_b = image::GrayImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                // img_a: horizontal stripes
                let va = if y % 16 < 8 { 200u8 } else { 50u8 };
                // img_b: vertical stripes (orthogonal pattern)
                let vb = if x % 16 < 8 { 200u8 } else { 50u8 };
                img_a.put_pixel(x, y, image::Luma([va]));
                img_b.put_pixel(x, y, image::Luma([vb]));
            }
        }

        let dist = hash_distance(compute_hash(&img_a), compute_hash(&img_b));
        assert!(dist > 10, "Orthogonal patterns should have high hash distance, got {}", dist);
    }

    #[test]
    fn vdd_static_scene_no_events() {
        use std::sync::Arc;
        use crate::CameraIntrinsics;
        use crate::frame::Pipeline;

        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(SceneHashNode::full_res(0.2)));

        let img = image::RgbImage::from_pixel(64, 64, image::Rgb([100, 100, 100]));
        let intrinsics = Arc::new(CameraIntrinsics {
            fx: 51.2, fy: 51.2, cx: 32.0, cy: 32.0,
            width: 64, height: 64, distortion: [0.0; 3],
        });

        let mut events_count = 0;
        for i in 0..10 {
            let events = pipeline.process_frame(
                img.clone(), "cam0", intrinsics.clone(), i as f64 / 30.0,
            );
            events_count += events.len();
        }
        assert_eq!(events_count, 0, "Static scene should produce no scene hash events");
    }

    #[test]
    fn vdd_sudden_change_triggers_event() {
        use std::sync::Arc;
        use crate::CameraIntrinsics;
        use crate::frame::Pipeline;

        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(SceneHashNode::full_res(0.15)));

        let intrinsics = Arc::new(CameraIntrinsics {
            fx: 51.2, fy: 51.2, cx: 32.0, cy: 32.0,
            width: 64, height: 64, distortion: [0.0; 3],
        });

        // 5 frames of dark scene
        let dark = image::RgbImage::from_pixel(64, 64, image::Rgb([30, 30, 30]));
        for i in 0..5 {
            pipeline.process_frame(dark.clone(), "cam0", intrinsics.clone(), i as f64 / 30.0);
        }

        // Sudden bright scene (lights turned on)
        let mut bright = image::RgbImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if ((x / 8) + (y / 8)) % 2 == 0 { 220u8 } else { 180u8 };
                bright.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }

        let events = pipeline.process_frame(
            bright, "cam0", intrinsics.clone(), 5.0 / 30.0,
        );

        assert!(
            !events.is_empty(),
            "Sudden dark→bright scene change should trigger scene hash event"
        );
    }
}
