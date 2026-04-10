//! Entity tracker — builds persistent tracks from motion events.
//!
//! When the triage pipeline fires a motion event, the tracker:
//! 1. Checks if the motion matches an existing entity track
//! 2. If yes: update the track (new position, velocity)
//! 3. If no: create a new entity track
//! 4. Expire old tracks that haven't been observed recently
//!
//! All coordinates are normalized 0-1. No pixel counts. No resolution dependency.
//! The tracker doesn't know or care what resolution the camera runs at.

use crate::{TrackedEntity, EntityClass};

/// Configuration for the entity tracker.
pub struct TrackerConfig {
    /// Maximum distance (normalized) to match a detection to an existing track.
    /// 0.1 = 10% of frame diagonal.
    pub match_radius: f64,
    /// Seconds without observation before a track is expired.
    pub expire_seconds: f64,
    /// Minimum observations before a track is considered confirmed.
    pub confirm_threshold: u32,
    /// Velocity below this (normalized/sec) is considered stationary.
    pub stationary_threshold: f64,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            match_radius: 0.15,
            expire_seconds: 5.0,
            confirm_threshold: 3,
            stationary_threshold: 0.01,
        }
    }
}

/// An observation — a detection at a point in time.
#[derive(Debug, Clone)]
pub struct Observation {
    /// Position in normalized frame coords (0-1, 0-1)
    pub position: [f64; 2],
    /// Timestamp (seconds)
    pub timestamp: f64,
    /// Which camera saw this
    pub camera_id: String,
    /// Optional class from ML detector
    pub class: Option<EntityClass>,
    /// Confidence
    pub confidence: f64,
}

/// The entity tracker.
pub struct EntityTracker {
    config: TrackerConfig,
    entities: Vec<TrackedEntity>,
    next_id: u64,
    observation_counts: std::collections::HashMap<u64, u32>,
}

