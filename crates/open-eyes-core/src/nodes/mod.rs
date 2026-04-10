//! Processing nodes — subscribers in the pipeline.
//!
//! Each node implements ProcessNode, reads from the Frame's lazy getters,
//! and emits PipelineEvents. The pipeline feeds every frame to every
//! enabled node. Nodes are independent — no coupling between them.
//!
//! This is the CBP_AnalyzerThread hierarchy from CBAR, minus the
//! threading (handled by the pipeline's async executor instead).

pub mod motion;
pub mod background;
