//! Pipeline-to-grid bridge — converts pipeline events to grid events.
//!
//! The pipeline produces PipelineEvents (internal, Rust-only).
//! The grid needs GridEvents (serializable, IPC-ready).
//! This bridge converts between them and queues for transmission.
//!
//! This is the seam between "CV algorithm produced a result" and
//! "the continuum mesh knows about it." All normalization happens here.

use open_eyes_core::frame::PipelineEvent;
use open_eyes_detect::TrackedEntity;
use crate::events::{GridEvent, MotionPayload, EntityPayload};

/// Convert pipeline events to grid events.
/// Called after every frame — takes pipeline output, produces grid output.
pub fn pipeline_to_grid(
    node_id: &str,
    pipeline_events: &[PipelineEvent],
) -> Vec<GridEvent> {
    let mut grid_events = Vec::new();

    for event in pipeline_events {
        match event {
            PipelineEvent::Motion { camera_id, magnitude } => {
                // Only emit to grid if magnitude is significant
                // (the pipeline already thresholded, but we gate again
                // to avoid flooding the grid with noise)
                if *magnitude > 0.001 {
                    grid_events.push(GridEvent::motion(
                        node_id,
                        camera_id,
                        *magnitude,
                        [0.0, 0.0], // TODO: extract direction from flow field
                        0,          // TODO: extract quadrant from flow distribution
                    ));
                }
            }
            PipelineEvent::CameraDrift { camera_id, drift_pixels } => {
                grid_events.push(GridEvent {
                    topic: "camera:drift:detected".into(),
                    payload: serde_json::json!({
                        "camera_id": camera_id,
                        "drift_pixels": drift_pixels,
                    }),
                    node_id: node_id.into(),
                    timestamp: now(),
                });
            }
            PipelineEvent::Detection { camera_id, class, bbox, confidence } => {
                grid_events.push(GridEvent::entity_entered(
                    node_id,
                    EntityPayload {
                        camera_id: camera_id.clone(),
                        entity_id: format!("{}_{}", camera_id, now() as u64),
                        class: class.clone(),
                        position: [bbox[0], bbox[1], 0.0], // 2D bbox center, no depth yet
                        confidence: *confidence as f64,
                    },
                ));
            }
            PipelineEvent::Features { .. } | PipelineEvent::Lines { .. } => {
                // Internal pipeline events — don't emit to grid.
                // Features and lines are consumed by fusion/tracking,
                // not by personas or the UI.
            }
        }
    }

    grid_events
}

/// Convert tracked entities to grid events.
/// Called periodically (e.g., every 30 frames) to report entity state.
pub fn entities_to_grid(
    node_id: &str,
    entities: &[TrackedEntity],
) -> Vec<GridEvent> {
    entities.iter().map(|e| {
        GridEvent {
            topic: "scene:entity:tracked".into(),
            payload: serde_json::json!({
                "entity_id": e.id,
                "class": format!("{:?}", e.class),
                "position": e.position,
                "velocity": e.velocity,
                "confidence": e.confidence,
                "moving": e.moving,
                "cameras": e.cameras,
                "distance": e.distance,
                "first_seen": e.first_seen,
                "last_seen": e.last_seen,
            }),
            node_id: node_id.into(),
            timestamp: now(),
        }
    }).collect()
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_motion_event_converts() {
        let pipeline_events = vec![
            PipelineEvent::Motion {
                camera_id: "cam-0".into(),
                magnitude: 0.42,
            },
        ];

        let grid_events = pipeline_to_grid("node-1", &pipeline_events);

        assert_eq!(grid_events.len(), 1);
        assert_eq!(grid_events[0].topic, "camera:motion:detected");
        assert_eq!(grid_events[0].node_id, "node-1");

        let payload: MotionPayload = serde_json::from_value(
            grid_events[0].payload.clone()
        ).unwrap();
        assert_eq!(payload.camera_id, "cam-0");
        assert!((payload.magnitude - 0.42).abs() < 0.001);
    }

    #[test]
    fn vdd_low_magnitude_filtered() {
        let pipeline_events = vec![
            PipelineEvent::Motion {
                camera_id: "cam-0".into(),
                magnitude: 0.0005, // below grid threshold
            },
        ];

        let grid_events = pipeline_to_grid("node-1", &pipeline_events);
        assert!(grid_events.is_empty(), "Sub-threshold motion should not emit to grid");
    }

    #[test]
    fn vdd_features_not_emitted() {
        let pipeline_events = vec![
            PipelineEvent::Features {
                camera_id: "cam-0".into(),
                count: 150,
            },
        ];

        let grid_events = pipeline_to_grid("node-1", &pipeline_events);
        assert!(grid_events.is_empty(), "Feature events are internal, not grid events");
    }

    #[test]
    fn vdd_detection_converts_to_entity() {
        let pipeline_events = vec![
            PipelineEvent::Detection {
                camera_id: "cam-0".into(),
                class: "person".into(),
                bbox: [0.4, 0.5, 0.2, 0.6],
                confidence: 0.92,
            },
        ];

        let grid_events = pipeline_to_grid("node-1", &pipeline_events);
        assert_eq!(grid_events.len(), 1);
        assert_eq!(grid_events[0].topic, "camera:entity:entered");
    }

    #[test]
    fn vdd_mixed_events_correct_count() {
        let pipeline_events = vec![
            PipelineEvent::Motion { camera_id: "cam-0".into(), magnitude: 0.5 },
            PipelineEvent::Features { camera_id: "cam-0".into(), count: 100 },
            PipelineEvent::Lines { camera_id: "cam-0".into(), count: 12 },
            PipelineEvent::CameraDrift { camera_id: "cam-0".into(), drift_pixels: 3.5 },
            PipelineEvent::Motion { camera_id: "cam-1".into(), magnitude: 0.0001 }, // filtered
        ];

        let grid_events = pipeline_to_grid("node-1", &pipeline_events);
        // Motion(0.5) + Drift = 2 events. Features, Lines, low Motion filtered.
        assert_eq!(grid_events.len(), 2);
    }
}
