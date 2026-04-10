//! rtsp — RTSP client for connecting to camera streams
//!
//! The day-one integration path. Most cheap cameras expose RTSP
//! on their stock firmware. Common URLs:
//!
//!   rtsp://IP:554/stream1              (generic)
//!   rtsp://IP:554/ch0/main/av_stream   (HiSilicon)
//!   rtsp://IP:554/live/ch00_1          (Ingenic)
//!   rtsp://IP:8554/unicast             (some Virtavo/Bekamtron)
//!
//! The RTSP client handles:
//! - TCP/UDP transport negotiation
//! - H.264/H.265 frame decoding (via ffmpeg-next)
//! - Reconnection on stream loss (Wi-Fi cameras drop occasionally)
//! - Frame timestamping from RTP timestamps
//!
//! Authentication: most cheap cameras use digest auth with default
//! credentials (admin/admin, admin/empty). We support auth but
//! the open-eyes agent mode replaces this with Tailscale — no
//! passwords needed.

// TODO: implement RtspSource using ffmpeg-next for H.264 decoding
// The implementation follows the CameraSource trait from source/mod.rs
