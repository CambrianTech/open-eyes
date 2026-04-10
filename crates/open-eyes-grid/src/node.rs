//! Grid node — the open-eyes side of the continuum integration.
//!
//! An OpenEyesNode owns the pipeline, cameras, and scene state.
//! It handles commands from the grid and emits events to the grid.
//! The continuum Foreman manages it like any other grid resource.

use crate::events::{GridEvent, EventTopic};
use crate::commands;
use serde::{Serialize, Deserialize};

/// Configuration for an open-eyes grid node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node ID (typically the Tailscale hostname)
    pub node_id: String,
    /// Display name
    pub name: Option<String>,
    /// Maximum cameras this node can manage
    pub max_cameras: u32,
    /// IPC socket path for continuum-core connection
    pub ipc_socket: String,
    /// Data directory for scene state + recordings
    pub data_dir: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: gethostname(),
            name: None,
            max_cameras: 16,
            ipc_socket: "/root/.continuum/sockets/continuum-core.sock".into(),
            data_dir: "/data/open-eyes".into(),
        }
    }
}

/// The grid node — owns the pipeline and bridges to continuum.
pub struct OpenEyesNode {
    config: NodeConfig,
    /// Pending events to send to the grid
    outbound_events: Vec<GridEvent>,
}

impl OpenEyesNode {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            config,
            outbound_events: Vec::new(),
        }
    }

    /// Handle a command from the grid.
    pub fn handle_command(&self, name: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        commands::handle_command(name, params)
    }

    /// Queue an event for sending to the grid.
    pub fn emit(&mut self, event: GridEvent) {
        self.outbound_events.push(event);
    }

    /// Drain pending events (called by the IPC bridge).
    pub fn drain_events(&mut self) -> Vec<GridEvent> {
        std::mem::take(&mut self.outbound_events)
    }

    /// Get the node's capabilities for grid §10.5 matching.
    pub fn capabilities(&self) -> NodeCapabilities {
        NodeCapabilities {
            node_id: self.config.node_id.clone(),
            application: "open-eyes".into(),
            max_cameras: self.config.max_cameras,
            has_gpu: false, // TODO: detect
            has_npu: false,
            available_vram_gb: 0.0,
            commands: vec![
                commands::names::CAMERA_LIST.into(),
                commands::names::CAMERA_STATUS.into(),
                commands::names::SCENE_VIEW.into(),
                commands::names::SCENE_COVERAGE.into(),
                commands::names::ENTITY_LIST.into(),
                commands::names::ENTITY_TRACK.into(),
                commands::names::POWER_STATUS.into(),
            ],
            event_topics: vec![
                EventTopic::MotionDetected.as_str().into(),
                EventTopic::EntityEntered.as_str().into(),
                EventTopic::ThreatAssessed.as_str().into(),
                EventTopic::Heartbeat.as_str().into(),
            ],
        }
    }
}

/// Node capabilities — advertised to the grid for §10.5 routing.
/// The Foreman uses this to decide which node handles which request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub node_id: String,
    pub application: String,
    pub max_cameras: u32,
    pub has_gpu: bool,
    pub has_npu: bool,
    pub available_vram_gb: f32,
    /// Commands this node can handle
    pub commands: Vec<String>,
    /// Event topics this node emits
    pub event_topics: Vec<String>,
}

fn gethostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_creates_with_defaults() {
        let node = OpenEyesNode::new(NodeConfig::default());
        assert!(node.outbound_events.is_empty());
    }

    #[test]
    fn node_queues_events() {
        let mut node = OpenEyesNode::new(NodeConfig::default());
        node.emit(GridEvent::motion("test", "cam-0", 0.5, [1.0, 0.0], 0));
        node.emit(GridEvent::motion("test", "cam-1", 0.3, [0.0, 1.0], 2));
        assert_eq!(node.outbound_events.len(), 2);

        let drained = node.drain_events();
        assert_eq!(drained.len(), 2);
        assert!(node.outbound_events.is_empty());
    }

    #[test]
    fn capabilities_include_all_commands() {
        let node = OpenEyesNode::new(NodeConfig::default());
        let caps = node.capabilities();
        assert_eq!(caps.application, "open-eyes");
        assert!(caps.commands.contains(&commands::names::CAMERA_LIST.to_string()));
        assert!(caps.commands.contains(&commands::names::SCENE_VIEW.to_string()));
    }
}
