//! open-eyes-camera — camera feed acquisition and management
//!
//! Camera-agnostic feed acquisition. Every camera is a `CameraSource`
//! that produces frames. The pipeline doesn't care whether the source
//! is a $15 Virtavo egg on Wi-Fi or a Vision Pro with LiDAR.
//!
//! Two operating modes:
//!
//! **Mode 1: Raw RTSP client (no flash required)**
//! Connect to any camera's RTSP stream, decode H.264/H.265 frames,
//! feed into the CBAR pipeline. Works with stock firmware. Day one.
//!
//! **Mode 2: open-eyes agent ON the camera (OpenIPC flashed)**
//! The camera runs our Rust binary. It captures frames locally, runs
//! tier-1 optical flow, emits motion events, and streams RTSP to
//! the grid node. The grid node receives pre-filtered frames with
//! motion metadata — less bandwidth, smarter processing.
//!
//! The CameraSource trait unifies both modes. The pipeline doesn't
//! know or care which mode a camera is running.

pub mod source;
pub mod video;
pub mod rtsp;
pub mod discovery;
pub mod agent;

use open_eyes_core::CameraIntrinsics;

/// Configuration for connecting to a camera.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraConfig {
    /// Unique identifier for this camera
    pub id: String,
    /// Human-readable name ("Front door", "Backyard north", etc.)
    pub name: String,
    /// Connection URL
    ///   rtsp://192.168.1.100:554/stream1  — RTSP (most common)
    ///   rtsp://192.168.1.100:554/ch0      — some Chinese cameras
    ///   http://192.168.1.100/mjpeg        — MJPEG over HTTP
    ///   v4l2:///dev/video0                 — USB camera (Linux)
    ///   openipc://192.168.1.100           — open-eyes agent mode
    pub url: String,
    /// Camera intrinsics (if known). If None, auto-calibrate from
    /// stream resolution using a generic wide-angle model.
    pub intrinsics: Option<CameraIntrinsics>,
    /// Expected resolution (for pre-allocation). Auto-detected from stream if None.
    pub resolution: Option<(u32, u32)>,
    /// Target FPS. Some cameras support multiple streams at different rates.
    pub target_fps: Option<u32>,
    /// Whether this camera is running the open-eyes agent (Mode 2).
    /// When true, the source expects motion events alongside the RTSP stream.
    pub has_agent: bool,
    /// Static mount location (for cross-camera calibration).
    /// If None, calibration is done via feature matching.
    pub mount_position: Option<[f64; 3]>,
    /// Static mount orientation as quaternion (w, x, y, z). NEVER Euler angles.
    pub mount_orientation: Option<[f64; 4]>,
}

/// Runtime state of a connected camera.
#[derive(Debug, Clone)]
pub struct CameraStatus {
    pub id: String,
    pub connected: bool,
    pub resolution: Option<(u32, u32)>,
    pub fps: f32,
    pub frames_received: u64,
    pub frames_dropped: u64,
    pub last_frame_time: f64,
    pub has_agent: bool,
    pub agent_version: Option<String>,
    pub signal_strength_dbm: Option<i32>,  // Wi-Fi signal if available
}
