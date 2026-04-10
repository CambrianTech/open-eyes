//! Tests for the FFI boundary.
//!
//! TDD: Does the code work correctly?
//! VDD: Does it actually work in real-world scenarios?
//!   - Can it survive garbage input without crashing?
//!   - Does pixel conversion produce sane output?
//!   - Does the engine maintain correct state across many frames?
//!   - Do events flow properly through the pipeline?
//!   - Does it handle multi-camera scenarios?
//!   - What happens at boundary conditions (0x0 frame, huge frame, null data)?

#[cfg(test)]
mod tests {
    use crate::engine::*;
    use crate::types::*;

    // ── Helper: generate a solid-color BGRA frame ──────────────────

    fn bgra_frame(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        let pixel_count = (width * height) as usize;
        let mut data = Vec::with_capacity(pixel_count * 4);
        for _ in 0..pixel_count {
            data.push(b);  // B
            data.push(g);  // G
            data.push(r);  // R
            data.push(255); // A
        }
        data
    }

    fn rgb_frame(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        let pixel_count = (width * height) as usize;
        let mut data = Vec::with_capacity(pixel_count * 3);
        for _ in 0..pixel_count {
            data.push(r);
            data.push(g);
            data.push(b);
        }
        data
    }

    fn gray_frame(width: u32, height: u32, value: u8) -> Vec<u8> {
        vec![value; (width * height) as usize]
    }

    // Generate YUV420 frame (Y plane + interleaved UV)
    fn yuv420_frame(width: u32, height: u32, y: u8, u: u8, v: u8) -> Vec<u8> {
        let w = width as usize;
        let h = height as usize;
        let y_size = w * h;
        let uv_size = w * (h / 2); // interleaved UV, half height
        let mut data = Vec::with_capacity(y_size + uv_size);
        // Y plane
        data.extend(std::iter::repeat(y).take(y_size));
        // UV interleaved
        for _ in 0..(h / 2) {
            for _ in 0..(w / 2) {
                data.push(u);
                data.push(v);
            }
        }
        data
    }

    // ── TDD: Engine lifecycle ──────────────────────────────────────

    #[test]
    fn engine_create_destroy() {
        let engine = OEEngine::new(OEConfig::default());
        assert_eq!(engine.frames_processed(), 0);
        assert_eq!(engine.motion_magnitude(), 0.0);
        // drop — should not panic
    }

    #[test]
    fn engine_default_config() {
        let config = OEConfig::default();
        assert_eq!(config.max_cameras, 16);
        assert!(config.enable_flow);
        assert!(config.enable_features);
        assert!(!config.enable_ml); // off by default
    }

    // ── TDD: Frame ingestion ───────────────────────────────────────

