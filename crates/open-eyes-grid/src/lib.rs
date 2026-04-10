//! open-eyes-grid — continuum grid integration
//!
//! Makes an open-eyes camera cluster a first-class grid node in the
//! continuum mesh. Camera feeds become events on the grid event bus.
//! The 3D scene reconstruction is available as a navigable view to
//! any continuum client on the mesh. Persona security teams subscribe
//! to detection events and reason about threats across the unified
//! 3D scene model.
//!
//! Transport: Tailscale (encrypted mesh) + Reticulum (offline-capable).
//! Same grid primitives as continuum — Commands.execute, Events.emit.

pub struct OpenEyesGridNode {
    pub node_id: String,
    pub cameras: Vec<String>,
    pub scene_endpoint: String,
}
