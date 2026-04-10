//! open-eyes-camera — camera feed acquisition and management
//!
//! Handles connecting to cameras (RTSP, ONVIF, USB, cheap wireless),
//! frame capture, and feeding frames into the core reconstruction pipeline.
//! Camera-agnostic — works with $20 Chinese wireless cameras, Ring
//! (reversed), USB webcams, Raspberry Pi cameras, or any RTSP source.
//!
//! Each camera is a CameraSource that produces CameraFrames. Multiple
//! sources feed into the core's multi-camera fusion pipeline.

pub struct CameraSource {
    pub id: String,
    pub name: String,
    pub url: String,  // rtsp://, http://, usb://0, etc.
    pub intrinsics: open_eyes_core::CameraIntrinsics,
}
