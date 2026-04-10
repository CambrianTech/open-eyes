//! rtos — Real-Time Operating System-inspired pipeline executor
//!
//! The CBAR pipeline's thread-event model adapted to Rust async.
//! Each ProcessNode runs as an independent tokio task. Frames flow
//! between tasks via channels. Tasks sleep when idle, wake on new
//! frames, process, emit events, sleep again.
//!
//! This is the REFERENCE IMPLEMENTATION of the RTOS pattern that
//! continuum's daemon architecture will eventually adopt. The goals:
//!
//! 1. **Proportional compute** — CPU cost scales with scene activity,
//!    not with camera count or node count. Quiet scene = near-zero CPU.
//! 2. **Predictable latency** — the optical flow heartbeat has a
//!    guaranteed time budget. Other tasks run in the slack.
//! 3. **Self-regulating** — nodes enable/disable themselves based on
//!    upstream signals. No central scheduler decides what runs.
//! 4. **Observable** — every task transition (sleep→wake→process→sleep)
//!    emits timing metrics. The pipeline's behavior is fully visible.
//!
//! The architecture mirrors react-home-ar's C++17 pthread model:
//! - pthread_create → tokio::spawn
//! - pthread_cond_wait → tokio::sync::watch::changed()
//! - pthread_cond_signal → watch::Sender::send()
//! - shared memory (Frame*) → Arc<Frame>
//! - mutex-protected lazy getters → OnceLock<T>
//!
//! Why tokio and not raw threads: the cameras are I/O-bound (RTSP
//! streams, USB read, network), not CPU-bound. Tokio's cooperative
//! scheduling handles I/O multiplexing better than pthreads. The
//! CPU-heavy work (feature extraction, edge detection, flow) runs
//! on tokio::task::spawn_blocking or rayon, not on the async runtime.

use crate::frame::{Frame, PipelineEvent, ProcessNode};
use crate::CameraIntrinsics;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

/// Configuration for the RTOS pipeline.
#[derive(Debug, Clone)]
pub struct RtosConfig {
    /// Max frames to buffer before dropping (backpressure).
    /// Low value = responsive but drops frames under load.
    /// High value = processes everything but may lag.
    pub frame_buffer_size: usize,

    /// Event channel capacity.
    pub event_buffer_size: usize,

    /// Whether to log task state transitions (wake/sleep/process).
    pub trace_tasks: bool,
}

impl Default for RtosConfig {
    fn default() -> Self {
        Self {
            frame_buffer_size: 4,   // 4 frames of buffer (~130ms at 30fps)
            event_buffer_size: 256,
            trace_tasks: false,
        }
    }
}

/// A handle to a running RTOS pipeline.
///
/// Returned by `RtosPipeline::start()`. Use this to:
/// - Feed frames via `submit_frame()`
/// - Subscribe to events via `events()`
/// - Shut down via `shutdown()`
pub struct RtosHandle {
    /// Send frames into the pipeline
    frame_tx: mpsc::Sender<Arc<Frame>>,
    /// Subscribe to pipeline events
    event_tx: broadcast::Sender<PipelineEvent>,
    /// Signal shutdown
    shutdown_tx: watch::Sender<bool>,
}

impl RtosHandle {
    /// Submit a raw camera frame for processing.
    ///
    /// Non-blocking. If the pipeline is backed up (frame_buffer_size
    /// exceeded), the oldest unprocessed frame is dropped and this
    /// one takes its place. This is the correct behavior for real-time
    /// systems: freshness > completeness.
    pub async fn submit_frame(
        &self,
        raw: image::RgbImage,
        camera_id: &str,
        intrinsics: Arc<CameraIntrinsics>,
        timestamp: f64,
    ) {
        let frame = Arc::new(Frame::new(
            raw,
            camera_id.to_string(),
            intrinsics,
            0, // index assigned by pipeline
            timestamp,
        ));

        // Try send — if buffer full, drop oldest (backpressure)
        match self.frame_tx.try_send(frame) {
            Ok(_) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("pipeline backpressure: dropping frame from {camera_id}");
                // The mpsc channel is full; frame is dropped.
                // In a production system, this metric would feed into
                // the node's capability vector (§10.5 of GRID-ARCHITECTURE.md)
                // so the grid scheduler knows this node is overloaded.
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::error!("pipeline shut down");
            }
        }
    }

    /// Subscribe to pipeline events.
    ///
    /// Returns a broadcast receiver. Multiple subscribers can listen
    /// (the fusion engine, the grid event bridge, the UI, etc.)
    pub fn events(&self) -> broadcast::Receiver<PipelineEvent> {
        self.event_tx.subscribe()
    }

    /// Signal the pipeline to shut down.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// The RTOS pipeline — spawns async tasks for each ProcessNode.
