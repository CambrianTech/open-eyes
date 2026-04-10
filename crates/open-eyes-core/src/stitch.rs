//! stitch — Multi-camera view stitching and unified scene map
//!
//! Takes calibrated camera feeds and produces unified views:
//! - Top-down floor-plan-aligned 2D map (the quick win)
//! - Panoramic stitch across overlapping cameras
//! - Entity trails drawn across camera boundaries
//! - Dead zone visualization (where coverage gaps are)
//!
//! Three zone types with different concerns:
//!
//! **External** (property perimeter, yard, driveway, approaches)
//!   - Primary security concern: who's approaching, from where
//!   - Wide coverage, stitched top-down map
//!   - Entity tracking across the full perimeter
//!   - Weather/lighting variation handling
//!
//! **Internal** (rooms, hallways, entry points)
//!   - Privacy-first: optional, user-controlled per-room
//!   - Occupancy detection (which rooms are active)
//!   - Entry point monitoring (doors, windows)
//!   - Different retention policy (shorter, or occupancy-only no video)
//!
//! **Vehicle** (dashcam, cabin, surround view)
//!   - Moving platform: the car's GPS/IMU provides pose
//!   - Forward + rear + side cameras stitch into surround view
//!   - Cabin monitoring (driver alertness, passengers)
//!   - Trip recording with 3D context
//!
//! The stitching is a ProcessNode — it subscribes to calibrated
//! frames from the fusion engine and produces stitched views.
//! Same lazy pattern: if nobody requests the top-down map, the
//! stitching doesn't run.

use crate::geometry::Plane;
use crate::{CameraIntrinsics, Point3, Transform};
use std::collections::HashMap;

/// A zone in the scene with its own privacy and tracking rules.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Zone {
    /// Unique zone identifier
    pub id: String,
    /// Human-readable name ("Front yard", "Living room", "Garage")
    pub name: String,
    /// Zone type determines default privacy and retention rules
    pub zone_type: ZoneType,
    /// 2D boundary polygon (in world coordinates, projected to ground plane)
    pub boundary: Vec<[f64; 2]>,
    /// Which cameras cover this zone (by camera ID)
    pub cameras: Vec<String>,
    /// Privacy settings for this zone
    pub privacy: ZonePrivacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ZoneType {
    /// Property perimeter, yard, driveway, approaches
    External,
    /// Rooms, hallways, entry points inside the home
    Internal,
    /// Vehicle dashcam, cabin, surround view
    Vehicle,
    /// Public-facing (sidewalk, street visible from property)
    PublicFacing,
}

/// Privacy settings per zone — the human controls these.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZonePrivacy {
    /// Whether video is recorded in this zone
    pub record_video: bool,
    /// Whether entities are tracked in this zone
    pub track_entities: bool,
    /// Video retention period (hours). 0 = no retention.
    pub retention_hours: u32,
    /// Whether to share this zone's data with the community mesh (if opted in)
    pub share_with_mesh: bool,
    /// Occupancy-only mode: detect presence but don't store video
    pub occupancy_only: bool,
}

impl Default for ZonePrivacy {
    fn default() -> Self {
        Self {
            record_video: true,
            track_entities: true,
            retention_hours: 72,     // 3 days default
            share_with_mesh: false,  // never share by default
            occupancy_only: false,
        }
    }
}

impl ZonePrivacy {
    /// Conservative defaults for internal zones (privacy-first)
    pub fn internal_default() -> Self {
        Self {
            record_video: false,     // no video by default inside
            track_entities: false,
            retention_hours: 0,
            share_with_mesh: false,
            occupancy_only: true,    // presence detection only
        }
    }

    /// External zone defaults (security-oriented)
    pub fn external_default() -> Self {
        Self {
            record_video: true,
            track_entities: true,
            retention_hours: 168,    // 7 days
            share_with_mesh: false,
            occupancy_only: false,
        }
    }
}

/// A stitched top-down view of a zone.
///
/// This is the "quick win" output — a 2D bird's-eye map of the
/// property with all cameras' views warped onto a common ground plane.
#[derive(Debug, Clone)]
pub struct TopDownView {
    /// Zone this view covers
    pub zone_id: String,
    /// Pixels per meter in the output image
    pub scale: f64,
    /// Origin of the view in world coordinates
    pub origin: [f64; 2],
    /// Output image dimensions
    pub width: u32,
    pub height: u32,
    /// The rendered top-down image (RGBA)
    pub image: Vec<u8>,
    /// Camera coverage mask (which pixels are covered by at least one camera)
    pub coverage: Vec<bool>,
    /// Dead zones (regions with no camera coverage)
    pub dead_zones: Vec<[f64; 4]>,  // [x, y, w, h] in world coords
}

