//! VideoSource — CameraSource implementation backed by a video file or RTSP URL.
//!
//! Uses OpenCV VideoCapture under the hood. Handles:
//! - Local files (MP4, AVI, MOV)
//! - RTSP streams (rtsp://host:port/path)
//! - HTTP streams (http://host/stream.mjpg)
//! - USB/V4L2 devices (/dev/video0 or device index)
//!
//! Auto-reconnects on RTSP disconnect. Configurable retry interval.

use open_eyes_core::frame::Frame;
use open_eyes_core::cv::VideoReader;
use open_eyes_core::CameraIntrinsics;
use crate::source::CameraSource;
use std::sync::Arc;

pub struct VideoSource {
    id: String,
    name: String,
    url: String,
    reader: Option<VideoReader>,
    intrinsics: Arc<CameraIntrinsics>,
    frame_index: u64,
    connected: bool,
    reconnect_attempts: u32,
    max_reconnect_attempts: u32,
}

impl VideoSource {
    /// Open a video file or RTSP stream.
    pub fn open(id: &str, url: &str) -> Result<Self, String> {
        let reader = VideoReader::open(url)?;

        let intrinsics = Arc::new(CameraIntrinsics {
            fx: reader.width() as f64 * 0.8,
            fy: reader.height() as f64 * 0.8,
            cx: reader.width() as f64 / 2.0,
            cy: reader.height() as f64 / 2.0,
            width: reader.width(),
            height: reader.height(),
            distortion: [0.0; 3],
        });

        let name = if url.starts_with("rtsp://") {
            format!("RTSP {}", id)
        } else {
            std::path::Path::new(url)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| url.to_string())
        };

        Ok(Self {
            id: id.to_string(),
            name,
            url: url.to_string(),
            reader: Some(reader),
            intrinsics,
            frame_index: 0,
            connected: true,
            reconnect_attempts: 0,
            max_reconnect_attempts: 10,
        })
    }

    /// Open a USB/V4L2 camera by device index.
    pub fn open_device(id: &str, device_index: i32) -> Result<Self, String> {
        Self::open(id, &device_index.to_string())
    }

    pub fn url(&self) -> &str { &self.url }
    pub fn fps(&self) -> f64 {
        self.reader.as_ref().map(|r| r.fps()).unwrap_or(30.0)
    }
    pub fn total_frames(&self) -> u64 {
        self.reader.as_ref().map(|r| r.total_frames()).unwrap_or(0)
    }
}

impl CameraSource for VideoSource {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn intrinsics(&self) -> Arc<CameraIntrinsics> { self.intrinsics.clone() }

    fn next_frame(&mut self) -> Option<Frame> {
        let reader = self.reader.as_mut()?;
        let image = reader.next_frame()?;

        let timestamp = reader.timestamp();
        let frame = Frame::new(
            image,
            self.id.clone(),
            self.intrinsics.clone(),
            self.frame_index,
            timestamp,
        );
        self.frame_index += 1;
        Some(frame)
    }

    fn connected(&self) -> bool { self.connected }

    fn reconnect(&mut self) -> bool {
        if self.reconnect_attempts >= self.max_reconnect_attempts {
            tracing::error!("Camera {}: max reconnect attempts reached", self.id);
            return false;
        }

        self.reconnect_attempts += 1;
        tracing::info!(
            "Camera {}: reconnecting (attempt {}/{})",
            self.id, self.reconnect_attempts, self.max_reconnect_attempts
        );

        match VideoReader::open(&self.url) {
            Ok(reader) => {
                self.reader = Some(reader);
                self.connected = true;
                self.reconnect_attempts = 0;
                tracing::info!("Camera {}: reconnected", self.id);
                true
            }
            Err(e) => {
                tracing::warn!("Camera {}: reconnect failed: {}", self.id, e);
                self.connected = false;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_open_nonexistent_file_returns_error() {
        let result = VideoSource::open("cam-0", "/nonexistent/video.mp4");
        assert!(result.is_err());
    }

    // Integration test with a real video file would go here.
    // For now we test error paths. Real camera testing happens
    // on hardware via `oe-eval --video` or with a connected camera.
}
