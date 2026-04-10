//! open-eyes-detect — detection, tracking, and threat assessment
//!
//! Runs detection models (YOLO variants forged via sentinel-ai forge
//! pipeline) on camera feeds and the 3D scene. Tracks entities across
//! cameras in the unified 3D coordinate system — not per-camera 2D
//! bounding boxes but world-space 3D trajectories.
//!
//! Detection models are forged to run on consumer hardware via the
//! same forge-alloy pipeline that produces the LLM compactions.
//! The forge makes detection models small enough to run on a
//! Raspberry Pi or old laptop — the same "consumer hardware does
//! the impossible" thesis from continuum.

pub struct TrackedEntity {
    pub id: String,
    pub class: String,  // person, vehicle, animal, unknown
    pub position_3d: open_eyes_core::Point3,
    pub velocity: open_eyes_core::Vector3,
    pub confidence: f32,
    pub first_seen: f64,
    pub last_seen: f64,
    pub camera_observations: Vec<String>,  // which cameras have seen this entity
}
