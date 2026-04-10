//! BackgroundSubNode — cheapest possible change detector.
//!
//! Running average background model. Diffs current frame against reference.
//! If changed pixels exceed threshold → emit event. Cost: ~0.5ms on ARM.
//!
//! This is the bouncer at the door. It runs BEFORE optical flow.
//! If nothing changed, flow doesn't run. 99% of frames on a quiet
//! night are rejected here at 0.5ms instead of by flow at 5ms.
//!
//! VDD contract:
//! - Static scene → change_fraction ≈ 0.0
//! - Person entering → change_fraction > area_threshold in the person's region
//! - Lighting ramp → change_fraction stays low (background adapts)

use crate::frame::{Frame, ProcessNode, PipelineEvent};
use crate::cv;

pub struct BackgroundSubNode {
    /// Reference frame (grayscale, possibly quarter-res)
    reference: Option<image::GrayImage>,
    /// Learning rate: how fast the reference adapts (0.005 = slow, 0.1 = fast)
    pub alpha: f32,
    /// Per-pixel difference threshold (intensity units, 0-255)
    pixel_threshold: u8,
    /// Fraction of pixels that must change to trigger an event
    area_threshold: f32,
    /// Use quarter-res for speed
    quarter_res: bool,
    /// Frames since last reference update
    frames_since_init: u64,
}

impl BackgroundSubNode {
    pub fn new(pixel_threshold: u8, area_threshold: f32) -> Self {
        Self {
            reference: None,
            alpha: 0.005,
            pixel_threshold,
            area_threshold,
            quarter_res: true,
            frames_since_init: 0,
        }
    }

    pub fn full_res(pixel_threshold: u8, area_threshold: f32) -> Self {
        Self {
            reference: None,
            alpha: 0.005,
            pixel_threshold,
            area_threshold,
            quarter_res: false,
            frames_since_init: 0,
        }
    }

    /// Blend current frame into reference (slow adaptation).
    fn update_reference(&mut self, current: &image::GrayImage) {
        if let Some(ref mut reference) = self.reference {
            let alpha = self.alpha;
            for (r, c) in reference.pixels_mut().zip(current.pixels()) {
                let ref_val = r.0[0] as f32;
                let cur_val = c.0[0] as f32;
                r.0[0] = (ref_val * (1.0 - alpha) + cur_val * alpha) as u8;
            }
        }
    }
}

impl ProcessNode for BackgroundSubNode {
    fn name(&self) -> &str { "background-sub" }

    fn update(&mut self, frame: &Frame) -> Vec<PipelineEvent> {
        let gray = frame.greyscale();

        let current = if self.quarter_res {
            cv::downsample(gray, 4)
        } else {
            gray.clone()
        };

        self.frames_since_init += 1;

        // First frame: set as reference, no comparison
        if self.reference.is_none() {
            self.reference = Some(current);
            return Vec::new();
        }

        let ref_img = self.reference.as_ref().unwrap();

        // OpenCV background diff: absdiff + threshold + count
        let change_fraction = cv::background_diff(ref_img, &current, self.pixel_threshold);

        // Slowly adapt reference to current (handles gradual lighting changes)
        self.update_reference(&current);

        let mut events = Vec::new();

        if change_fraction > self.area_threshold {
            // Something changed significantly
            events.push(PipelineEvent::Motion {
                camera_id: frame.camera_id().to_string(),
                magnitude: change_fraction as f64,
            });
        }

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
    fn vdd_static_no_change() {
        let mut pipeline = Pipeline::new();
        // Low area threshold so even small changes would trigger
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.01)));

        let frame = image::RgbImage::from_pixel(160, 120, image::Rgb([100, 100, 100]));
        let intrinsics = test_intrinsics(160, 120);

        let mut motion_count = 0;
        for i in 0..20 {
            let events = pipeline.process_frame(
                frame.clone(), "cam0", intrinsics.clone(), i as f64 / 30.0,
            );
            for e in &events {
                if matches!(e, PipelineEvent::Motion { .. }) {
                    motion_count += 1;
                }
            }
        }

        assert_eq!(motion_count, 0, "Static scene should produce zero change events");
    }

    #[test]
    fn vdd_rectangle_appears_triggers_change() {
        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));

        let w = 160u32;
        let h = 120u32;
        let intrinsics = test_intrinsics(w, h);

        // 10 frames of static background
        let bg = image::RgbImage::from_pixel(w, h, image::Rgb([80, 80, 80]));
        for i in 0..10 {
            pipeline.process_frame(bg.clone(), "cam0", intrinsics.clone(), i as f64 / 30.0);
        }

        // Frame with a bright rectangle (significant change)
        let mut changed = image::RgbImage::from_pixel(w, h, image::Rgb([80, 80, 80]));
        for y in 30..90 {
            for x in 40..120 {
                changed.put_pixel(x, y, image::Rgb([220, 220, 220]));
            }
        }

        let events = pipeline.process_frame(
            changed, "cam0", intrinsics.clone(), 10.0 / 30.0,
        );

        let motion_events: Vec<_> = events.iter().filter(|e| {
            matches!(e, PipelineEvent::Motion { .. })
        }).collect();

        assert!(
            !motion_events.is_empty(),
            "Bright rectangle appearing should trigger a change event"
        );
    }

    #[test]
    fn vdd_gradual_lighting_adapts() {
        // Slow brightness ramp — background model adapts, few false triggers
        let mut node = BackgroundSubNode::full_res(30, 0.05);
        // Alpha must track the lighting change rate.
        // 0.5 intensity/frame change needs alpha ≥ ~0.02 to keep up.
        node.alpha = 0.05;
        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(node));

        let w = 160u32;
        let h = 120u32;
        let intrinsics = test_intrinsics(w, h);

        let mut false_positives = 0;

        for i in 0..100 {
            // Very gradual: 0.5 intensity units per frame
            let intensity = (80.0 + i as f32 * 0.5).min(255.0) as u8;
            let img = image::RgbImage::from_pixel(w, h, image::Rgb([intensity, intensity, intensity]));

            let events = pipeline.process_frame(
                img, "cam0", intrinsics.clone(), i as f64 / 30.0,
            );

            // Skip first few frames (reference still initializing)
            if i > 5 {
                for e in &events {
                    if matches!(e, PipelineEvent::Motion { .. }) {
                        false_positives += 1;
                    }
                }
            }
        }

        assert!(
            false_positives < 5,
            "Gradual lighting should cause < 5 false positives, got {}",
            false_positives
        );
    }
}
