//! C-ABI types for the open-eyes FFI boundary.
//!
//! RULE: Everything that crosses this boundary is GEOMETRY.
//! No rasterized images. No pixel buffers as output.
//! The only pixels entering are raw camera frames.
//! The only pixels leaving are... nothing. The UI layer renders geometry.

use std::os::raw::c_char;

/// Pixel format of incoming camera frames.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OEPixelFormat {
    /// BGRA 8-bit per channel (iOS AVCaptureSession default)
    Bgra8 = 0,
    /// YUV 4:2:0 planar (Android Camera2 / CameraX)
    Yuv420 = 1,
    /// RGB 8-bit per channel
    Rgb8 = 2,
    /// Grayscale 8-bit
    Gray8 = 3,
}

/// Plane label — what kind of surface this is.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OEPlaneLabel {
    Floor = 0,
    Wall = 1,
    Ceiling = 2,
    Ground = 3,
    Table = 4,
    Door = 5,
    Window = 6,
    Fence = 7,
    Unknown = 255,
}

/// Entity class — what kind of thing is being tracked.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OEEntityClass {
    Person = 0,
    Vehicle = 1,
    Animal = 2,
    Package = 3,
    Unknown = 255,
}

/// Event types emitted by the pipeline.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OEEventType {
    /// Optical flow detected significant motion
    Motion = 0,
    /// Camera drift detected (recalibration may be needed)
    CameraDrift = 1,
    /// New entity entered scene
    EntityEntered = 2,
    /// Entity left scene
    EntityLeft = 3,
    /// Entity crossed zone boundary
    ZoneCrossing = 4,
    /// New plane detected
    PlaneDetected = 5,
    /// Pipeline error
    Error = 255,
}

/// A detected 3D plane — pure geometry.
/// Boundary is a polygon in 3D world coordinates, NOT a rasterized mask.
#[repr(C)]
pub struct OEPlane {
    pub normal: [f32; 3],
    pub distance: f32,
    pub label: OEPlaneLabel,
    pub confidence: f32,
    /// Polygon boundary vertices (3D world coords)
    pub boundary: *const [f32; 3],
    pub boundary_len: u32,
}

/// A tracked entity — position + velocity in 3D, NOT a bounding box image.
#[repr(C)]
pub struct OEEntity {
    pub id: u64,
    pub class: OEEntityClass,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub confidence: f32,
    pub threat_level: f32,
    /// Which cameras currently observe this entity (bitmask, up to 64 cameras)
    pub cameras_observing: u64,
}

/// An entity's trajectory — 3D polyline, NOT a rasterized path on an image.
#[repr(C)]
pub struct OETrail {
    pub entity_id: u64,
    pub class: OEEntityClass,
    /// 3D world-coordinate polyline
    pub points: *const [f32; 3],
    pub points_len: u32,
    /// Timestamp per point (seconds since epoch)
    pub timestamps: *const f64,
    /// Total distance traveled in meters
    pub distance_meters: f32,
}

/// Camera pose and frustum — pure transform, NOT a rendered FOV image.
#[repr(C)]
pub struct OECamera {
    pub id: u32,
    /// 4x4 column-major world transform
    pub transform: [f32; 16],
    pub fov_h_rad: f32,
    pub fov_v_rad: f32,
    pub connected: bool,
}

/// A 2D polygon in world XZ plane (for floor plan rendering).
/// The UI layer draws this as a polygon. We don't rasterize it.
#[repr(C)]
pub struct OEPolygon2D {
    /// Points in world XZ coordinates
    pub points: *const [f32; 2],
    pub points_len: u32,
}

/// Room in the scene graph — geometry + metadata.
#[repr(C)]
pub struct OERoom {
    pub id: u32,
    pub floor_level: i32,
    pub boundary: OEPolygon2D,
    pub occupancy_count: u32,
    /// Room type as a C string (null-terminated)
    pub room_type: *const c_char,
}

/// Complete scene snapshot — ALL geometry, ZERO pixels.
#[repr(C)]
pub struct OESceneSnapshot {
    pub planes: *const OEPlane,
    pub planes_len: u32,

    pub entities: *const OEEntity,
    pub entities_len: u32,

    pub trails: *const OETrail,
    pub trails_len: u32,

    pub cameras: *const OECamera,
    pub cameras_len: u32,

    pub rooms: *const OERoom,
    pub rooms_len: u32,

    /// Scene bounding box (world coords)
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],

    /// Motion magnitude (0.0 = static, 1.0+ = significant motion)
    pub motion_magnitude: f32,

    /// Frames processed since creation
    pub frames_processed: u64,
}

/// Pipeline event delivered via callback.
#[repr(C)]
pub struct OEEvent {
    pub event_type: OEEventType,
    pub camera_id: u32,
    pub entity_id: u64,
    /// Event-specific scalar (e.g. motion magnitude, drift amount)
    pub value: f32,
    pub position: [f32; 3],
    /// Monotonic timestamp (seconds)
    pub timestamp: f64,
}

/// Configuration for pipeline creation.
#[repr(C)]
pub struct OEConfig {
    /// Maximum cameras to support
    pub max_cameras: u32,
    /// Enable optical flow (Tier 1 — every frame)
    pub enable_flow: bool,
    /// Enable feature tracking
    pub enable_features: bool,
    /// Enable ML inference (normals, semantic — requires models)
    pub enable_ml: bool,
    /// Target processing FPS (0 = unlimited)
    pub target_fps: u32,
}

impl Default for OEConfig {
    fn default() -> Self {
        Self {
            max_cameras: 16,
            enable_flow: true,
            enable_features: true,
            enable_ml: false, // off until models are loaded
            target_fps: 0,
        }
    }
}
