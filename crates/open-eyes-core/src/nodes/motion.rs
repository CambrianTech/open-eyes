//! MotionDetectorNode — detects motion via optical flow magnitude.
//!
//! The CBAR heartbeat: compute optical flow between consecutive frames,
//! report magnitude. This is the Tier 1 detector that gates everything
//! else — if flow says nothing moved, the expensive stuff doesn't run.
//!
//! VDD contract:
//! - Synthetic 5px/frame translation → magnitude ≈ 5.0
//! - Static scene (identical frames) → magnitude ≈ 0.0
//! - Lighting change (brightness ramp) → magnitude below threshold (NOT motion)

use crate::frame::{Frame, ProcessNode, PipelineEvent};
use crate::features::flow_motion_magnitude;
use crate::cv;

/// Detects motion by comparing consecutive frames via optical flow.
pub struct MotionDetectorNode {
    /// Previous frame's grayscale (for flow computation between frames)
    prev_gray: Option<image::GrayImage>,
    /// Magnitude threshold as fraction of frame diagonal (resolution-independent).
    /// 0.001 = motion of 0.1% of diagonal. NEVER hardcode to pixel count.
    /// On 1920x1080 (diag=2203): 0.001 → 2.2px threshold
    /// On 160x120 (diag=200): 0.001 → 0.2px threshold
    /// Same sensitivity regardless of resolution.
    pub threshold_fraction: f64,
    /// Use quarter-res for speed (like CBAR's tier-1 heartbeat)
    pub quarter_res: bool,
}

impl MotionDetectorNode {
    pub fn new(threshold_fraction: f64) -> Self {
        Self {
            prev_gray: None,
            threshold_fraction,
            quarter_res: true,
        }
    }

    /// Full-res mode (for grid node evaluation, not on-device)
    pub fn full_res(threshold_fraction: f64) -> Self {
        Self {
            prev_gray: None,
            threshold_fraction,
            quarter_res: false,
        }
    }

    /// Convert fraction-of-diagonal to absolute pixel threshold for a given frame size.
    fn pixel_threshold(&self, width: u32, height: u32) -> f64 {
        let diagonal = ((width * width + height * height) as f64).sqrt();
        self.threshold_fraction * diagonal
    }
}

impl ProcessNode for MotionDetectorNode {
    fn name(&self) -> &str { "motion-detector" }

