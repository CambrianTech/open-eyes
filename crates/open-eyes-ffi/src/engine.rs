//! The OEEngine — opaque handle that the native layer owns.
//!
//! The native layer (Swift AVFoundation / Kotlin Camera2) calls these functions
//! directly from camera callbacks. No managed runtime in the hot path.
//! CVPixelBuffer → oe_push_frame → Pipeline → geometry out.

use std::sync::{Arc, Mutex};

use open_eyes_core::frame::{Pipeline, PipelineEvent};
use open_eyes_core::fusion::FusionEngine;
use open_eyes_core::stitch::StitchEngine;
use open_eyes_core::CameraIntrinsics;

use crate::types::*;

/// Internal engine state. Opaque to callers — they get a raw pointer.
pub struct OEEngine {
    pub(crate) config: OEConfig,
    pipeline: Pipeline,
    fusion: FusionEngine,
    stitch: StitchEngine,

    /// Per-camera intrinsics cache
    pub(crate) camera_intrinsics: std::collections::HashMap<u32, Arc<CameraIntrinsics>>,

    /// Ring buffer of recent events
    events: Mutex<Vec<OEEvent>>,

    /// Frame counter
    frames_processed: u64,

    /// Last motion magnitude from optical flow
    motion_magnitude: f32,
}

impl OEEngine {
    pub fn new(config: OEConfig) -> Self {
        Self {
            config,
            pipeline: Pipeline::new(),
            fusion: FusionEngine::new(),
            stitch: StitchEngine::new(),
            camera_intrinsics: std::collections::HashMap::new(),
            events: Mutex::new(Vec::with_capacity(256)),
            frames_processed: 0,
            motion_magnitude: 0.0,
        }
    }

    /// Push a raw camera frame into the pipeline.
    /// Called from native camera callback — must be FAST.
    pub fn push_frame(
        &mut self,
        camera_id: u32,
        data: &[u8],
        width: u32,
        height: u32,
        format: OEPixelFormat,
        _rotation: i32,
    ) {
        // Convert raw bytes to RGB — the ONE place pixels enter the system.
        let rgb_image = match format {
            OEPixelFormat::Bgra8 => bgra_to_rgb(data, width, height),
            OEPixelFormat::Rgb8 => {
                image::RgbImage::from_raw(width, height, data.to_vec())
                    .unwrap_or_else(|| image::RgbImage::new(width, height))
            }
            OEPixelFormat::Yuv420 => yuv420_to_rgb(data, width, height),
            OEPixelFormat::Gray8 => gray_to_rgb(data, width, height),
        };

        let intrinsics = self.camera_intrinsics
            .entry(camera_id)
            .or_insert_with(|| Arc::new(CameraIntrinsics {
                fx: width as f64 * 0.8,
                fy: height as f64 * 0.8,
                cx: width as f64 / 2.0,
                cy: height as f64 / 2.0,
                width,
                height,
                distortion: [0.0; 3],
            }))
            .clone();

        let cam_id_str = camera_id.to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Run through pipeline — all registered ProcessNodes fire
        let events = self.pipeline.process_frame(
            rgb_image,
            &cam_id_str,
            intrinsics,
            timestamp,
        );

        // Convert pipeline events to FFI events
        if let Ok(mut event_buf) = self.events.lock() {
            for event in &events {
                let oe_event = match event {
                    PipelineEvent::Motion { magnitude, .. } => OEEvent {
                        event_type: OEEventType::Motion,
                        camera_id,
                        entity_id: 0,
                        value: *magnitude as f32,
                        position: [0.0; 3],
                        timestamp,
                    },
                    PipelineEvent::CameraDrift { drift_pixels, .. } => OEEvent {
                        event_type: OEEventType::CameraDrift,
                        camera_id,
                        entity_id: 0,
                        value: *drift_pixels as f32,
                        position: [0.0; 3],
                        timestamp,
                    },
                    _ => continue,
                };
                if event_buf.len() >= 256 {
                    event_buf.remove(0);
                }
                event_buf.push(oe_event);
            }
        }

        self.frames_processed += 1;
    }

    /// Set camera pose from ARKit/ARCore (4x4 column-major transform).
    pub fn set_camera_pose(&mut self, camera_id: u32, transform: &[f32; 16]) {
        let pose = nalgebra::Matrix4::from_column_slice(
            &transform.map(|v| v as f64)
        );

        let cam_id = camera_id.to_string();
        if !self.fusion.cameras().contains_key(&cam_id) {
            let intrinsics = self.camera_intrinsics
                .get(&camera_id)
                .map(|i| (**i).clone())
                .unwrap_or(CameraIntrinsics {
                    fx: 1000.0, fy: 1000.0,
                    cx: 540.0, cy: 960.0,
                    width: 1080, height: 1920,
                    distortion: [0.0; 3],
                });
            self.fusion.register_camera(&cam_id, intrinsics, Some(pose));
        }
    }

    pub fn motion_magnitude(&self) -> f32 {
        self.motion_magnitude
    }

    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Drain pending events into a caller-provided callback.
    pub fn poll_events<F: FnMut(&OEEvent)>(&self, mut callback: F) {
        if let Ok(mut events) = self.events.lock() {
            for event in events.drain(..) {
                callback(&event);
            }
        }
    }
}

// --- Pixel conversion (the ONLY place we touch pixels) ---

fn bgra_to_rgb(data: &[u8], width: u32, height: u32) -> image::RgbImage {
    let pixel_count = (width * height) as usize;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for i in 0..pixel_count {
        let base = i * 4;
        if base + 2 < data.len() {
            rgb.push(data[base + 2]); // R
            rgb.push(data[base + 1]); // G
            rgb.push(data[base]);     // B
        }
    }
    image::RgbImage::from_raw(width, height, rgb)
        .unwrap_or_else(|| image::RgbImage::new(width, height))
}

fn yuv420_to_rgb(data: &[u8], width: u32, height: u32) -> image::RgbImage {
    let w = width as usize;
    let h = height as usize;
    let y_size = w * h;
    let mut rgb = Vec::with_capacity(y_size * 3);

    for row in 0..h {
        for col in 0..w {
            let y = data[row * w + col] as f32;
            let uv_row = row / 2;
            let uv_col = col / 2;
            let uv_idx = y_size + uv_row * w + (uv_col * 2);

            let (u, v) = if uv_idx + 1 < data.len() {
                (data[uv_idx] as f32 - 128.0, data[uv_idx + 1] as f32 - 128.0)
            } else {
                (0.0, 0.0)
            };

            rgb.push((y + 1.370705 * v).clamp(0.0, 255.0) as u8);
            rgb.push((y - 0.337633 * u - 0.698001 * v).clamp(0.0, 255.0) as u8);
            rgb.push((y + 1.732446 * u).clamp(0.0, 255.0) as u8);
        }
    }

    image::RgbImage::from_raw(width, height, rgb)
        .unwrap_or_else(|| image::RgbImage::new(width, height))
}

fn gray_to_rgb(data: &[u8], width: u32, height: u32) -> image::RgbImage {
    let mut rgb = Vec::with_capacity(data.len() * 3);
    for &g in data.iter().take((width * height) as usize) {
        rgb.push(g);
        rgb.push(g);
        rgb.push(g);
    }
    image::RgbImage::from_raw(width, height, rgb)
        .unwrap_or_else(|| image::RgbImage::new(width, height))
}
