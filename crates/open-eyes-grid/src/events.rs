//! Grid event types — what open-eyes emits to the continuum event bus.
//!
//! These events follow the continuum naming convention:
//!   {domain}:{resource}:{action}
//!
//! Examples:
//!   camera:motion:detected
//!   camera:entity:entered
//!   scene:updated
//!
//! Events are serialized as JSON and sent over the IPC socket.
//! The continuum EventBridge picks them up and emits them to all
//! subscribers (personas, widgets, other grid nodes).

use serde::{Serialize, Deserialize};

/// All event types emitted by open-eyes nodes.
/// The string value is the event topic for Events.emit().
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventTopic {
    // Camera triage events
    #[serde(rename = "camera:motion:detected")]
    MotionDetected,
    #[serde(rename = "camera:drift:detected")]
    DriftDetected,
    #[serde(rename = "camera:presence:detected")]
    PresenceDetected,
    #[serde(rename = "camera:audio:detected")]
    AudioDetected,

    // Entity tracking events
    #[serde(rename = "camera:entity:entered")]
    EntityEntered,
    #[serde(rename = "camera:entity:left")]
    EntityLeft,
    #[serde(rename = "camera:zone:crossing")]
    ZoneCrossing,

    // Threat assessment
    #[serde(rename = "camera:threat:assessed")]
    ThreatAssessed,

    // System health
    #[serde(rename = "camera:connected")]
    CameraConnected,
    #[serde(rename = "camera:disconnected")]
    CameraDisconnected,
    #[serde(rename = "camera:battery:low")]
    BatteryLow,
    #[serde(rename = "camera:battery:critical")]
    BatteryCritical,
    #[serde(rename = "camera:solar:blocked")]
    SolarBlocked,
    #[serde(rename = "camera:coverage:gap")]
    CoverageGap,
    #[serde(rename = "camera:thermal:throttle")]
    ThermalThrottle,
    #[serde(rename = "camera:heartbeat")]
    Heartbeat,

    // Scene
    #[serde(rename = "scene:updated")]
    SceneUpdated,
    #[serde(rename = "scene:plane:detected")]
    PlaneDetected,
}

impl EventTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MotionDetected => "camera:motion:detected",
            Self::DriftDetected => "camera:drift:detected",
            Self::PresenceDetected => "camera:presence:detected",
            Self::AudioDetected => "camera:audio:detected",
            Self::EntityEntered => "camera:entity:entered",
            Self::EntityLeft => "camera:entity:left",
            Self::ZoneCrossing => "camera:zone:crossing",
            Self::ThreatAssessed => "camera:threat:assessed",
            Self::CameraConnected => "camera:connected",
            Self::CameraDisconnected => "camera:disconnected",
            Self::BatteryLow => "camera:battery:low",
            Self::BatteryCritical => "camera:battery:critical",
            Self::SolarBlocked => "camera:solar:blocked",
            Self::CoverageGap => "camera:coverage:gap",
            Self::ThermalThrottle => "camera:thermal:throttle",
            Self::Heartbeat => "camera:heartbeat",
            Self::SceneUpdated => "scene:updated",
            Self::PlaneDetected => "scene:plane:detected",
        }
    }
}

/// A grid event ready to send over IPC.
/// Serialized as JSON: { "topic": "camera:motion:detected", "payload": { ... } }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridEvent {
    pub topic: String,
    pub payload: serde_json::Value,
    /// Source node ID
    pub node_id: String,
    /// Monotonic timestamp (seconds since node boot)
    pub timestamp: f64,
}

// ── Typed event payloads ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionPayload {
    pub camera_id: String,
    /// Normalized 0-1 (fraction of frame diagonal)
    pub magnitude: f64,
    /// Normalized direction vector
    pub direction: [f64; 2],
    /// Which quadrant (0-3)
    pub quadrant: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityPayload {
    pub camera_id: String,
    pub entity_id: String,
    pub class: String,
    /// 3D world position
    pub position: [f64; 3],
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub camera_id: String,
    pub uptime_s: u64,
    pub temperature_c: f32,
    pub wifi_rssi: i8,
    pub light_level: f32,
    pub battery_level: Option<f32>,
    pub charge_rate_w: f32,
    pub triage_tier: String,
    pub fps: f32,
    pub frames_processed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGapPayload {
    pub zone: String,
    pub reason: String,
    pub compensating_cameras: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatPayload {
    pub entity_id: String,
    pub level: f64,
    pub reason: String,
    pub cameras: Vec<String>,
    pub position: [f64; 3],
}

impl GridEvent {
    pub fn motion(node_id: &str, camera_id: &str, magnitude: f64, direction: [f64; 2], quadrant: u8) -> Self {
        Self {
            topic: EventTopic::MotionDetected.as_str().into(),
            payload: serde_json::to_value(MotionPayload {
                camera_id: camera_id.into(),
                magnitude,
                direction,
                quadrant,
            }).unwrap(),
            node_id: node_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        }
    }

    pub fn heartbeat(node_id: &str, payload: HeartbeatPayload) -> Self {
        Self {
            topic: EventTopic::Heartbeat.as_str().into(),
            payload: serde_json::to_value(payload).unwrap(),
            node_id: node_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        }
    }

    pub fn entity_entered(node_id: &str, payload: EntityPayload) -> Self {
        Self {
            topic: EventTopic::EntityEntered.as_str().into(),
            payload: serde_json::to_value(payload).unwrap(),
            node_id: node_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        }
    }

    pub fn threat(node_id: &str, payload: ThreatPayload) -> Self {
        Self {
            topic: EventTopic::ThreatAssessed.as_str().into(),
            payload: serde_json::to_value(payload).unwrap(),
            node_id: node_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_topic_strings() {
        assert_eq!(EventTopic::MotionDetected.as_str(), "camera:motion:detected");
        assert_eq!(EventTopic::EntityEntered.as_str(), "camera:entity:entered");
        assert_eq!(EventTopic::Heartbeat.as_str(), "camera:heartbeat");
    }

    #[test]
    fn motion_event_serializes() {
        let event = GridEvent::motion("node-1", "cam-0", 0.42, [0.8, 0.6], 1);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("camera:motion:detected"));
        assert!(json.contains("0.42"));
        assert!(json.contains("node-1"));
    }

    #[test]
    fn heartbeat_event_serializes() {
        let event = GridEvent::heartbeat("node-1", HeartbeatPayload {
            camera_id: "cam-0".into(),
            uptime_s: 3600,
            temperature_c: 42.5,
            wifi_rssi: -65,
            light_level: 0.8,
            battery_level: Some(0.72),
            charge_rate_w: 1.2,
            triage_tier: "standard".into(),
            fps: 28.5,
            frames_processed: 102400,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("camera:heartbeat"));
        assert!(json.contains("102400"));
    }
}
