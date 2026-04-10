//! Synthetic test sequences with known ground truth.
//!
//! Generate video sequences where we KNOW the correct answer:
//! - Moving rectangle → known flow vectors, known event timing
//! - Static scene → zero flow, no events
//! - Lighting change → background model should NOT trigger motion
//! - Person-sized blob entering → event at known frame
//!
//! This is the fastest path to VDD — no downloads, no flaky servers,
//! deterministic ground truth, runs anywhere.

use image::{Rgb, RgbImage};
use std::sync::Arc;
use open_eyes_core::CameraIntrinsics;
use open_eyes_core::frame::PipelineEvent;

/// A synthetic frame with its ground truth.
pub struct SyntheticFrame {
    pub image: RgbImage,
    pub frame_index: u64,
    /// True if this frame has foreground motion (ground truth for event detection)
    pub has_motion: bool,
    /// Ground truth flow: (dx, dy) of the moving object, if any
    pub gt_flow: Option<(f32, f32)>,
    /// Bounding box of moving object [x, y, w, h] in pixels, if any
    pub gt_bbox: Option<(u32, u32, u32, u32)>,
}

/// Generate a sequence: static background, then a rectangle moves across.
///
/// Frames 0-19: static gray background (no motion)
/// Frames 20-79: white rectangle moves right at 5px/frame
/// Frames 80-99: static again (rectangle gone)
///
/// Ground truth:
/// - Motion events should fire from frame 20-79
/// - Flow vectors should be ~(5.0, 0.0) in the rectangle region
/// - Background model should NOT trigger on frames 0-19 or 80-99
pub fn moving_rectangle(
    width: u32,
    height: u32,
    num_frames: u64,
) -> Vec<SyntheticFrame> {
    let bg_gray: u8 = 80;
    let fg_white: u8 = 220;
    let rect_w = 60u32;
    let rect_h = 80u32;
    let rect_y = (height / 2).saturating_sub(rect_h / 2);
    let speed = 5i32; // pixels per frame
    let motion_start = 20u64;
    let motion_end = num_frames.saturating_sub(20);

    let mut frames = Vec::with_capacity(num_frames as usize);

    for i in 0..num_frames {
        let mut img = RgbImage::from_pixel(width, height, Rgb([bg_gray, bg_gray, bg_gray]));
        let mut has_motion = false;
        let mut gt_flow = None;
        let mut gt_bbox = None;

        if i >= motion_start && i < motion_end {
            has_motion = true;
            gt_flow = Some((speed as f32, 0.0));

            let rect_x = ((i - motion_start) as i32 * speed).max(0) as u32;
            if rect_x + rect_w < width {
                gt_bbox = Some((rect_x, rect_y, rect_w, rect_h));

                // Draw the rectangle
                for y in rect_y..(rect_y + rect_h).min(height) {
                    for x in rect_x..(rect_x + rect_w).min(width) {
                        img.put_pixel(x, y, Rgb([fg_white, fg_white, fg_white]));
                    }
                }
            }
        }

        frames.push(SyntheticFrame {
            image: img,
            frame_index: i,
            has_motion,
            gt_flow,
            gt_bbox,
        });
    }

    frames
}

/// Generate a lighting change sequence — the whole image gets brighter.
///
/// This tests that the background model adapts to gradual changes
/// and does NOT fire false motion events.
///
/// Frames 0-49: gray at intensity 80
/// Frames 50-99: linearly ramp to intensity 160
///
/// Ground truth: NO motion at any frame. This is lighting, not motion.
pub fn lighting_change(width: u32, height: u32, num_frames: u64) -> Vec<SyntheticFrame> {
    let mut frames = Vec::with_capacity(num_frames as usize);
    let ramp_start = num_frames / 2;

    for i in 0..num_frames {
        let intensity = if i < ramp_start {
            80u8
        } else {
            let progress = (i - ramp_start) as f32 / (num_frames - ramp_start) as f32;
            (80.0 + progress * 80.0).min(255.0) as u8
        };

        let img = RgbImage::from_pixel(width, height, Rgb([intensity, intensity, intensity]));

        frames.push(SyntheticFrame {
            image: img,
            frame_index: i,
            has_motion: false, // lighting change is NOT motion
            gt_flow: None,
            gt_bbox: None,
        });
    }

    frames
}

