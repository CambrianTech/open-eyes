//! open-eyes-ffi — C ABI boundary for the open-eyes pipeline.
//!
//! This crate compiles to a cdylib (.so / .dylib) and staticlib (.a).
//! cbindgen generates the C header from these types.
//!
//! RULES:
//! 1. Everything that crosses this boundary is GEOMETRY. No pixel output.
//! 2. The only pixels entering are raw camera frames.
//! 3. All image-space work (filters, flow, edges) stays on GPU as texture IDs.
//! 4. The CPU reads back SPARSE geometric results (points, vectors, planes).
//! 5. The native layer (Swift/Kotlin) calls these directly from camera callbacks.
//!    No managed runtime (Dart/JS) in the hot path.

pub mod types;
pub mod engine;
#[cfg(test)]
mod tests;

use std::os::raw::c_char;
use std::slice;
use types::*;
use engine::OEEngine;

// ── Lifecycle ──────────────────────────────────────────────────────

/// Create a new open-eyes engine with default configuration.
/// Returns an opaque pointer. Caller owns it, must call oe_destroy.
#[no_mangle]
pub extern "C" fn oe_create() -> *mut OEEngine {
    let engine = OEEngine::new(OEConfig::default());
    Box::into_raw(Box::new(engine))
}

/// Create with custom configuration.
#[no_mangle]
pub extern "C" fn oe_create_with_config(config: OEConfig) -> *mut OEEngine {
    let engine = OEEngine::new(config);
    Box::into_raw(Box::new(engine))
}

/// Destroy the engine. Must be called exactly once per oe_create.
///
/// # Safety
/// `engine` must be a valid pointer from oe_create, not yet destroyed.
#[no_mangle]
pub unsafe extern "C" fn oe_destroy(engine: *mut OEEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

// ── Input: Camera Frames ───────────────────────────────────────────

/// Push a raw camera frame into the pipeline.
///
/// Called directly from the native camera callback (AVCaptureOutput / CameraX).
/// The pixel data is read synchronously — caller can reuse the buffer after return.
/// All GPU filter work is dispatched lazily from here.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
/// - `engine` must be valid
/// - `data` must point to `len` readable bytes
#[no_mangle]
pub unsafe extern "C" fn oe_push_frame(
    engine: *mut OEEngine,
    camera_id: u32,
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
    format: OEPixelFormat,
    rotation: i32,
) -> i32 {
    if engine.is_null() || data.is_null() {
        return -1;
    }

    let engine = &mut *engine;
    let frame_data = slice::from_raw_parts(data, len);
    engine.push_frame(camera_id, frame_data, width, height, format, rotation);
    0
}

// ── Input: AR Pose ─────────────────────────────────────────────────

/// Set camera pose from ARKit/ARCore.
/// `transform` is a 4x4 column-major float matrix (16 floats).
///
/// # Safety
/// - `engine` must be valid
/// - `transform` must point to 16 readable f32 values
#[no_mangle]
pub unsafe extern "C" fn oe_set_camera_pose(
    engine: *mut OEEngine,
    camera_id: u32,
    transform: *const f32,
) -> i32 {
    if engine.is_null() || transform.is_null() {
        return -1;
    }

    let engine = &mut *engine;
    let mat: &[f32; 16] = &*(transform as *const [f32; 16]);
    engine.set_camera_pose(camera_id, mat);
    0
}

// ── Output: Scene State (ALL GEOMETRY) ─────────────────────────────

/// Get the current motion magnitude (0.0 = static, higher = more motion).
/// Cheapest possible query — single float, no allocation.
#[no_mangle]
pub unsafe extern "C" fn oe_get_motion(engine: *const OEEngine) -> f32 {
    if engine.is_null() {
        return 0.0;
    }
    (*engine).motion_magnitude()
}

/// Get total frames processed.
#[no_mangle]
pub unsafe extern "C" fn oe_get_frame_count(engine: *const OEEngine) -> u64 {
    if engine.is_null() {
        return 0;
    }
    (*engine).frames_processed()
}

// ── Output: Events ─────────────────────────────────────────────────

/// Poll pending events. Calls `callback` for each event, then drains the buffer.
/// The OEEvent pointer is valid only for the duration of the callback.
///
/// # Safety
/// - `engine` must be valid
/// - `callback` must be a valid function pointer
#[no_mangle]
pub unsafe extern "C" fn oe_poll_events(
    engine: *const OEEngine,
    callback: extern "C" fn(*const OEEvent),
) -> i32 {
    if engine.is_null() {
        return -1;
    }

    let engine = &*engine;
    engine.poll_events(|event| {
        callback(event as *const OEEvent);
    });
    0
}

// ── Configuration ──────────────────────────────────────────────────

/// Set camera intrinsics for a specific camera.
/// Must be called before pushing frames for best results.
///
/// # Safety
/// - `engine` must be valid
#[no_mangle]
pub unsafe extern "C" fn oe_set_intrinsics(
    engine: *mut OEEngine,
    camera_id: u32,
    fx: f64, fy: f64,
    cx: f64, cy: f64,
    width: u32, height: u32,
    k1: f64, k2: f64, k3: f64,
) -> i32 {
    if engine.is_null() {
        return -1;
    }

    let engine = &mut *engine;
    let intrinsics = open_eyes_core::CameraIntrinsics {
        fx, fy, cx, cy, width, height,
        distortion: [k1, k2, k3],
    };
    engine.camera_intrinsics.insert(
        camera_id,
        std::sync::Arc::new(intrinsics),
    );
    0
}
