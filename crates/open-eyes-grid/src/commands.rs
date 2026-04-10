//! Grid commands — what users and personas can request from open-eyes nodes.
//!
//! Follows the continuum Commands.execute() pattern:
//!   Commands.execute('open-eyes/camera/list', {}) → [{ id, status, ... }]
//!
//! Commands are registered via IPC when the open-eyes container starts.
//! The continuum CommandDaemon auto-discovers them and routes requests
//! to the camera node via the grid.

use serde::{Serialize, Deserialize};

/// Command names — constants, never magic strings.
pub mod names {
    pub const CAMERA_LIST: &str = "open-eyes/camera/list";
    pub const CAMERA_REGISTER: &str = "open-eyes/camera/register";
    pub const CAMERA_STREAM: &str = "open-eyes/camera/stream";
    pub const CAMERA_STATUS: &str = "open-eyes/camera/status";

    pub const SCENE_VIEW: &str = "open-eyes/scene/view";
    pub const SCENE_COVERAGE: &str = "open-eyes/scene/coverage";
    pub const SCENE_HISTORY: &str = "open-eyes/scene/history";

    pub const ENTITY_LIST: &str = "open-eyes/entity/list";
    pub const ENTITY_TRACK: &str = "open-eyes/entity/track";
    pub const ENTITY_THREATS: &str = "open-eyes/entity/threats";

    pub const TRIAGE_CONFIG: &str = "open-eyes/triage/config";
    pub const TRIAGE_TIER: &str = "open-eyes/triage/tier";
    pub const POWER_STATUS: &str = "open-eyes/power/status";
}

// ── Command params & results ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraListResult {
    pub cameras: Vec<CameraSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraSummary {
    pub id: String,
    pub name: Option<String>,
    pub connected: bool,
    pub fps: f32,
    pub resolution: Option<(u32, u32)>,
    pub triage_tier: String,
    pub battery_level: Option<f32>,
    pub last_motion: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneViewResult {
    /// Detected planes as geometry (normal + boundary polygon)
    pub planes: Vec<PlaneGeometry>,
    /// Currently tracked entities
    pub entities: Vec<EntitySummary>,
    /// Camera positions + FOV cones
    pub cameras: Vec<CameraGeometry>,
    /// Scene bounds
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub frames_processed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaneGeometry {
    pub normal: [f64; 3],
    pub distance: f64,
    pub label: String,
    pub boundary: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: String,
    pub class: String,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub confidence: f64,
    pub threat_level: f64,
    pub cameras_observing: Vec<String>,
    pub first_seen: f64,
    pub last_seen: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraGeometry {
    pub id: String,
    /// 4x4 column-major world transform
    pub transform: [f64; 16],
    pub fov_h_rad: f64,
    pub fov_v_rad: f64,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageResult {
    pub zones: Vec<ZoneCoverage>,
    pub dead_zones: Vec<DeadZone>,
    pub total_coverage_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneCoverage {
    pub zone_id: String,
    pub name: String,
    pub cameras: Vec<String>,
    pub coverage_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadZone {
    pub zone_id: String,
    /// Bounding box in world XZ coords
    pub bounds: [f64; 4],
    pub suggested_camera_position: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStatusResult {
    pub cameras: Vec<CameraPowerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraPowerStatus {
    pub id: String,
    pub source: String,
    pub battery_level: Option<f32>,
    pub charge_rate_w: f32,
    pub hours_remaining: Option<f32>,
    pub triage_tier: String,
    pub temperature_c: f32,
}

/// Route a command to the appropriate handler.
/// This is called by the IPC bridge when continuum-core forwards a command.
pub fn handle_command(name: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    match name {
        names::CAMERA_LIST => {
            // TODO: query actual camera state from the engine
            Ok(serde_json::to_value(CameraListResult { cameras: Vec::new() }).unwrap())
        }
        names::SCENE_VIEW => {
            // TODO: query scene from open-eyes-core
            Ok(serde_json::to_value(SceneViewResult {
                planes: Vec::new(),
                entities: Vec::new(),
                cameras: Vec::new(),
                bounds_min: [0.0; 3],
                bounds_max: [0.0; 3],
                frames_processed: 0,
            }).unwrap())
        }
        names::POWER_STATUS => {
            Ok(serde_json::to_value(PowerStatusResult { cameras: Vec::new() }).unwrap())
        }
        _ => Err(format!("Unknown command: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_namespaced() {
        assert!(names::CAMERA_LIST.starts_with("open-eyes/"));
        assert!(names::SCENE_VIEW.starts_with("open-eyes/"));
        assert!(names::POWER_STATUS.starts_with("open-eyes/"));
    }

    #[test]
    fn camera_list_returns_valid_json() {
        let result = handle_command(names::CAMERA_LIST, serde_json::Value::Null).unwrap();
        let parsed: CameraListResult = serde_json::from_value(result).unwrap();
        assert!(parsed.cameras.is_empty()); // no cameras connected yet
    }

    #[test]
    fn unknown_command_returns_error() {
        let result = handle_command("bogus/command", serde_json::Value::Null);
        assert!(result.is_err());
    }
}
