//! rtsp — RTSP camera source with authentication and reconnection.
//!
//! Wraps VideoSource (which uses OpenCV VideoCapture for RTSP) with:
//! - Digest/basic authentication
//! - Exponential backoff on disconnect
//! - Stream health monitoring (frame rate tracking)
//! - Common credential probing for unconfigured cameras

use crate::source::CameraSource;
use crate::video::VideoSource;
use open_eyes_core::frame::Frame;
use open_eyes_core::CameraIntrinsics;
use std::sync::Arc;

/// RTSP camera source with auth and reconnection.
pub struct RtspSource {
    inner: Option<VideoSource>,
    id: String,
    url: String,
    credentials: Option<RtspCredentials>,
    reconnect_delay_ms: u64,
    max_reconnect_delay_ms: u64,
    frames_since_connect: u64,
    last_frame_time: std::time::Instant,
    stall_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RtspCredentials {
    pub username: String,
    pub password: String,
}

impl RtspSource {
    /// Connect to an RTSP camera.
    pub fn connect(id: &str, url: &str, credentials: Option<RtspCredentials>) -> Result<Self, String> {
        let authed_url = match &credentials {
            Some(creds) => inject_credentials(url, &creds.username, &creds.password),
            None => url.to_string(),
        };

        let inner = VideoSource::open(id, &authed_url)?;

        Ok(Self {
            inner: Some(inner),
            id: id.to_string(),
            url: url.to_string(),
            credentials,
            reconnect_delay_ms: 1000,
            max_reconnect_delay_ms: 30000,
            frames_since_connect: 0,
            last_frame_time: std::time::Instant::now(),
            stall_timeout_ms: 5000,
        })
    }

    /// Try connecting with common default credentials.
    /// Many cheap cameras ship with admin/admin or admin/(empty).
    pub fn connect_with_probe(id: &str, url: &str) -> Result<Self, String> {
        // Try no auth first
        if let Ok(source) = Self::connect(id, url, None) {
            return Ok(source);
        }

        // Common defaults for cheap cameras
        let defaults = [
            ("admin", "admin"),
            ("admin", ""),
            ("admin", "12345"),
            ("root", "root"),
            ("root", ""),
        ];

        for (user, pass) in &defaults {
            let creds = RtspCredentials {
                username: user.to_string(),
                password: pass.to_string(),
            };
            if let Ok(source) = Self::connect(id, url, Some(creds)) {
                tracing::info!("Camera {}: connected with credentials {}/*", id, user);
                return Ok(source);
            }
        }

        Err(format!("Could not connect to {} with any known credentials", url))
    }

    /// Check if the stream appears stalled (no frames for too long).
    pub fn is_stalled(&self) -> bool {
        self.last_frame_time.elapsed().as_millis() as u64 > self.stall_timeout_ms
    }
}

impl CameraSource for RtspSource {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.id }

    fn intrinsics(&self) -> Arc<CameraIntrinsics> {
        self.inner.as_ref()
            .map(|s| s.intrinsics())
            .unwrap_or_else(|| Arc::new(CameraIntrinsics {
                fx: 640.0, fy: 640.0, cx: 320.0, cy: 240.0,
                width: 640, height: 480, distortion: [0.0; 3],
            }))
    }

    fn next_frame(&mut self) -> Option<Frame> {
        let inner = self.inner.as_mut()?;
        match inner.next_frame() {
            Some(frame) => {
                self.frames_since_connect += 1;
                self.last_frame_time = std::time::Instant::now();
                self.reconnect_delay_ms = 1000; // reset backoff on success
                Some(frame)
            }
            None => {
                // Stream ended or disconnected
                tracing::warn!("Camera {}: stream ended after {} frames", self.id, self.frames_since_connect);
                self.inner = None;
                None
            }
        }
    }

    fn connected(&self) -> bool {
        self.inner.is_some() && !self.is_stalled()
    }

    fn reconnect(&mut self) -> bool {
        tracing::info!(
            "Camera {}: reconnecting (backoff {}ms)",
            self.id, self.reconnect_delay_ms
        );

        // Exponential backoff
        std::thread::sleep(std::time::Duration::from_millis(self.reconnect_delay_ms));
        self.reconnect_delay_ms = (self.reconnect_delay_ms * 2).min(self.max_reconnect_delay_ms);

        let authed_url = match &self.credentials {
            Some(creds) => inject_credentials(&self.url, &creds.username, &creds.password),
            None => self.url.clone(),
        };

        match VideoSource::open(&self.id, &authed_url) {
            Ok(source) => {
                self.inner = Some(source);
                self.frames_since_connect = 0;
                self.last_frame_time = std::time::Instant::now();
                tracing::info!("Camera {}: reconnected", self.id);
                true
            }
            Err(e) => {
                tracing::warn!("Camera {}: reconnect failed: {}", self.id, e);
                false
            }
        }
    }
}

/// Inject credentials into an RTSP URL.
/// rtsp://host:port/path → rtsp://user:pass@host:port/path
fn inject_credentials(url: &str, username: &str, password: &str) -> String {
    if let Some(rest) = url.strip_prefix("rtsp://") {
        format!("rtsp://{}:{}@{}", username, password, rest)
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_credentials_basic() {
        let url = inject_credentials("rtsp://192.168.1.100:554/stream1", "admin", "pass123");
        assert_eq!(url, "rtsp://admin:pass123@192.168.1.100:554/stream1");
    }

    #[test]
    fn inject_credentials_non_rtsp_unchanged() {
        let url = inject_credentials("http://example.com/video", "admin", "pass");
        assert_eq!(url, "http://example.com/video");
    }

    #[test]
    fn connect_bad_file_fails() {
        // Use a file path instead of a network address — no timeout, instant failure
        let result = RtspSource::connect("test", "rtsp:///nonexistent/path", None);
        assert!(result.is_err());
    }
}