impl EntityTracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            entities: Vec::new(),
            next_id: 1,
            observation_counts: std::collections::HashMap::new(),
        }
    }

    /// Process a new observation. Returns the entity ID it was matched/assigned to.
    pub fn observe(&mut self, obs: &Observation) -> u64 {
        // Try to match to existing track (nearest within radius)
        let mut best_match: Option<(usize, f64)> = None;

        for (i, entity) in self.entities.iter().enumerate() {
            let dx = obs.position[0] - entity.position[0];
            let dy = obs.position[1] - entity.position[1];
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < self.config.match_radius {
                if best_match.is_none() || dist < best_match.unwrap().1 {
                    best_match = Some((i, dist));
                }
            }
        }

        if let Some((idx, _)) = best_match {
            // Update existing track
            let entity = &mut self.entities[idx];
            let dt = (obs.timestamp - entity.last_seen).max(0.001);

            // Velocity: position delta / time delta
            entity.velocity = [
                (obs.position[0] - entity.position[0]) / dt,
                (obs.position[1] - entity.position[1]) / dt,
            ];

            // Distance traveled
            let dx = obs.position[0] - entity.position[0];
            let dy = obs.position[1] - entity.position[1];
            entity.distance += (dx * dx + dy * dy).sqrt();

            entity.position = obs.position;
            entity.last_seen = obs.timestamp;
            entity.confidence = obs.confidence.max(entity.confidence * 0.9);

            let speed = (entity.velocity[0].powi(2) + entity.velocity[1].powi(2)).sqrt();
            entity.moving = speed > self.config.stationary_threshold;

            if let Some(class) = obs.class {
                if class != EntityClass::Unknown {
                    entity.class = class;
                }
            }

            if !entity.cameras.contains(&obs.camera_id) {
                entity.cameras.push(obs.camera_id.clone());
            }

            *self.observation_counts.entry(entity.id).or_insert(0) += 1;

            entity.id
        } else {
            // Create new track
            let id = self.next_id;
            self.next_id += 1;

            self.entities.push(TrackedEntity {
                id,
                class: obs.class.unwrap_or(EntityClass::Unknown),
                position: obs.position,
                velocity: [0.0, 0.0],
                confidence: obs.confidence,
                first_seen: obs.timestamp,
                last_seen: obs.timestamp,
                cameras: vec![obs.camera_id.clone()],
                distance: 0.0,
                moving: false,
            });

            self.observation_counts.insert(id, 1);
            id
        }
    }

    /// Expire tracks that haven't been observed recently.
    pub fn expire(&mut self, current_time: f64) {
        let threshold = self.config.expire_seconds;
        self.entities.retain(|e| {
            current_time - e.last_seen < threshold
        });
    }

    /// Get all active (non-expired) entities.
    pub fn entities(&self) -> &[TrackedEntity] {
        &self.entities
    }

    /// Get confirmed entities (seen enough times to be real, not noise).
    pub fn confirmed_entities(&self) -> Vec<&TrackedEntity> {
        self.entities.iter().filter(|e| {
            self.observation_counts.get(&e.id).copied().unwrap_or(0)
                >= self.config.confirm_threshold
        }).collect()
    }

    /// Get entity by ID.
    pub fn entity(&self, id: u64) -> Option<&TrackedEntity> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Number of active tracks.
    pub fn count(&self) -> usize {
        self.entities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_single_observation_creates_entity() {
        let mut tracker = EntityTracker::new(TrackerConfig::default());
        let id = tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: Some(EntityClass::Person),
            confidence: 0.9,
        });

        assert_eq!(tracker.count(), 1);
        let entity = tracker.entity(id).unwrap();
        assert_eq!(entity.class, EntityClass::Person);
        assert!((entity.position[0] - 0.5).abs() < 0.001);
    }

    #[test]
    fn vdd_nearby_observations_merge_to_same_entity() {
        let mut tracker = EntityTracker::new(TrackerConfig::default());

        // First observation at (0.5, 0.5)
        let id1 = tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.8,
        });

        // Second observation nearby at (0.52, 0.51) — should match
        let id2 = tracker.observe(&Observation {
            position: [0.52, 0.51],
            timestamp: 1.1,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.85,
        });

        assert_eq!(id1, id2, "Nearby observations should merge");
        assert_eq!(tracker.count(), 1);

        let entity = tracker.entity(id1).unwrap();
        assert!(entity.distance > 0.0, "Entity should have moved");
    }

    #[test]
    fn vdd_distant_observations_create_separate_entities() {
        let mut tracker = EntityTracker::new(TrackerConfig::default());

        tracker.observe(&Observation {
            position: [0.1, 0.1],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.8,
        });

        tracker.observe(&Observation {
            position: [0.9, 0.9],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.8,
        });

        assert_eq!(tracker.count(), 2, "Distant observations should create separate entities");
    }

    #[test]
    fn vdd_entity_expires() {
        let mut tracker = EntityTracker::new(TrackerConfig {
            expire_seconds: 2.0,
            ..Default::default()
        });

        tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.8,
        });

        assert_eq!(tracker.count(), 1);
        tracker.expire(2.0); // 1 second later — still alive
        assert_eq!(tracker.count(), 1);
        tracker.expire(4.0); // 3 seconds later — expired
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn vdd_velocity_computed_correctly() {
        let mut tracker = EntityTracker::new(TrackerConfig::default());

        // Entity at (0.5, 0.5) at t=1.0
        let id = tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        // Moves to (0.6, 0.5) at t=2.0 — within match_radius (0.15)
        // velocity = (0.1, 0.0) per second
        tracker.observe(&Observation {
            position: [0.6, 0.5],
            timestamp: 2.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        let entity = tracker.entity(id).unwrap();
        assert!((entity.velocity[0] - 0.1).abs() < 0.01, "vx={}", entity.velocity[0]);
        assert!((entity.velocity[1]).abs() < 0.01, "vy={}", entity.velocity[1]);
        assert!(entity.moving, "Entity should be flagged as moving");
        assert!(entity.distance > 0.09, "Should have traveled ~0.1 units");
    }

    #[test]
    fn vdd_confirmed_requires_minimum_observations() {
        let mut tracker = EntityTracker::new(TrackerConfig {
            confirm_threshold: 3,
            ..Default::default()
        });

        let id = tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        assert!(tracker.confirmed_entities().is_empty(), "1 observation: not confirmed");

        tracker.observe(&Observation {
            position: [0.51, 0.5],
            timestamp: 1.1,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        assert!(tracker.confirmed_entities().is_empty(), "2 observations: not confirmed");

        tracker.observe(&Observation {
            position: [0.52, 0.5],
            timestamp: 1.2,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        assert_eq!(tracker.confirmed_entities().len(), 1, "3 observations: confirmed");
    }

    #[test]
    fn vdd_multi_camera_tracks() {
        let mut tracker = EntityTracker::new(TrackerConfig::default());

        let id = tracker.observe(&Observation {
            position: [0.5, 0.5],
            timestamp: 1.0,
            camera_id: "cam-0".into(),
            class: None,
            confidence: 0.9,
        });

        tracker.observe(&Observation {
            position: [0.51, 0.5],
            timestamp: 1.1,
            camera_id: "cam-1".into(),
            class: None,
            confidence: 0.85,
        });

        let entity = tracker.entity(id).unwrap();
        assert_eq!(entity.cameras.len(), 2);
        assert!(entity.cameras.contains(&"cam-0".to_string()));
        assert!(entity.cameras.contains(&"cam-1".to_string()));
    }
}
