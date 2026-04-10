//! Full end-to-end demo: video → pipeline → tracker → grid events.
//!
//! This is the complete path from "pixels enter the system" to
//! "continuum personas can see what's happening."
//!
//! video frames
//!   → triage nodes (background, flow, edge density, scene hash)
//!     → entity tracker (build persistent tracks)
//!       → grid bridge (convert to grid events)
//!         → node (queue for IPC transmission)
//!           → continuum EventBridge → personas subscribe

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use open_eyes_core::CameraIntrinsics;
    use open_eyes_core::frame::{Pipeline, PipelineEvent};
    use open_eyes_core::nodes::background::BackgroundSubNode;
    use open_eyes_core::nodes::motion::MotionDetectorNode;
    use open_eyes_core::nodes::edge_density::EdgeDensityNode;
    use open_eyes_core::nodes::scene_hash::SceneHashNode;
    use open_eyes_detect::tracker::{EntityTracker, TrackerConfig, Observation};
    use open_eyes_detect::EntityClass;
    use crate::bridge::{pipeline_to_grid, entities_to_grid};
    use crate::node::{OpenEyesNode, NodeConfig};

    fn intrinsics(w: u32, h: u32) -> Arc<CameraIntrinsics> {
        Arc::new(CameraIntrinsics {
            fx: w as f64 * 0.8, fy: h as f64 * 0.8,
            cx: w as f64 / 2.0, cy: h as f64 / 2.0,
            width: w, height: h,
            distortion: [0.0; 3],
        })
    }

    /// Generate a textured background frame.
    fn background(w: u32, h: u32) -> image::RgbImage {
        let mut img = image::RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if ((x / 16) + (y / 16)) % 2 == 0 { 90u8 } else { 70u8 };
                img.put_pixel(x, y, image::Rgb([v, v, v]));
            }
        }
        img
    }

    /// Add a "person" blob to a frame at the given position.
    fn add_person(img: &mut image::RgbImage, cx: u32, cy: u32) {
        let w = img.width();
        let h = img.height();
        let pw = 40u32; // person width
        let ph = 80u32; // person height
        let x0 = cx.saturating_sub(pw / 2);
        let y0 = cy.saturating_sub(ph / 2);
        for y in y0..(y0 + ph).min(h) {
            for x in x0..(x0 + pw).min(w) {
                let stripe = ((x + y) / 6) % 2;
                let val = if stripe == 0 { 210u8 } else { 170u8 };
                img.put_pixel(x, y, image::Rgb([val, val, val]));
            }
        }
    }

    #[test]
    fn vdd_e2e_full_pipeline_to_grid_events() {
        let w = 320u32;
        let h = 240u32;
        let int = intrinsics(w, h);

        // ── Set up the full stack ──
        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));
        pipeline.add_node(Box::new(EdgeDensityNode::full_res(0.03)));
        pipeline.add_node(Box::new(SceneHashNode::full_res(0.2)));

        let mut tracker = EntityTracker::new(TrackerConfig::default());
        let mut node = OpenEyesNode::new(NodeConfig {
            node_id: "demo-node".into(),
            ..NodeConfig::default()
        });

        // ── Phase 1: 10 frames of static background ──
        for i in 0..10 {
            let img = background(w, h);
            let events = pipeline.process_frame(img, "cam-0", int.clone(), i as f64 / 30.0);
            let grid_events = pipeline_to_grid("demo-node", &events);
            for ge in grid_events { node.emit(ge); }
        }

        let phase1_events = node.drain_events();
        // Static: should have very few (ideally zero) events
        assert!(
            phase1_events.len() < 5,
            "Static phase should have < 5 events, got {}",
            phase1_events.len()
        );

        // ── Phase 2: Person walks across frame (20 frames) ──
        let mut motion_events_count = 0;
        for i in 10..30 {
            let mut img = background(w, h);
            let person_x = 40 + ((i - 10) * 12) as u32; // walks right
            add_person(&mut img, person_x, 140);

            let t = i as f64 / 30.0;
            let events = pipeline.process_frame(img, "cam-0", int.clone(), t);

            // Feed motion events to tracker
            for event in &events {
                if let PipelineEvent::Motion { magnitude, .. } = event {
                    // Convert pipeline motion to a tracker observation
                    // In real system: the motion region centroid becomes the observation position
                    let norm_x = person_x as f64 / w as f64;
                    tracker.observe(&Observation {
                        position: [norm_x, 140.0 / h as f64],
                        timestamp: t,
                        camera_id: "cam-0".into(),
                        class: Some(EntityClass::Unknown), // no ML detector yet
                        confidence: *magnitude,
                    });
                    motion_events_count += 1;
                }
            }

            let grid_events = pipeline_to_grid("demo-node", &events);
            for ge in grid_events { node.emit(ge); }
        }

        // ── Verify: motion was detected ──
        let phase2_events = node.drain_events();
        assert!(
            !phase2_events.is_empty(),
            "Person walking should produce grid events"
        );
        assert!(
            motion_events_count > 0,
            "Pipeline should fire motion events for walking person"
        );

        // ── Verify: entity was tracked ──
        assert!(
            tracker.count() > 0,
            "Tracker should have at least one entity"
        );

        let entity = &tracker.entities()[0];
        assert!(entity.distance > 0.0, "Entity should have traveled");
        assert!(entity.cameras.contains(&"cam-0".to_string()));

        // ── Phase 3: Convert tracked entities to grid events ──
        let entity_events = entities_to_grid("demo-node", tracker.entities());
        assert!(
            !entity_events.is_empty(),
            "Should produce entity grid events"
        );

        for event in &entity_events {
            assert_eq!(event.topic, "scene:entity:tracked");
            assert_eq!(event.node_id, "demo-node");

            // Verify payload has required fields
            let payload = &event.payload;
            assert!(payload.get("entity_id").is_some());
            assert!(payload.get("position").is_some());
            assert!(payload.get("velocity").is_some());
            assert!(payload.get("cameras").is_some());
            assert!(payload.get("moving").is_some());
        }

        // ── Phase 4: Person leaves, entity should eventually expire ──
        for i in 30..50 {
            let img = background(w, h); // no person
            let t = i as f64 / 30.0;
            pipeline.process_frame(img, "cam-0", int.clone(), t);
            tracker.expire(t);
        }

        // After 20 frames (~0.67s) with default 5s timeout, entity still alive
        assert!(tracker.count() > 0, "Entity shouldn't expire after 0.67s");

        // After 6 seconds, entity should be expired
        tracker.expire(40.0);
        assert_eq!(tracker.count(), 0, "Entity should be expired after 6+ seconds");

        // ── Summary ──
        // Full path proven:
        //   video frames → 4 triage nodes → motion events
        //   → entity tracker → persistent tracks with velocity + distance
        //   → grid bridge → typed GridEvents with JSON payloads
        //   → node event queue → ready for IPC to continuum
    }
}
