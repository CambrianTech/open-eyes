//! discovery — automatic camera discovery on the local network
//!
//! Finds cameras without manual configuration:
//!
//! 1. ONVIF discovery (WS-Discovery multicast) — the standard
//!    protocol for IP camera auto-discovery. Most cameras that
//!    support RTSP also support ONVIF discovery.
//!
//! 2. mDNS/Bonjour — some cameras advertise via mDNS
//!
//! 3. Network scan — ARP scan + port probe on 554 (RTSP) and
//!    8554 (alternate RTSP). Brute-force but reliable.
//!
//! 4. Grid discovery — cameras running the open-eyes agent
//!    announce themselves to the grid automatically via Tailscale.
//!
//! The discovery module produces CameraConfig entries that the
//! user confirms and saves. No silent camera additions — the
//! human must approve each camera joining the system.

// TODO: implement ONVIF + network scan discovery