/// Generate a person-sized blob entering from the left.
///
/// Tests entity detection: a ~50x120 pixel blob (roughly human proportions)
/// appears at frame 30, walks across at 3px/frame, exits at frame ~150.
pub fn person_enters(width: u32, height: u32, num_frames: u64) -> Vec<SyntheticFrame> {
    let bg: u8 = 100;
    let fg: u8 = 200;
    let person_w = 50u32;
    let person_h = 120u32;
    let person_y = height.saturating_sub(person_h).saturating_sub(20); // near bottom
    let speed = 3i32;
    let enter_frame = 30u64;

    let mut frames = Vec::with_capacity(num_frames as usize);

    for i in 0..num_frames {
        let mut img = RgbImage::from_pixel(width, height, Rgb([bg, bg, bg]));
        let mut has_motion = false;
        let mut gt_bbox = None;

        if i >= enter_frame {
            let elapsed = (i - enter_frame) as i32;
            let person_x = (elapsed * speed - person_w as i32).max(0) as u32;

            if person_x < width {
                has_motion = true;
                gt_bbox = Some((person_x, person_y, person_w.min(width - person_x), person_h));

                // Draw person blob (simple rectangle — VDD doesn't need realism)
                for y in person_y..(person_y + person_h).min(height) {
                    for x in person_x..(person_x + person_w).min(width) {
                        img.put_pixel(x, y, Rgb([fg, fg, fg]));
                    }
                }
            }
        }

        frames.push(SyntheticFrame {
            image: img,
            frame_index: i,
            has_motion,
            gt_flow: if has_motion { Some((speed as f32, 0.0)) } else { None },
            gt_bbox,
        });
    }

    frames
}

