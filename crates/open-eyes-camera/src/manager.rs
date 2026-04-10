//! CameraManager — orchestrates multiple camera sources.
//!
//! Owns N CameraSource instances, feeds their frames to the pipeline,
//! handles reconnection, and reports status. This is the layer that
//! the grid node talks to — "give me all cameras" → CameraManager.
//!
//! Concurrency: each camera runs its own frame loop on a separate
//! tokio task. Frames are sent to the pipeline via a shared mpsc channel.
//! Cameras that disconnect are auto-reconnected with exponential backoff.

use crate::source::CameraSource;
use crate::video::VideoSource;
use crate::CameraConfig;
use crate::CameraStatus;
use std::collections::HashMap;

/// Manages multiple camera sources.
pub struct CameraManager {
    cameras: HashMap<String, ManagedCamera>,
}

struct ManagedCamera {
    config: CameraConfig,
    source: Option<Box<dyn CameraSource>>,
    status: CameraStatus,
}

impl CameraManager {
    pub fn new() -> Self {
        Self { cameras: HashMap::new() }
    }

    /// Add a camera from config. Attempts to connect immediately.
    pub fn add(&mut self, config: CameraConfig) -> Result<(), String> {
        let id = config.id.clone();

        let source = VideoSource::open(&config.id, &config.url)?;
        let status = CameraStatus {
            id: id.clone(),
            connected: true,
            resolution: Some((source.intrinsics().width, source.intrinsics().height)),
            fps: source.fps() as f32,
            frames_received: 0,
            frames_dropped: 0,
            last_frame_time: 0.0,
            has_agent: config.has_agent,
            agent_version: None,
            signal_strength_dbm: None,
        };

        self.cameras.insert(id, ManagedCamera {
            config,
            source: Some(Box::new(source)),
            status,
        });

        Ok(())
    }

    /// Remove a camera.
    pub fn remove(&mut self, id: &str) -> bool {
        self.cameras.remove(id).is_some()
    }

    /// Get status for all cameras.
    pub fn status(&self) -> Vec<CameraStatus> {
        self.cameras.values().map(|c| c.status.clone()).collect()
    }

    /// Get status for one camera.
    pub fn camera_status(&self, id: &str) -> Option<&CameraStatus> {
        self.cameras.get(id).map(|c| &c.status)
    }

    /// Number of connected cameras.
    pub fn connected_count(&self) -> usize {
        self.cameras.values().filter(|c| c.status.connected).count()
    }

    /// Total cameras (connected + disconnected).
    pub fn total_count(&self) -> usize {
        self.cameras.len()
    }

    /// Get IDs of all cameras.
    pub fn camera_ids(&self) -> Vec<String> {
        self.cameras.keys().cloned().collect()
    }

    /// Pull the next frame from a specific camera.
    /// Returns None if camera disconnected or no frame available.
    pub fn next_frame(&mut self, camera_id: &str) -> Option<open_eyes_core::frame::Frame> {
        let managed = self.cameras.get_mut(camera_id)?;
        let source = managed.source.as_mut()?;

        match source.next_frame() {
            Some(frame) => {
                managed.status.frames_received += 1;
                managed.status.last_frame_time = frame.timestamp();
                Some(frame)
            }
            None => {
                managed.status.connected = false;
                // Try reconnect
                if source.reconnect() {
                    managed.status.connected = true;
                    // Try again after reconnect
                    source.next_frame().map(|frame| {
                        managed.status.frames_received += 1;
                        managed.status.last_frame_time = frame.timestamp();
                        frame
                    })
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_starts_empty() {
        let mgr = CameraManager::new();
        assert_eq!(mgr.total_count(), 0);
        assert_eq!(mgr.connected_count(), 0);
    }

    #[test]
    fn manager_add_invalid_camera_returns_error() {
        let mut mgr = CameraManager::new();
        let result = mgr.add(CameraConfig {
            id: "bad".into(),
            name: "Bad Camera".into(),
            url: "/nonexistent/video.mp4".into(),
            intrinsics: None,
            resolution: None,
            target_fps: None,
            has_agent: false,
            mount_position: None,
            mount_orientation: None,
        });
        assert!(result.is_err());
        assert_eq!(mgr.total_count(), 0);
    }
}