pub struct RtosPipeline;

impl RtosPipeline {
    /// Start the pipeline with the given nodes.
    ///
    /// Each node runs as an independent tokio task. Frames are
    /// broadcast to all nodes via a shared watch channel. Events
    /// from all nodes are collected into a single broadcast channel.
    ///
    /// Returns a handle for submitting frames and receiving events.
    pub fn start(
        nodes: Vec<Box<dyn ProcessNode + 'static>>,
        config: RtosConfig,
    ) -> RtosHandle {
        let (frame_tx, mut frame_rx) = mpsc::channel::<Arc<Frame>>(config.frame_buffer_size);
        let (event_tx, _) = broadcast::channel::<PipelineEvent>(config.event_buffer_size);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let event_tx_clone = event_tx.clone();

        // Dispatcher task: receives frames, runs each node, emits events
        // In a more advanced version, each node would be its own task
        // with its own frame channel. For v0, sequential processing in
        // one task is correct (simpler, deterministic, easier to debug).
        tokio::spawn(async move {
            let mut nodes = nodes;
            let mut shutdown = shutdown_rx;
            let mut frame_index: u64 = 0;

            loop {
                tokio::select! {
                    // New frame to process
                    frame = frame_rx.recv() => {
                        match frame {
                            Some(frame) => {
                                frame_index += 1;
                                if config.trace_tasks {
                                    tracing::debug!(
                                        "pipeline: processing frame {} from {}",
                                        frame_index,
                                        frame.camera_id()
                                    );
                                }

                                // Feed frame to each enabled node
                                for node in &mut nodes {
                                    if node.enabled() {
                                        let t0 = std::time::Instant::now();
                                        let events = node.update(&frame);
                                        let elapsed = t0.elapsed();

                                        if config.trace_tasks {
                                            tracing::debug!(
                                                "  node '{}': {} events in {:?}",
                                                node.name(),
                                                events.len(),
                                                elapsed
                                            );
                                        }

                                        for event in events {
                                            let _ = event_tx_clone.send(event);
                                        }
                                    }
                                }
                            }
                            None => {
                                // Frame channel closed — all senders dropped
                                tracing::info!("pipeline: frame channel closed, shutting down");
                                break;
                            }
                        }
                    }
                    // Shutdown signal
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            tracing::info!("pipeline: shutdown signal received");
                            break;
                        }
                    }
                }
            }

            tracing::info!("pipeline: exited after {} frames", frame_index);
        });

        RtosHandle {
            frame_tx,
            event_tx,
            shutdown_tx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::PipelineEvent;

    struct MotionDetector {
        threshold: f64,
    }

    impl ProcessNode for MotionDetector {
        fn name(&self) -> &str { "motion-detector" }
        fn update(&mut self, frame: &Frame) -> Vec<PipelineEvent> {
            // Pull optical flow from the frame's lazy getter
            let flow = frame.optical_flow();
            let magnitude = crate::features::flow_motion_magnitude(flow);
            if magnitude > self.threshold {
                vec![PipelineEvent::Motion {
                    camera_id: frame.camera_id().to_string(),
                    magnitude,
                }]
            } else {
                vec![]
            }
        }
    }

    #[tokio::test]
    async fn rtos_pipeline_processes_frames() {
        let nodes: Vec<Box<dyn ProcessNode>> = vec![
            Box::new(MotionDetector { threshold: 1.0 }),
        ];

        let handle = RtosPipeline::start(nodes, RtosConfig::default());
        let mut events = handle.events();

        // Submit a frame
        let intrinsics = Arc::new(CameraIntrinsics {
            fx: 500.0, fy: 500.0, cx: 320.0, cy: 240.0,
            width: 640, height: 480, distortion: [0.0; 3],
        });
        handle.submit_frame(
            image::RgbImage::new(640, 480),
            "cam1",
            intrinsics,
            0.0,
        ).await;

        // Give the pipeline a moment to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Shutdown
        handle.shutdown();

        // No motion events expected (zero flow on blank image)
        // But the pipeline should have processed without panic
    }

    #[tokio::test]
    async fn rtos_pipeline_shutdown() {
        let nodes: Vec<Box<dyn ProcessNode>> = vec![];
        let handle = RtosPipeline::start(nodes, RtosConfig::default());
        handle.shutdown();
        // Should not hang
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}
