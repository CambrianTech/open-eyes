//! scene — The persistent 3D world model you navigate like Google Maps
//!
//! The scene is what you see when you open the open-eyes app on your
//! iPad and explore your property. Pinch to zoom, drag to pan, tap a
//! room to go inside. Like Google Maps' 3D view but it's YOUR space,
//! YOUR cameras built it, and it updates in real time.
//!
//! The scene is hierarchical:
//!
//! ```text
//! Property (the top-level world)
//!   ├── Exterior
//!   │   ├── Front yard (cameras 1, 2)
//!   │   ├── Backyard (cameras 3, 4, 5)
//!   │   ├── Driveway (camera 6)
//!   │   └── Side alley (camera 7)
//!   ├── Interior
//!   │   ├── Floor 1
//!   │   │   ├── Living room (camera 8)
//!   │   │   ├── Kitchen (camera 9)
//!   │   │   └── Front entry (camera 10)
//!   │   └── Floor 2
//!   │       ├── Master bedroom (no camera — occupancy only via WiFi CSI)
//!   │       └── Hallway (camera 11)
//!   ├── Garage
//!   │   └── Interior (camera 12)
//!   └── Vehicles
//!       └── Car (dashcam front + rear)
//! ```
//!
//! Navigation modes (all from the same model):
//!
//! **Bird's eye** — Google Maps style top-down. See the whole property.
//! Entity trails drawn in real time. Tap to zoom into any zone.
//!
//! **Floor plan** — architectural view of interior spaces. Walls,
//! doors, windows auto-detected from camera geometry + surface normals.
//! Room occupancy shown as heat overlays. Tap a room to see inside.
//!
//! **Walk-through** — first-person navigation through the 3D model.
//! Gaussian splats render the scene at interactive framerates. On
//! iPad: touch to move. On Vision Pro: walk physically. On desktop:
//! WASD. The same splat scene, different input method.
//!
//! **Timeline** — scrub backward in time. The whole property rewinds.
//! Entity trails show where everyone went. "What happened at 3am?"
//! answered on a unified map, not by clicking through camera clips.
//!
//! **Live overlay** — the 3D model with live camera feeds textured
//! onto the geometry. See what's happening NOW in spatial context.
//! Entity bounding boxes drawn in 3D, not in per-camera 2D.

use crate::geometry::Plane;
use crate::stitch::{Zone, ZoneType, EntityTrail};
use crate::{Point3, Transform, SceneState};
use std::collections::HashMap;

/// The top-level scene — everything open-eyes knows about the space.
///
/// This is the single object the UI navigates. It contains the
/// geometry, the zones, the live entity state, and the historical
/// timeline. The UI never talks to cameras directly — it talks
/// to the scene.
#[derive(Debug)]
pub struct Scene {
    /// Property-level metadata
    pub property: PropertyInfo,
    /// All defined zones (exterior, interior, vehicle)
    pub zones: Vec<Zone>,
    /// Detected planes (floors, walls, ceilings) from RANSAC + normals
    pub planes: Vec<Plane>,
    /// Room graph — which rooms connect to which via doors/hallways
    pub rooms: Vec<Room>,
    /// Floor definitions (for multi-story buildings)
    pub floors: Vec<Floor>,
    /// Live entity state (currently tracked entities)
    pub entities: HashMap<String, LiveEntity>,
    /// Historical trails (where entities went)
    pub trails: Vec<EntityTrail>,
    /// The accumulated 3D reconstruction from all cameras
    pub reconstruction: SceneState,
    /// Per-camera last-known status
    pub camera_status: HashMap<String, CameraSceneStatus>,
    /// Scene bounds (auto-computed from camera coverage)
    pub bounds: Option<SceneBounds>,
}

/// Property-level info shown in the top bar of the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropertyInfo {
    pub name: String,         // "Joel's House", "Mom's Place"
    pub address: Option<String>,
    pub timezone: String,
    pub total_cameras: u32,
    pub cameras_online: u32,
    pub total_area_sqm: Option<f64>,
}

