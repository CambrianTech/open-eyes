//! open-eyes-grid — continuum grid integration.
//!
//! Makes open-eyes cameras first-class grid nodes in the continuum mesh.
//! Camera events flow through the grid event bus. Commands route to camera
//! nodes from any grid node. The Foreman orchestrates power/coverage.
//!
//! Integration follows the universal continuum pattern:
//! - Commands.execute('open-eyes/camera/list') → routes to camera node
//! - Events.emit('camera:motion:detected') → all subscribers on all nodes
//! - Docker container shares IPC socket with continuum-core
//!
//! This crate is the Rust side. The TypeScript side is a thin daemon
//! in continuum that bridges IPC events to the web Events system.

pub mod events;
pub mod commands;
pub mod node;
pub mod bridge;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod e2e_demo;