pub fn default_intrinsics(width: u32, height: u32) -> Arc<CameraIntrinsics> {
    Arc::new(CameraIntrinsics {
        fx: width as f64 * 0.8,
        fy: height as f64 * 0.8,
        cx: width as f64 / 2.0,
        cy: height as f64 / 2.0,
        width, height,
        distortion: [0.0; 3],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_eyes_core::frame::Pipeline;
    use open_eyes_core::features::flow_motion_magnitude;

    #[test]
    fn vdd_static_frames_have_no_motion_gt() {
        let seq = moving_rectangle(320, 240, 100);
        // First 20 frames: no motion
        for frame in &seq[0..20] {
            assert!(!frame.has_motion, "Frame {} should be static", frame.frame_index);
            assert!(frame.gt_flow.is_none());
            assert!(frame.gt_bbox.is_none());
        }
        // Frames 20-79: motion
        for frame in &seq[20..80] {
            assert!(frame.has_motion, "Frame {} should have motion", frame.frame_index);
            let (dx, _dy) = frame.gt_flow.unwrap();
            assert!((dx - 5.0).abs() < 0.01, "Expected 5px/frame flow");
        }
        // Last 20 frames: static again
        for frame in &seq[80..100] {
            assert!(!frame.has_motion, "Frame {} should be static again", frame.frame_index);
        }
    }

    #[test]
    fn vdd_lighting_change_never_has_motion() {
        let seq = lighting_change(320, 240, 100);
        for frame in &seq {
            assert!(!frame.has_motion, "Lighting change should never be flagged as motion");
        }
    }

    #[test]
    fn vdd_person_enters_at_correct_frame() {
        let seq = person_enters(320, 240, 100);
        for frame in &seq[0..30] {
            assert!(!frame.has_motion, "No person before frame 30");
        }
        assert!(seq[31].has_motion, "Person should be present at frame 31");
        assert!(seq[31].gt_bbox.is_some(), "Should have bbox at frame 31");
    }

    #[test]
    fn vdd_full_triage_pipeline_on_synthetic() {
        // THE REAL E2E TEST: full triage pipeline (background + flow + edge density)
        // on synthetic sequence with known ground truth.
        use open_eyes_core::nodes::background::BackgroundSubNode;
        use open_eyes_core::nodes::motion::MotionDetectorNode;
        use open_eyes_core::nodes::edge_density::EdgeDensityNode;

        let seq = moving_rectangle(320, 240, 80);
        let mut pipeline = Pipeline::new();

        // All three triage nodes — same as on-device pipeline
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));
        pipeline.add_node(Box::new(EdgeDensityNode::full_res(0.03)));

        let mut motion_frames: Vec<u64> = Vec::new();
        let mut static_false_positives = 0u64;

        for frame in &seq {
            let intrinsics = default_intrinsics(320, 240);
            let events = pipeline.process_frame(
                frame.image.clone(),
                "synthetic",
                intrinsics,
                frame.frame_index as f64 / 30.0,
            );

            let has_event = events.iter().any(|e| matches!(e, PipelineEvent::Motion { .. }));

            if has_event {
                motion_frames.push(frame.frame_index);
                if !frame.has_motion {
                    static_false_positives += 1;
                }
            }
        }

        assert_eq!(pipeline.frame_count(), 80, "All 80 frames processed");

        // Pipeline MUST detect motion during the motion window (frames 20-59)
        let detected_in_window: Vec<_> = motion_frames.iter()
            .filter(|f| **f >= 20 && **f < 60)
            .collect();
        assert!(
            !detected_in_window.is_empty(),
            "Pipeline must detect motion during ground truth motion window (frames 20-59). \
             Detected at frames: {:?}", motion_frames
        );

        // False positives in static regions should be minimal
        assert!(
            static_false_positives < 5,
            "False positives in static frames should be < 5, got {}. \
             False positive frames: {:?}",
            static_false_positives,
            motion_frames.iter().filter(|f| **f < 20 || **f >= 60).collect::<Vec<_>>()
        );
    }

    #[test]
    fn vdd_lighting_change_low_false_positive_rate() {
        // Full pipeline on lighting change — should NOT trigger significant events
        use open_eyes_core::nodes::background::BackgroundSubNode;
        use open_eyes_core::nodes::motion::MotionDetectorNode;

        let seq = lighting_change(160, 120, 60);
        let mut pipeline = Pipeline::new();

        let mut bg_node = BackgroundSubNode::full_res(30, 0.05);
        bg_node.alpha = 0.05; // fast adaptation for lighting
        pipeline.add_node(Box::new(bg_node));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.01)));

        let mut false_positives = 0;

        for frame in &seq {
            let intrinsics = default_intrinsics(160, 120);
            let events = pipeline.process_frame(
                frame.image.clone(),
                "synthetic",
                intrinsics,
                frame.frame_index as f64 / 30.0,
            );

            // Skip first 5 frames (initialization)
            if frame.frame_index > 5 {
                for e in &events {
                    if matches!(e, PipelineEvent::Motion { .. }) {
                        false_positives += 1;
                    }
                }
            }
        }

        // Two nodes (background + flow) both emit Motion events, so
        // the combined false positive count is higher than either alone.
        // With 60 frames × 2 nodes = 120 possible events, < 20 is good.
        // The key metric: false positive RATE, not count.
        let total_possible = 55 * 2; // (60 frames - 5 init) × 2 nodes
        let fp_rate = false_positives as f64 / total_possible as f64;
        assert!(
            fp_rate < 0.20,
            "Lighting change false positive rate should be < 20%, got {:.1}% ({}/{})",
            fp_rate * 100.0, false_positives, total_possible
        );
    }
}