/// A room in the interior floor plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,           // "Living Room", "Kitchen"
    pub floor_id: String,       // which floor this room is on
    pub zone_id: String,        // which zone this room belongs to
    /// Room boundary polygon (2D, in floor-plan coordinates)
    pub boundary: Vec<[f64; 2]>,
    /// Detected doors connecting to other rooms
    pub doors: Vec<Door>,
    /// Detected windows
    pub windows: Vec<Window>,
    /// Cameras that can see into this room
    pub cameras: Vec<String>,
    /// Current occupancy (from cameras or WiFi CSI)
    pub occupancy: Occupancy,
    /// Room type (auto-classified from furniture + layout)
    pub room_type: RoomType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoomType {
    LivingRoom,
    Kitchen,
    Bedroom,
    Bathroom,
    Hallway,
    Entry,
    Garage,
    Office,
    Closet,
    Stairs,
    Unknown,
}

/// A floor in a multi-story building.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Floor {
    pub id: String,
    pub name: String,           // "Ground Floor", "Second Floor", "Basement"
    pub level: i32,             // 0 = ground, 1 = second floor, -1 = basement
    pub elevation_m: f64,       // height above ground in meters
    pub rooms: Vec<String>,     // room IDs on this floor
}

/// A door connecting two rooms.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Door {
    pub position: [f64; 2],     // center point in floor-plan coords
    pub width_m: f64,
    pub connects: [String; 2],  // [room_a_id, room_b_id]
    pub is_exterior: bool,      // front door, back door, etc.
}

/// A window in a room.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub position: [f64; 2],
    pub width_m: f64,
    pub height_m: f64,
    pub faces_direction: Option<String>, // "north", "street", etc.
}

/// Current occupancy state of a room.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Occupancy {
    /// Number of entities currently in this room
    pub count: u32,
    /// How we know (camera, wifi_csi, inferred)
    pub source: OccupancySource,
    /// Last state change
    pub since: f64,
    /// Entity IDs in this room (if tracked, empty if occupancy-only)
    pub entity_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OccupancySource {
    Camera,      // direct visual detection
    WifiCsi,     // WiFi channel state information (through-wall)
    Inferred,    // last seen entering, not seen leaving
    Unknown,
}

/// A live tracked entity in the scene.
#[derive(Debug, Clone)]
pub struct LiveEntity {
    pub id: String,
    pub class: String,          // person, vehicle, animal, package
    pub position: Point3,       // current 3D world position
    pub velocity: [f64; 3],     // current velocity vector
    pub current_zone: Option<String>,
    pub current_room: Option<String>,
    pub cameras_observing: Vec<String>,
    pub confidence: f32,
    pub first_seen: f64,
    pub last_seen: f64,
    /// Threat level assessed by security persona (0.0 = none, 1.0 = critical)
    pub threat_level: f32,
    /// Whether the entity is known (family, regular visitor, etc.)
    pub known: bool,
    pub known_name: Option<String>,
}

/// Camera status within the scene context.
#[derive(Debug, Clone)]
pub struct CameraSceneStatus {
    pub camera_id: String,
    pub connected: bool,
    pub zone_id: Option<String>,
    pub rooms_visible: Vec<String>,
    pub fov_polygon: Vec<[f64; 2]>,  // FOV projected onto ground plane
    pub last_motion: f64,            // last time motion was detected
}

/// Scene bounding box.
#[derive(Debug, Clone)]
pub struct SceneBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
    /// Center point (for default camera position in 3D view)
    pub center: [f64; 3],
    /// Suggested viewing distance for bird's eye
    pub suggested_altitude: f64,
}

