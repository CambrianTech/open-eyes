//! open-eyes-detect — detection, tracking, and threat assessment.
//!
//! Takes motion events from the triage pipeline and builds persistent
//! entity tracks. Entities are tracked in normalized 2D space (0-1)
//! and promoted to 3D when multi-camera fusion provides depth.
//!
//! The tracker doesn't run ML models — it operates on the GEOMETRIC
//! output of the triage nodes (motion regions, edge density changes,
//! background diff masks). When ML detection models are available
//! (forged via sentinel-ai), they feed into this tracker with
//! classified bounding boxes.

pub mod tracker;

use serde::{Serialize, Deserialize};

/// A tracked entity — position + velocity in normalized space.
/// NOT pixel coordinates. NOT resolution-dependent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedEntity {
    pub id: u64,
    /// Entity class (person, vehicle, animal, unknown)
    pub class: EntityClass,
    /// Position in normalized frame coords (0-1, 0-1)
    pub position: [f64; 2],
    /// Velocity in normalized coords per second
    pub velocity: [f64; 2],
    /// Confidence (0-1)
    pub confidence: f64,
    /// First observed timestamp (seconds)
    pub first_seen: f64,
    /// Last observed timestamp (seconds)
    pub last_seen: f64,
    /// Which cameras currently observe this entity
    pub cameras: Vec<String>,
    /// Total distance traveled (normalized units)
    pub distance: f64,
    /// Is this entity moving or stationary?
    pub moving: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityClass {
    Person,
    Vehicle,
    Animal,
    Unknown,
}

impl Default for EntityClass {
    fn default() -> Self { Self::Unknown }
}
