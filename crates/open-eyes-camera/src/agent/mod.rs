//! agent — the on-camera open-eyes agent (runs on OpenIPC)
//!
//! This module contains the code that runs ON the camera itself
//! when flashed with OpenIPC + open-eyes firmware. It is cross-
//! compiled for ARM (HiSilicon/Sigmastar) or MIPS (Ingenic).
//!
//! The agent's job:
//! 1. Capture frames from the camera's ISP (V4L2)
//! 2. Run tier-1 optical flow at quarter-res (~5ms/frame on ARM)
//! 3. Emit motion events when flow exceeds threshold
//! 4. Serve RTSP stream to the grid node (H.264 HW-encoded)
//! 5. Report health (temp, uptime, WiFi signal, frame drops)
//!
//! The agent communicates with the grid node via:
//! - RTSP (video stream, H.264/H.265)
//! - Tailscale (encrypted transport, authentication)
//! - JSON events over TCP (motion, drift, health)
//!
//! Recovery: if the agent crashes, OpenIPC's init respawns it.
//! If the firmware is corrupted, UART + U-Boot reflash recovers.
//! If the SPI flash is damaged, a $10 CH341A programmer rewrites
//! it externally. You CANNOT permanently brick these cameras.
//!
//! SD card support: if the camera has an SD slot, the agent
//! stores a rolling buffer of recent frames locally. If the
//! Wi-Fi drops, nothing is lost — the buffer catches up when
//! connectivity resumes.

// TODO: implement V4L2 capture + tier-1 flow + RTSP server
// Cross-compile target: armv7-unknown-linux-gnueabihf (HiSilicon)
//                       mipsel-unknown-linux-gnu (Ingenic)