impl Scene {
    pub fn new(property: PropertyInfo) -> Self {
        Self {
            property,
            zones: Vec::new(),
            planes: Vec::new(),
            rooms: Vec::new(),
            floors: Vec::new(),
            entities: HashMap::new(),
            trails: Vec::new(),
            reconstruction: SceneState::default(),
            camera_status: HashMap::new(),
            bounds: None,
        }
    }

    /// Which room is this entity in? Check room boundaries.
    pub fn entity_room(&self, entity: &LiveEntity) -> Option<&Room> {
        let x = entity.position.x;
        let y = entity.position.y;
        self.rooms.iter().find(|room| {
            point_in_polygon(x, y, &room.boundary)
        })
    }

    /// All entities currently in a specific room.
    pub fn entities_in_room(&self, room_id: &str) -> Vec<&LiveEntity> {
        self.entities.values()
            .filter(|e| e.current_room.as_deref() == Some(room_id))
            .collect()
    }

    /// All entities currently in a specific zone.
    pub fn entities_in_zone(&self, zone_id: &str) -> Vec<&LiveEntity> {
        self.entities.values()
            .filter(|e| e.current_zone.as_deref() == Some(zone_id))
            .collect()
    }

    /// All unknown entities (potential threats).
    pub fn unknown_entities(&self) -> Vec<&LiveEntity> {
        self.entities.values()
            .filter(|e| !e.known && e.confidence > 0.5)
            .collect()
    }

    /// Total occupancy across all rooms.
    pub fn total_occupancy(&self) -> u32 {
        self.rooms.iter().map(|r| r.occupancy.count).sum()
    }
}

/// Point-in-polygon test (ray casting algorithm).
fn point_in_polygon(x: f64, y: f64, polygon: &[[f64; 2]]) -> bool {
    let n = polygon.len();
    if n < 3 { return false; }

    let mut inside = false;
    let mut j = n - 1;

    for i in 0..n {
        let (xi, yi) = (polygon[i][0], polygon[i][1]);
        let (xj, yj) = (polygon[j][0], polygon[j][1]);

        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }

    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_in_simple_rectangle() {
        let rect = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(point_in_polygon(5.0, 5.0, &rect));
        assert!(!point_in_polygon(15.0, 5.0, &rect));
        assert!(!point_in_polygon(-1.0, 5.0, &rect));
    }

    #[test]
    fn point_outside_polygon() {
        let rect = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(!point_in_polygon(15.0, 15.0, &rect));
        assert!(!point_in_polygon(-5.0, -5.0, &rect));
    }

    #[test]
    fn scene_entity_room_lookup() {
        let mut scene = Scene::new(PropertyInfo {
            name: "Test House".into(),
            address: None,
            timezone: "America/Chicago".into(),
            total_cameras: 2,
            cameras_online: 2,
            total_area_sqm: None,
        });

        scene.rooms.push(Room {
            id: "living".into(),
            name: "Living Room".into(),
            floor_id: "ground".into(),
            zone_id: "interior".into(),
            boundary: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 4.0], [0.0, 4.0]],
            doors: Vec::new(),
            windows: Vec::new(),
            cameras: vec!["cam8".into()],
            occupancy: Occupancy {
                count: 0, source: OccupancySource::Camera,
                since: 0.0, entity_ids: Vec::new(),
            },
            room_type: RoomType::LivingRoom,
        });

        let entity = LiveEntity {
            id: "person-1".into(),
            class: "person".into(),
            position: Point3::new(2.5, 2.0, 0.0),
            velocity: [0.0, 0.0, 0.0],
            current_zone: Some("interior".into()),
            current_room: None,
            cameras_observing: vec!["cam8".into()],
            confidence: 0.95,
            first_seen: 0.0,
            last_seen: 1.0,
            threat_level: 0.0,
            known: true,
            known_name: Some("Joel".into()),
        };

        let room = scene.entity_room(&entity);
        assert!(room.is_some());
        assert_eq!(room.unwrap().name, "Living Room");
    }
}