/// An entity trail on the stitched view — the path an entity took
/// across multiple cameras over time.
#[derive(Debug, Clone)]
pub struct EntityTrail {
    /// Entity ID from the detection/tracking system
    pub entity_id: String,
    /// Entity class (person, vehicle, animal)
    pub class: String,
    /// Trail points: (world_x, world_y, timestamp)
    pub points: Vec<(f64, f64, f64)>,
    /// Which cameras observed each point
    pub camera_observations: Vec<Vec<String>>,
    /// First seen timestamp
    pub first_seen: f64,
    /// Last seen timestamp
    pub last_seen: f64,
    /// Total distance traveled (meters)
    pub distance_meters: f64,
}

/// The stitching engine — produces unified views from calibrated cameras.
pub struct StitchEngine {
    /// Registered zones
    zones: Vec<Zone>,
    /// Per-camera homographies to the ground plane (for top-down view)
    /// Computed once during calibration, cached forever.
    homographies: HashMap<String, [[f64; 3]; 3]>,
    /// Active entity trails
    trails: Vec<EntityTrail>,
    /// The ground plane (from RANSAC plane fitting on the scene)
    ground_plane: Option<Plane>,
}

impl StitchEngine {
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            homographies: HashMap::new(),
            trails: Vec::new(),
            ground_plane: None,
        }
    }

    /// Define a zone in the scene.
    pub fn add_zone(&mut self, zone: Zone) {
        self.zones.push(zone);
    }

    /// Set the ground plane (needed for top-down projection).
    /// Typically computed by the fusion engine's RANSAC plane fitter.
    pub fn set_ground_plane(&mut self, plane: Plane) {
        self.ground_plane = Some(plane);
        // Invalidate cached homographies — they depend on the ground plane
        self.homographies.clear();
    }

    /// Compute the homography from a camera to the ground plane.
    ///
    /// This is the mapping that warps a camera's image onto the
    /// top-down view. Computed once per camera, cached until the
    /// ground plane or camera pose changes.
    pub fn compute_homography(
        &mut self,
        camera_id: &str,
        intrinsics: &CameraIntrinsics,
        pose: &Transform,
    ) {
        let ground = match &self.ground_plane {
            Some(g) => g,
            None => return, // can't compute without ground plane
        };

        // The homography H maps image points (u,v) to ground plane
        // points (x,y) via:
        //   [x, y, 1]^T ~ H * [u, v, 1]^T
        //
        // Derived from the camera's intrinsics, extrinsic pose, and
        // the ground plane equation. For a camera looking down at
        // the ground, this is a standard perspective-to-orthographic
        // projection.
        //
        // TODO: implement the actual homography computation from
        // K (intrinsics matrix), R|t (pose), and n·x+d=0 (plane).
        // For now, store an identity placeholder.
        let h = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        self.homographies.insert(camera_id.to_string(), h);
    }

    /// Project a 3D world point to the top-down 2D map coordinates.
    pub fn world_to_topdown(&self, point: &Point3, view: &TopDownView) -> Option<(f64, f64)> {
        // Project onto ground plane (drop the height component)
        // This assumes the ground plane is approximately horizontal
        // and the top-down view is aligned with world X/Y.
        let x = (point.x - view.origin[0]) * view.scale;
        let y = (point.y - view.origin[1]) * view.scale;

        if x >= 0.0 && x < view.width as f64 && y >= 0.0 && y < view.height as f64 {
            Some((x, y))
        } else {
            None
        }
    }

    /// Add a point to an entity's trail.
    pub fn update_trail(
        &mut self,
        entity_id: &str,
        class: &str,
        world_pos: (f64, f64),
        timestamp: f64,
        cameras: Vec<String>,
    ) {
        // Find or create the trail
        let trail = self.trails.iter_mut().find(|t| t.entity_id == entity_id);

        match trail {
            Some(trail) => {
                // Update existing trail
                let last = trail.points.last().unwrap();
                let dx = world_pos.0 - last.0;
                let dy = world_pos.1 - last.1;
                trail.distance_meters += (dx * dx + dy * dy).sqrt();
                trail.points.push((world_pos.0, world_pos.1, timestamp));
                trail.camera_observations.push(cameras);
                trail.last_seen = timestamp;
            }
            None => {
                // New trail
                self.trails.push(EntityTrail {
                    entity_id: entity_id.to_string(),
                    class: class.to_string(),
                    points: vec![(world_pos.0, world_pos.1, timestamp)],
                    camera_observations: vec![cameras],
                    first_seen: timestamp,
                    last_seen: timestamp,
                    distance_meters: 0.0,
                });
            }
        }
    }

    /// Get all active trails (entities seen in the last N seconds).
    pub fn active_trails(&self, since: f64) -> Vec<&EntityTrail> {
        self.trails.iter().filter(|t| t.last_seen >= since).collect()
    }

    /// Get the trail for a specific entity.
    pub fn trail(&self, entity_id: &str) -> Option<&EntityTrail> {
        self.trails.iter().find(|t| t.entity_id == entity_id)
    }

    /// Identify dead zones — regions in a zone with no camera coverage.
    pub fn find_dead_zones(&self, zone: &Zone) -> Vec<[f64; 4]> {
        // TODO: rasterize camera FOV polygons onto the zone boundary,
        // find uncovered regions. For now, return empty.
        Vec::new()
    }

    /// Prune old trails (entities not seen for a long time).
    pub fn prune_trails(&mut self, older_than: f64) {
        self.trails.retain(|t| t.last_seen >= older_than);
    }

    /// Get all defined zones.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_zone() {
        let mut engine = StitchEngine::new();
        engine.add_zone(Zone {
            id: "front-yard".into(),
            name: "Front Yard".into(),
            zone_type: ZoneType::External,
            boundary: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]],
            cameras: vec!["cam1".into(), "cam2".into()],
            privacy: ZonePrivacy::external_default(),
        });
        assert_eq!(engine.zones().len(), 1);
        assert_eq!(engine.zones()[0].zone_type, ZoneType::External);
    }

    #[test]
    fn internal_zone_privacy_defaults() {
        let privacy = ZonePrivacy::internal_default();
        assert!(!privacy.record_video);
        assert!(!privacy.track_entities);
        assert!(privacy.occupancy_only);
        assert!(!privacy.share_with_mesh);
    }

    #[test]
    fn entity_trail_tracking() {
        let mut engine = StitchEngine::new();

        engine.update_trail("person-1", "person", (0.0, 0.0), 0.0, vec!["cam1".into()]);
        engine.update_trail("person-1", "person", (1.0, 0.0), 1.0, vec!["cam1".into(), "cam2".into()]);
        engine.update_trail("person-1", "person", (1.0, 2.0), 2.0, vec!["cam2".into()]);

        let trail = engine.trail("person-1").unwrap();
        assert_eq!(trail.points.len(), 3);
        assert!((trail.distance_meters - 3.0).abs() < 0.01); // 1m right + 2m up = 3m
        assert_eq!(trail.first_seen, 0.0);
        assert_eq!(trail.last_seen, 2.0);
        // Second point was seen by two cameras
        assert_eq!(trail.camera_observations[1].len(), 2);
    }

    #[test]
    fn active_trails_filter() {
        let mut engine = StitchEngine::new();

        engine.update_trail("old", "person", (0.0, 0.0), 0.0, vec![]);
        engine.update_trail("recent", "person", (1.0, 1.0), 100.0, vec![]);

        let active = engine.active_trails(50.0);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].entity_id, "recent");
    }

    #[test]
    fn prune_old_trails() {
        let mut engine = StitchEngine::new();

        engine.update_trail("old", "person", (0.0, 0.0), 0.0, vec![]);
        engine.update_trail("new", "person", (1.0, 1.0), 100.0, vec![]);

        engine.prune_trails(50.0);
        assert_eq!(engine.trails.len(), 1);
        assert_eq!(engine.trails[0].entity_id, "new");
    }

    #[test]
    fn zone_types_are_distinct() {
        assert_ne!(ZoneType::External, ZoneType::Internal);
        assert_ne!(ZoneType::Internal, ZoneType::Vehicle);
        assert_ne!(ZoneType::Vehicle, ZoneType::PublicFacing);
    }
}