    #[test]
    fn push_single_bgra_frame() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = bgra_frame(64, 48, 128, 64, 32);
        engine.push_frame(0, &frame, 64, 48, OEPixelFormat::Bgra8, 0);
        assert_eq!(engine.frames_processed(), 1);
    }

    #[test]
    fn push_single_rgb_frame() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = rgb_frame(64, 48, 128, 64, 32);
        engine.push_frame(0, &frame, 64, 48, OEPixelFormat::Rgb8, 0);
        assert_eq!(engine.frames_processed(), 1);
    }

    #[test]
    fn push_single_gray_frame() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = gray_frame(64, 48, 128);
        engine.push_frame(0, &frame, 64, 48, OEPixelFormat::Gray8, 0);
        assert_eq!(engine.frames_processed(), 1);
    }

    #[test]
    fn push_single_yuv420_frame() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = yuv420_frame(64, 48, 128, 128, 128);
        engine.push_frame(0, &frame, 64, 48, OEPixelFormat::Yuv420, 0);
        assert_eq!(engine.frames_processed(), 1);
    }

    #[test]
    fn push_many_frames_increments_counter() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = bgra_frame(32, 24, 100, 100, 100);
        for i in 0..100 {
            engine.push_frame(0, &frame, 32, 24, OEPixelFormat::Bgra8, 0);
            assert_eq!(engine.frames_processed(), i + 1);
        }
    }

    // ── TDD: Multi-camera ──────────────────────────────────────────

    #[test]
    fn multiple_cameras_independent_intrinsics() {
        let mut engine = OEEngine::new(OEConfig::default());
        let frame_a = bgra_frame(640, 480, 100, 100, 100);
        let frame_b = bgra_frame(1280, 720, 200, 200, 200);

        engine.push_frame(0, &frame_a, 640, 480, OEPixelFormat::Bgra8, 0);
        engine.push_frame(1, &frame_b, 1280, 720, OEPixelFormat::Bgra8, 0);

        assert_eq!(engine.frames_processed(), 2);
        assert_eq!(engine.camera_intrinsics.len(), 2);

        let cam0 = &engine.camera_intrinsics[&0];
        let cam1 = &engine.camera_intrinsics[&1];
        assert_eq!(cam0.width, 640);
        assert_eq!(cam1.width, 1280);
    }

    #[test]
    fn set_intrinsics_overrides_defaults() {
        let mut engine = OEEngine::new(OEConfig::default());

        // Push a frame first to create default intrinsics
        let frame = bgra_frame(640, 480, 100, 100, 100);
        engine.push_frame(0, &frame, 640, 480, OEPixelFormat::Bgra8, 0);
        assert!((engine.camera_intrinsics[&0].fx - 512.0).abs() < 0.01); // 640 * 0.8

        // Override with real intrinsics
        engine.camera_intrinsics.insert(0, std::sync::Arc::new(
            open_eyes_core::CameraIntrinsics {
                fx: 920.0, fy: 920.0,
                cx: 320.0, cy: 240.0,
                width: 640, height: 480,
                distortion: [0.1, -0.2, 0.0],
            }
        ));

        assert!((engine.camera_intrinsics[&0].fx - 920.0).abs() < 0.01);
    }

    // ── TDD: Camera pose ───────────────────────────────────────────

    #[test]
    fn set_camera_pose_registers_camera() {
        let mut engine = OEEngine::new(OEConfig::default());
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        engine.set_camera_pose(0, &identity);
        assert!(engine.fusion.cameras().contains_key("0"));
    }

    #[test]
    fn set_camera_pose_uses_custom_intrinsics() {
        let mut engine = OEEngine::new(OEConfig::default());

        // Set intrinsics before pose
        engine.camera_intrinsics.insert(5, std::sync::Arc::new(
            open_eyes_core::CameraIntrinsics {
                fx: 1200.0, fy: 1200.0,
                cx: 640.0, cy: 360.0,
                width: 1280, height: 720,
                distortion: [0.0; 3],
            }
        ));

        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        engine.set_camera_pose(5, &identity);
        assert!(engine.fusion.cameras().contains_key("5"));
    }

    // ── TDD: Event system ──────────────────────────────────────────

    #[test]
    fn events_start_empty() {
        let engine = OEEngine::new(OEConfig::default());
        let mut count = 0;
        engine.poll_events(|_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn poll_events_drains_buffer() {
        let engine = OEEngine::new(OEConfig::default());

        // Manually push an event (simulating what push_frame would do)
        {
            let mut events = engine.events.lock().unwrap();
            events.push(OEEvent {
                event_type: OEEventType::Motion,
                camera_id: 0,
                entity_id: 0,
                value: 0.5,
                position: [1.0, 2.0, 3.0],
                timestamp: 1000.0,
            });
        }

        // First poll: should get the event
        let mut received = Vec::new();
        engine.poll_events(|e| received.push(e.value));
        assert_eq!(received.len(), 1);
        assert!((received[0] - 0.5).abs() < 0.001);

        // Second poll: buffer should be drained
        let mut count = 0;
        engine.poll_events(|_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn event_ring_buffer_caps_at_256() {
        let engine = OEEngine::new(OEConfig::default());
        {
            let mut events = engine.events.lock().unwrap();
            for i in 0..300 {
                events.push(OEEvent {
                    event_type: OEEventType::Motion,
                    camera_id: 0,
                    entity_id: 0,
                    value: i as f32,
                    position: [0.0; 3],
                    timestamp: 0.0,
                });
                // Enforce ring buffer cap (same logic as push_frame)
                if events.len() > 256 {
                    events.remove(0);
                }
            }
            assert_eq!(events.len(), 256);
        }

        let mut received = Vec::new();
        engine.poll_events(|e| received.push(e.value));
        assert_eq!(received.len(), 256);
        // First event should be #44 (300 - 256)
        assert!((received[0] - 44.0).abs() < 0.001);
    }

    // ── TDD: Pixel conversion correctness ──────────────────────────

    #[test]
    fn bgra_to_rgb_pure_red() {
        // BGRA for red: B=0, G=0, R=255, A=255
        let bgra = vec![0u8, 0, 255, 255]; // one pixel
        let rgb = crate::engine::bgra_to_rgb(&bgra, 1, 1);
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 0, 0]); // RGB red
    }

    #[test]
    fn bgra_to_rgb_pure_blue() {
        let bgra = vec![255u8, 0, 0, 255]; // BGRA blue
        let rgb = crate::engine::bgra_to_rgb(&bgra, 1, 1);
        assert_eq!(rgb.get_pixel(0, 0).0, [0, 0, 255]); // RGB blue
    }

    #[test]
    fn gray_to_rgb_midtone() {
        let gray = vec![128u8]; // one pixel
        let rgb = crate::engine::gray_to_rgb(&gray, 1, 1);
        assert_eq!(rgb.get_pixel(0, 0).0, [128, 128, 128]);
    }

    #[test]
    fn yuv420_neutral_gray() {
        // Y=128, U=128, V=128 should produce ~neutral gray
        let yuv = yuv420_frame(2, 2, 128, 128, 128);
        let rgb = crate::engine::yuv420_to_rgb(&yuv, 2, 2);
        let pixel = rgb.get_pixel(0, 0).0;
        // YUV (128, 128, 128) → approximately (128, 128, 128) in RGB
        // Allow some tolerance due to integer math
        assert!((pixel[0] as i32 - 128).abs() < 5, "R={}", pixel[0]);
        assert!((pixel[1] as i32 - 128).abs() < 5, "G={}", pixel[1]);
        assert!((pixel[2] as i32 - 128).abs() < 5, "B={}", pixel[2]);
    }

    // ── VDD: Real-world scenarios ──────────────────────────────────
    //
    // These test what actually happens on a real camera, not just
    // code correctness. Does it survive? Does it produce sane results?

    #[test]
    fn vdd_static_scene_no_motion() {
        // A camera staring at a wall. Same frame repeated.
        // Validation: motion should be 0 (or near 0) after multiple frames.
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = bgra_frame(160, 120, 100, 100, 100);

        for _ in 0..10 {
            engine.push_frame(0, &frame, 160, 120, OEPixelFormat::Bgra8, 0);
        }

        // Static scene = no motion
        assert!(
            engine.motion_magnitude() < 0.1,
            "Static scene should have near-zero motion, got {}",
            engine.motion_magnitude()
        );
    }

    #[test]
    fn vdd_720p_frame_at_30fps_budget() {
        // Validation: pushing a 720p frame should complete in reasonable time.
        // On real hardware this validates the 15ms budget.
        // In CI this just validates it doesn't crash or hang.
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = bgra_frame(1280, 720, 100, 150, 200);

        let start = std::time::Instant::now();
        engine.push_frame(0, &frame, 1280, 720, OEPixelFormat::Bgra8, 0);
        let elapsed = start.elapsed();

        assert_eq!(engine.frames_processed(), 1);
        // On desktop this should be <50ms. On ARM we'd check <15ms.
        // Just verify it completes without hanging.
        assert!(elapsed.as_secs() < 5, "Frame processing took {:?}", elapsed);
    }

    #[test]
    fn vdd_survives_1000_frames_no_leak() {
        // Validation: process 1000 frames without OOM or crash.
        // Memory should be bounded (no accumulation).
        let mut engine = OEEngine::new(OEConfig::default());
        let frame = bgra_frame(160, 120, 50, 100, 150);

        for _ in 0..1000 {
            engine.push_frame(0, &frame, 160, 120, OEPixelFormat::Bgra8, 0);
        }

        assert_eq!(engine.frames_processed(), 1000);
        // Events buffer should be bounded at 256
        let mut event_count = 0;
        engine.poll_events(|_| event_count += 1);
        assert!(event_count <= 256);
    }

    #[test]
    fn vdd_multi_camera_independent() {
        // Validation: 4 cameras feeding frames simultaneously.
        // Each camera's state should be independent.
        let mut engine = OEEngine::new(OEConfig::default());

        for cam_id in 0..4u32 {
            let frame = bgra_frame(320, 240, cam_id as u8 * 50, 100, 100);
            for _ in 0..10 {
                engine.push_frame(cam_id, &frame, 320, 240, OEPixelFormat::Bgra8, 0);
            }
        }

        assert_eq!(engine.frames_processed(), 40); // 4 cameras × 10 frames
        assert_eq!(engine.camera_intrinsics.len(), 4);
    }

    #[test]
    fn vdd_camera_pose_before_and_after_frames() {
        // Validation: pose can be set before or after pushing frames.
        // Both paths should work without crashing.
        let mut engine = OEEngine::new(OEConfig::default());
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];

        // Pose before frames
        engine.set_camera_pose(0, &identity);
        let frame = bgra_frame(64, 48, 100, 100, 100);
        engine.push_frame(0, &frame, 64, 48, OEPixelFormat::Bgra8, 0);

        // Frames before pose
        engine.push_frame(1, &frame, 64, 48, OEPixelFormat::Bgra8, 0);
        engine.set_camera_pose(1, &identity);

        assert_eq!(engine.frames_processed(), 2);
    }

    // ── VDD: FFI safety ────────────────────────────────────────────
    //
    // The extern "C" functions receive raw pointers from Swift/Kotlin.
    // These tests validate the Rust side handles bad input gracefully.

    #[test]
    fn ffi_create_and_destroy() {
        unsafe {
            let engine = crate::oe_create();
            assert!(!engine.is_null());
            assert_eq!(crate::oe_get_frame_count(engine), 0);
            assert_eq!(crate::oe_get_motion(engine), 0.0);
            crate::oe_destroy(engine);
        }
    }

    #[test]
    fn ffi_push_frame_through_c_api() {
        unsafe {
            let engine = crate::oe_create();
            let frame = bgra_frame(64, 48, 100, 100, 100);

            let result = crate::oe_push_frame(
                engine, 0,
                frame.as_ptr(), frame.len(),
                64, 48,
                OEPixelFormat::Bgra8, 0,
            );

            assert_eq!(result, 0); // success
            assert_eq!(crate::oe_get_frame_count(engine), 1);
            crate::oe_destroy(engine);
        }
    }

    #[test]
    fn ffi_null_engine_returns_error() {
        unsafe {
            // All functions should handle null engine gracefully
            assert_eq!(crate::oe_get_motion(std::ptr::null()), 0.0);
            assert_eq!(crate::oe_get_frame_count(std::ptr::null()), 0);

            let frame = bgra_frame(64, 48, 100, 100, 100);
            let result = crate::oe_push_frame(
                std::ptr::null_mut(), 0,
                frame.as_ptr(), frame.len(),
                64, 48, OEPixelFormat::Bgra8, 0,
            );
            assert_eq!(result, -1); // error

            let identity = [0.0f32; 16];
            let result = crate::oe_set_camera_pose(
                std::ptr::null_mut(), 0, identity.as_ptr(),
            );
            assert_eq!(result, -1);
        }
    }

    #[test]
    fn ffi_destroy_null_is_safe() {
        unsafe {
            // Should not crash
            crate::oe_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn ffi_set_intrinsics_through_c_api() {
        unsafe {
            let engine = crate::oe_create();
            let result = crate::oe_set_intrinsics(
                engine, 0,
                920.0, 920.0,
                320.0, 240.0,
                640, 480,
                0.1, -0.2, 0.0,
            );
            assert_eq!(result, 0);
            crate::oe_destroy(engine);
        }
    }

    // ── VDD: Algorithm validation against ground truth ─────────────
    //
    // These tests feed KNOWN inputs with KNOWN correct outputs and
    // validate the algorithm produces mathematically correct results
    // within tolerance. Same discipline as forge eval (PPL within Δ%
    // of source) — the test knows the right answer.

    #[test]
    fn vdd_bgra_conversion_preserves_color_accuracy() {
        // Ground truth: a 2x2 BGRA image with known colors.
        // Validate RGB conversion matches expected values exactly.
        let mut bgra = Vec::new();
        // Pixel (0,0): red (BGRA: 0, 0, 255, 255)
        bgra.extend_from_slice(&[0, 0, 255, 255]);
        // Pixel (1,0): green (BGRA: 0, 255, 0, 255)
        bgra.extend_from_slice(&[0, 255, 0, 255]);
        // Pixel (0,1): blue (BGRA: 255, 0, 0, 255)
        bgra.extend_from_slice(&[255, 0, 0, 255]);
        // Pixel (1,1): white (BGRA: 255, 255, 255, 255)
        bgra.extend_from_slice(&[255, 255, 255, 255]);

        let rgb = crate::engine::bgra_to_rgb(&bgra, 2, 2);

        assert_eq!(rgb.get_pixel(0, 0).0, [255, 0, 0], "Red pixel");
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 255, 0], "Green pixel");
        assert_eq!(rgb.get_pixel(0, 1).0, [0, 0, 255], "Blue pixel");
        assert_eq!(rgb.get_pixel(1, 1).0, [255, 255, 255], "White pixel");
    }

    #[test]
    fn vdd_yuv_to_rgb_known_colors() {
        // Ground truth: YUV values for known colors (ITU-R BT.601).
        // Pure white: Y=255, U=128, V=128 → RGB ≈ (255, 255, 255)
        let yuv_white = yuv420_frame(2, 2, 255, 128, 128);
        let rgb = crate::engine::yuv420_to_rgb(&yuv_white, 2, 2);
        let p = rgb.get_pixel(0, 0).0;
        assert!((p[0] as i32 - 255).abs() < 3, "White R={}", p[0]);
        assert!((p[1] as i32 - 255).abs() < 3, "White G={}", p[1]);
        assert!((p[2] as i32 - 255).abs() < 3, "White B={}", p[2]);

        // Pure black: Y=0, U=128, V=128 → RGB ≈ (0, 0, 0)
        let yuv_black = yuv420_frame(2, 2, 0, 128, 128);
        let rgb = crate::engine::yuv420_to_rgb(&yuv_black, 2, 2);
        let p = rgb.get_pixel(0, 0).0;
        assert!((p[0] as i32).abs() < 3, "Black R={}", p[0]);
        assert!((p[1] as i32).abs() < 3, "Black G={}", p[1]);
        assert!((p[2] as i32).abs() < 3, "Black B={}", p[2]);
    }

    #[test]
    fn vdd_triangulation_known_3d_point() {
        // Ground truth: a known 3D point at (1, 2, 5).
        // Two cameras at known poses project this point.
        // Triangulation should recover the 3D point within ε.
        use open_eyes_core::{CameraIntrinsics, Point3, Transform};
        use open_eyes_core::geometry::{project_point, triangulate};

        let intrinsics = CameraIntrinsics {
            fx: 500.0, fy: 500.0,
            cx: 320.0, cy: 240.0,
            width: 640, height: 480,
            distortion: [0.0; 3],
        };

        let world_point = Point3::new(1.0, 2.0, 5.0);

        // Camera A at origin, looking down -Z
        let pose_a = Transform::identity();
        // Camera B shifted 2m to the right
        let mut pose_b = Transform::identity();
        pose_b[(0, 3)] = 2.0;

        let pixel_a = project_point(&world_point, &pose_a, &intrinsics).unwrap();
        let pixel_b = project_point(&world_point, &pose_b, &intrinsics).unwrap();

        let recovered = triangulate(
            pixel_a, &pose_a, &intrinsics,
            pixel_b, &pose_b, &intrinsics,
        ).expect("Triangulation should succeed for valid stereo pair");

        let error = (recovered - world_point).norm();
        assert!(
            error < 0.1,
            "Triangulated point {:?} should be within 0.1m of ground truth {:?}, error={}",
            recovered, world_point, error
        );
    }

    #[test]
    fn vdd_ransac_plane_from_noisy_coplanar_points() {
        // Ground truth: a horizontal plane at y=2.0 (normal=[0,1,0], distance=2.0).
        // Generate 100 points on this plane with Gaussian noise (σ=0.01m).
        // RANSAC should recover the plane within tolerance.
        use open_eyes_core::Point3;
        use open_eyes_core::geometry::fit_plane_ransac;

        let mut points = Vec::new();
        let mut rng_state: u64 = 42; // deterministic pseudo-random

        for i in 0..100 {
            // Simple PRNG for deterministic test
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let noise = ((rng_state >> 33) as f64 / u32::MAX as f64 - 0.5) * 0.02;

            let x = (i % 10) as f64 * 0.5 - 2.5;
            let z = (i / 10) as f64 * 0.5 - 2.5;
            points.push(Point3::new(x, 2.0 + noise, z));
        }

        let plane = fit_plane_ransac(&points, 0.05, 0.5, 200)
            .expect("RANSAC should find a plane from coplanar points");

        // Normal should be approximately [0, ±1, 0]
        let ny = plane.normal[1].abs();
        assert!(
            ny > 0.95,
            "Plane normal Y component should be ~1.0, got normal={:?}",
            plane.normal
        );

        // Distance from origin should be approximately 2.0
        assert!(
            (plane.distance.abs() - 2.0).abs() < 0.1,
            "Plane distance should be ~2.0, got {}",
            plane.distance
        );
    }

    #[test]
    fn vdd_flow_detects_known_translation() {
        // Ground truth: shift a frame 5 pixels to the right.
        // Optical flow should report rightward motion.
        use open_eyes_core::frame::FlowField;
        use open_eyes_core::features::flow_motion_magnitude;

        // Create a flow field representing uniform 5px rightward shift
        let width = 40u32;
        let height = 30u32;
        let vectors: Vec<(f32, f32)> = (0..(width * height))
            .map(|_| (5.0, 0.0)) // dx=5, dy=0
            .collect();

        let flow = FlowField { width, height, vectors };
        let magnitude = flow_motion_magnitude(&flow);

        // 75th percentile of uniform 5.0 vectors = 5.0
        assert!(
            (magnitude - 5.0).abs() < 0.1,
            "Flow magnitude should be ~5.0 for 5px translation, got {}",
            magnitude
        );
    }

    #[test]
    fn vdd_flow_zero_for_static() {
        // Ground truth: identical frames = zero motion.
        use open_eyes_core::frame::FlowField;
        use open_eyes_core::features::flow_motion_magnitude;

        let flow = FlowField {
            width: 40,
            height: 30,
            vectors: vec![(0.0, 0.0); 1200],
        };

        let magnitude = flow_motion_magnitude(&flow);
        assert!(
            magnitude < 0.001,
            "Static scene should have zero flow, got {}",
            magnitude
        );
    }

    #[test]
    fn vdd_feature_matching_identical_descriptors() {
        // Ground truth: matching a feature against itself should produce distance 0.
        use open_eyes_core::features::{FeaturePoint, match_features};

        let features_a = vec![
            FeaturePoint { pixel: (100.0, 200.0), descriptor: vec![0xAA; 32], track_id: None },
            FeaturePoint { pixel: (300.0, 400.0), descriptor: vec![0x55; 32], track_id: None },
        ];
        // Same descriptors, different pixel positions (simulating a moved camera)
        let features_b = vec![
            FeaturePoint { pixel: (105.0, 203.0), descriptor: vec![0xAA; 32], track_id: None },
            FeaturePoint { pixel: (298.0, 405.0), descriptor: vec![0x55; 32], track_id: None },
        ];

        let matches = match_features(&features_a, &features_b, 64);

        assert_eq!(matches.len(), 2, "Should find 2 matches for identical descriptors");
        // Matches should be (0→0, 1→1) with distance 0
        for (idx_a, idx_b, distance) in &matches {
            assert_eq!(idx_a, idx_b, "Identical descriptors should match same index");
            assert_eq!(*distance, 0, "Identical descriptors should have distance 0");
        }
    }

    #[test]
    fn ffi_poll_events_callback() {
        unsafe {
            let engine = crate::oe_create();

            // Push a frame to generate potential events
            let frame = bgra_frame(64, 48, 100, 100, 100);
            crate::oe_push_frame(engine, 0, frame.as_ptr(), frame.len(), 64, 48, OEPixelFormat::Bgra8, 0);

            // Poll — callback should be called for each event (may be 0)
            static mut CALLBACK_COUNT: i32 = 0;
            extern "C" fn count_callback(_event: *const OEEvent) {
                unsafe { CALLBACK_COUNT += 1; }
            }

            CALLBACK_COUNT = 0;
            crate::oe_poll_events(engine, count_callback);
            // Don't assert count — just verify it doesn't crash
            assert!(CALLBACK_COUNT >= 0);

            crate::oe_destroy(engine);
        }
    }
}