    fn update(&mut self, frame: &Frame) -> Vec<PipelineEvent> {
        let gray = frame.greyscale();

        // Downsample if quarter-res mode
        let current = if self.quarter_res {
            cv::downsample(gray, 4)
        } else {
            gray.clone()
        };

        let mut events = Vec::new();

        if let Some(ref prev) = self.prev_gray {
            // Compute dense flow between previous and current
            let flow = cv::compute_dense_flow(prev, &current);
            let raw_magnitude = flow_motion_magnitude(&flow);

            // Normalize to 0-1: divide by frame diagonal.
            // A flow of 1 full diagonal = 1.0. Resolution-independent.
            let (w, h) = (current.width(), current.height());
            let diagonal = ((w * w + h * h) as f64).sqrt();
            let normalized = if diagonal > 0.0 { raw_magnitude / diagonal } else { 0.0 };

            if normalized > self.threshold_fraction {
                events.push(PipelineEvent::Motion {
                    camera_id: frame.camera_id().to_string(),
                    magnitude: normalized,
                });
            }
        }

        // Store current for next frame's comparison
        self.prev_gray = Some(current);

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::CameraIntrinsics;
    use crate::frame::Pipeline;

    fn test_intrinsics(w: u32, h: u32) -> Arc<CameraIntrinsics> {
        Arc::new(CameraIntrinsics {
            fx: w as f64 * 0.8, fy: h as f64 * 0.8,
            cx: w as f64 / 2.0, cy: h as f64 / 2.0,
            width: w, height: h,
            distortion: [0.0; 3],
        })
    }

    #[test]
    fn vdd_static_scene_no_motion_events() {
        // Same frame repeated → zero motion → no events
        let mut pipeline = Pipeline::new();
        // threshold_fraction = 0.001 → needs 0.1% of diagonal motion
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.001)));

        let frame = image::RgbImage::from_pixel(160, 120, image::Rgb([100, 100, 100]));
        let intrinsics = test_intrinsics(160, 120);

        let mut all_events = Vec::new();
        for i in 0..10 {
            let events = pipeline.process_frame(
                frame.clone(), "cam0", intrinsics.clone(), i as f64 / 30.0,
            );
            all_events.extend(events);
        }

        // Static scene: no motion events should fire
        let motion_events: Vec<_> = all_events.iter().filter(|e| {
            matches!(e, PipelineEvent::Motion { .. })
        }).collect();
        assert!(
            motion_events.is_empty(),
            "Static scene should produce zero motion events, got {}",
            motion_events.len()
        );
    }

    #[test]
    fn vdd_moving_rectangle_triggers_motion() {
        // White rectangle moving across gray background → motion detected
        let mut pipeline = Pipeline::new();
        // 0.005 = motion of 0.5% of diagonal needed to trigger
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));

        let w = 320u32;
        let h = 240u32;
        let intrinsics = test_intrinsics(w, h);

        let mut all_events = Vec::new();

        for i in 0..30 {
            // Textured background (checkerboard) — Farneback needs gradients
            let mut img = image::RgbImage::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let checker = ((x / 16) + (y / 16)) % 2;
                    let val = if checker == 0 { 90u8 } else { 70u8 };
                    img.put_pixel(x, y, image::Rgb([val, val, val]));
                }
            }

            // Draw a textured rectangle that moves right at 8px/frame starting at frame 5
            if i >= 5 {
                let rect_x = ((i - 5) * 8) as u32;
                let rect_y = 60u32;
                for y in rect_y..(rect_y + 100).min(h) {
                    for x in rect_x..(rect_x + 80).min(w) {
                        // Textured foreground (different pattern from background)
                        let stripe = ((x + y) / 8) % 2;
                        let val = if stripe == 0 { 220u8 } else { 180u8 };
                        img.put_pixel(x, y, image::Rgb([val, val, val]));
                    }
                }
            }

            let events = pipeline.process_frame(
                img, "cam0", intrinsics.clone(), i as f64 / 30.0,
            );
            all_events.extend(events);
        }

        let motion_events: Vec<_> = all_events.iter().filter(|e| {
            matches!(e, PipelineEvent::Motion { .. })
        }).collect();

        // Must detect motion — a rectangle is moving across the frame
        assert!(
            !motion_events.is_empty(),
            "Moving rectangle should trigger motion events"
        );

        // Verify magnitude is reasonable (not zero, not insane)
        for event in &motion_events {
            if let PipelineEvent::Motion { magnitude, .. } = event {
                assert!(*magnitude > 0.0, "Normalized motion should be positive");
                assert!(*magnitude < 1.0, "Normalized motion should be < 1.0, got {}", magnitude);
            }
        }
    }

    #[test]
    fn vdd_lighting_change_minimal_false_positives() {
        // Gradual brightness ramp → should produce very few motion events
        // (some might trigger during the initial ramp, but steady state should be quiet)
        let mut pipeline = Pipeline::new();
        // High threshold — lighting changes should stay well below this
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.01)));

        let w = 160u32;
        let h = 120u32;
        let intrinsics = test_intrinsics(w, h);

        let mut motion_count = 0;

        for i in 0..60 {
            // Gradual brightness increase: 80 → 160 over 60 frames
            let intensity = (80.0 + (i as f32 / 60.0) * 80.0) as u8;
            let img = image::RgbImage::from_pixel(w, h, image::Rgb([intensity, intensity, intensity]));

            let events = pipeline.process_frame(
                img, "cam0", intrinsics.clone(), i as f64 / 30.0,
            );

            for event in &events {
                if matches!(event, PipelineEvent::Motion { .. }) {
                    motion_count += 1;
                }
            }
        }

        // Lighting change should trigger very few motion events
        // A 1.3px/frame brightness gradient across the whole image will show
        // some flow, but with threshold=1.0, most frames should be below it
        assert!(
            motion_count < 10,
            "Lighting change should cause < 10 false motion events, got {}",
            motion_count
        );
    }
}
