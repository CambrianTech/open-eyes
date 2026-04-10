//! source — the CameraSource trait and implementations
//!
//! Every camera implements CameraSource. The pipeline calls
//! next_frame() and gets back a Frame. Whether that frame came
//! from an RTSP stream, a USB device, or an on-camera agent
//! is invisible to the consumer.

use open_eyes_core::frame::Frame;
use open_eyes_core::CameraIntrinsics;
use std::sync::Arc;

/// A source of camera frames.
///
/// Implementations handle connection, reconnection, frame decoding,
/// and intrinsics management. The pipeline calls next_frame() in
/// a loop — blocking until a frame is available or the source
/// disconnects.
pub trait CameraSource: Send {
    /// Unique camera ID
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Camera intrinsics (may be updated after first frame if auto-detected)
    fn intrinsics(&self) -> Arc<CameraIntrinsics>;

    /// Block until the next frame is available.
    /// Returns None if the source disconnected.
    fn next_frame(&mut self) -> Option<Frame>;

    /// Whether the source is currently connected.
    fn connected(&self) -> bool;

    /// Attempt to reconnect after a disconnect.
    /// Returns true if reconnection succeeded.
    fn reconnect(&mut self) -> bool;

    /// Whether this source has an on-camera open-eyes agent.
    fn has_agent(&self) -> bool { false }
}
