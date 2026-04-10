//! End-to-end integration test: synthetic frames → pipeline → bridge → grid events.
//!
//! This tests the FULL path from raw pixels to grid events that continuum
//! personas would subscribe to. No mocks, no stubs — real OpenCV pipeline,
//! real event conversion, real serialization.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use open_eyes_core::CameraIntrinsics;
    use open_eyes_core::frame::{Pipeline, PipelineEvent};
    use open_eyes_core::nodes::background::BackgroundSubNode;
    use open_eyes_core::nodes::motion::MotionDetectorNode;
    use open_eyes_core::nodes::edge_density::EdgeDensityNode;
    use crate::bridge::pipeline_to_grid;
    use crate::events::MotionPayload;
    use crate::node::{OpenEyesNode, NodeConfig};

    fn test_intrinsics(w: u32, h: u32) -> Arc<CameraIntrinsics> {
        Arc::new(CameraIntrinsics {
            fx: w as f64 * 0.8, fy: h as f64 * 0.8,
            cx: w as f64 / 2.0, cy: h as f64 / 2.0,
            width: w, height: h,
            distortion: [0.0; 3],
        })
    }

    #[test]
    fn vdd_e2e_static_scene_no_grid_events() {
        // Static scene → pipeline → bridge → zero grid events
        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));

        let w = 160u32;
        let h = 120u32;
        let frame = image::RgbImage::from_pixel(w, h, image::Rgb([100, 100, 100]));
        let intrinsics = test_intrinsics(w, h);

        let mut total_grid_events = 0;

        for i in 0..20 {
            let events = pipeline.process_frame(
                frame.clone(), "cam-0", intrinsics.clone(), i as f64 / 30.0,
            );
            let grid_events = pipeline_to_grid("test-node", &events);
            total_grid_events += grid_events.len();
        }

        assert_eq!(total_grid_events, 0, "Static scene should produce zero grid events");
    }

    #[test]
    fn vdd_e2e_motion_produces_grid_events() {
        // Moving rectangle → pipeline → bridge → grid motion events
        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));
        pipeline.add_node(Box::new(EdgeDensityNode::full_res(0.03)));

        let w = 320u32;
        let h = 240u32;
        let intrinsics = test_intrinsics(w, h);

        let mut grid_events_by_frame: Vec<(u64, usize)> = Vec::new();
        let mut all_grid_events = Vec::new();

        for i in 0..40u64 {
            // Textured background
            let mut img = image::RgbImage::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let v = if ((x / 16) + (y / 16)) % 2 == 0 { 90u8 } else { 70u8 };
                    img.put_pixel(x, y, image::Rgb([v, v, v]));
                }
            }

            // Moving rectangle from frame 10 onward
            if i >= 10 {
                let rect_x = ((i - 10) * 8) as u32;
                for y in 60..(160.min(h)) {
                    for x in rect_x..(rect_x + 80).min(w) {
                        let stripe = ((x + y) / 8) % 2;
                        let val = if stripe == 0 { 220u8 } else { 180u8 };
                        img.put_pixel(x, y, image::Rgb([val, val, val]));
                    }
                }
            }

            let pipeline_events = pipeline.process_frame(
                img, "cam-0", intrinsics.clone(), i as f64 / 30.0,
            );
            let grid_events = pipeline_to_grid("test-node", &pipeline_events);

            grid_events_by_frame.push((i, grid_events.len()));
            all_grid_events.extend(grid_events);
        }

        // Must have some grid events during motion window
        assert!(
            !all_grid_events.is_empty(),
            "Moving rectangle should produce grid events"
        );

        // All grid events should have correct topic
        for event in &all_grid_events {
            assert!(
                event.topic.starts_with("camera:"),
                "Grid event topic should be camera:*, got: {}",
                event.topic
            );
            assert_eq!(event.node_id, "test-node");
        }

        // Motion events should have valid payloads
        let motion_events: Vec<_> = all_grid_events.iter()
            .filter(|e| e.topic == "camera:motion:detected")
            .collect();

        for event in &motion_events {
            let payload: MotionPayload = serde_json::from_value(event.payload.clone())
                .expect("Motion event should have valid MotionPayload");
            assert_eq!(payload.camera_id, "cam-0");
            assert!(payload.magnitude > 0.0);
            assert!(payload.magnitude < 1.0, "Normalized magnitude should be < 1.0");
        }
    }

    #[test]
    fn vdd_e2e_node_collects_events() {
        // Full integration: pipeline → bridge → node → drain
        let mut node = OpenEyesNode::new(NodeConfig {
            node_id: "test-node".into(),
            name: Some("Test Camera Node".into()),
            ..NodeConfig::default()
        });

        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));

        let w = 320u32;
        let h = 240u32;
        let intrinsics = test_intrinsics(w, h);

        // Feed 5 static frames (build background model)
        let bg = image::RgbImage::from_pixel(w, h, image::Rgb([100, 100, 100]));
        for i in 0..5 {
            let events = pipeline.process_frame(
                bg.clone(), "cam-0", intrinsics.clone(), i as f64 / 30.0,
            );
            for ge in pipeline_to_grid(&node.config.node_id, &events) {
                node.emit(ge);
            }
        }

        // Should have no events yet
        assert!(node.drain_events().is_empty(), "Static frames → no events");

        // Feed a frame with a bright rectangle (sudden change)
        let mut changed = image::RgbImage::from_pixel(w, h, image::Rgb([100, 100, 100]));
        for y in 40..180 {
            for x in 60..260 {
                changed.put_pixel(x, y, image::Rgb([240, 240, 240]));
            }
        }
        let events = pipeline.process_frame(
            changed, "cam-0", intrinsics.clone(), 5.0 / 30.0,
        );
        for ge in pipeline_to_grid(&node.config.node_id, &events) {
            node.emit(ge);
        }

        let drained = node.drain_events();
        assert!(
            !drained.is_empty(),
            "Sudden bright rectangle should produce grid events on the node"
        );

        // Events should serialize cleanly (this is what goes over IPC)
        for event in &drained {
            let json = serde_json::to_string(event).unwrap();
            assert!(json.len() > 10, "Event JSON should be non-trivial");
            // Round-trip: deserialize back
            let _: crate::events::GridEvent = serde_json::from_str(&json).unwrap();
        }
    }
}
