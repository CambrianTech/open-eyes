//! discovery — automatic camera discovery on the local network.
//!
//! Finds cameras without manual configuration:
//!
//! 1. ONVIF discovery (WS-Discovery multicast) — the standard
//!    protocol for IP camera auto-discovery
//! 2. mDNS/Bonjour — cameras advertising via _rtsp._tcp
//! 3. Network scan — probe common RTSP ports (554, 8554)
//! 4. Grid discovery — open-eyes agents announce via Tailscale
//!
//! The discovery module produces CandidateCamera entries that the
//! user confirms. No silent camera additions — human approves each.

use crate::CameraConfig;
use std::net::IpAddr;

/// A discovered camera candidate — not yet confirmed by the user.
#[derive(Debug, Clone)]
pub struct CandidateCamera {
    /// IP address found
    pub ip: IpAddr,
    /// RTSP port (usually 554)
    pub port: u16,
    /// Discovery method
    pub method: DiscoveryMethod,
    /// Probable RTSP URL (may need authentication)
    pub rtsp_url: String,
    /// Camera model/manufacturer if discoverable (ONVIF provides this)
    pub model: Option<String>,
    /// Whether the camera responded to a test connection
    pub reachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiscoveryMethod {
    /// WS-Discovery multicast (ONVIF standard)
    Onvif,
    /// mDNS/Bonjour service advertisement
    Mdns,
    /// Direct port scan on RTSP ports
    PortScan,
    /// open-eyes agent announcement via grid
    GridAgent,
    /// Manual URL entry by user
    Manual,
}

impl CandidateCamera {
    /// Convert to a CameraConfig (user confirmed this camera).
    pub fn to_config(&self, id: &str, name: &str) -> CameraConfig {
        CameraConfig {
            id: id.into(),
            name: name.into(),
            url: self.rtsp_url.clone(),
            intrinsics: None,
            resolution: None,
            target_fps: None,
            has_agent: self.method == DiscoveryMethod::GridAgent,
            mount_position: None,
            mount_orientation: None,
        }
    }
}

/// Common RTSP URL patterns for cheap cameras.
/// Try these in order until one connects.
pub fn common_rtsp_paths(ip: &IpAddr, port: u16) -> Vec<String> {
    let host = ip.to_string();
    vec![
        // Most common — works on majority of Chinese cameras
        format!("rtsp://{}:{}/stream1", host, port),
        format!("rtsp://{}:{}/ch0", host, port),
        format!("rtsp://{}:{}/live/ch0", host, port),
        format!("rtsp://{}:{}/h264_stream", host, port),
        // Hikvision
        format!("rtsp://{}:{}/Streaming/Channels/101", host, port),
        // Dahua
        format!("rtsp://{}:{}/cam/realmonitor?channel=1&subtype=0", host, port),
        // ONVIF profile S standard
        format!("rtsp://{}:{}/onvif1", host, port),
        format!("rtsp://{}:{}/media/video1", host, port),
        // Sub-streams (lower res, less bandwidth)
        format!("rtsp://{}:{}/stream2", host, port),
        format!("rtsp://{}:{}/ch0_1", host, port),
    ]
}

/// Test if an RTSP URL is reachable by trying to open it with OpenCV.
/// Returns true if the stream opens successfully.
pub fn test_rtsp_url(url: &str) -> bool {
    use open_eyes_core::cv::VideoReader;
    // Short timeout — just checking connectivity, not streaming
    match VideoReader::open(url) {
        Ok(reader) => reader.width() > 0 && reader.height() > 0,
        Err(_) => false,
    }
}

/// Probe a single IP for RTSP cameras.
/// Tries common URL patterns and returns the first that works.
pub fn probe_ip(ip: IpAddr) -> Option<CandidateCamera> {
    let ports = [554u16, 8554];

    for port in &ports {
        let urls = common_rtsp_paths(&ip, *port);
        for url in &urls {
            if test_rtsp_url(url) {
                return Some(CandidateCamera {
                    ip,
                    port: *port,
                    method: DiscoveryMethod::PortScan,
                    rtsp_url: url.clone(),
                    model: None,
                    reachable: true,
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn common_paths_cover_major_brands() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let paths = common_rtsp_paths(&ip, 554);
        assert!(paths.len() >= 8, "Should cover common camera brands");
        assert!(paths[0].contains("stream1"), "Most common path first");
        assert!(paths.iter().any(|p| p.contains("Streaming/Channels")), "Hikvision");
        assert!(paths.iter().any(|p| p.contains("realmonitor")), "Dahua");
    }

    #[test]
    fn candidate_to_config() {
        let candidate = CandidateCamera {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
            port: 554,
            method: DiscoveryMethod::PortScan,
            rtsp_url: "rtsp://192.168.1.50:554/stream1".into(),
            model: Some("Generic IP Camera".into()),
            reachable: true,
        };

        let config = candidate.to_config("cam-front", "Front Door");
        assert_eq!(config.id, "cam-front");
        assert_eq!(config.name, "Front Door");
        assert_eq!(config.url, "rtsp://192.168.1.50:554/stream1");
        assert!(!config.has_agent);
    }

    #[test]
    fn grid_agent_candidate_sets_has_agent() {
        let candidate = CandidateCamera {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
            port: 554,
            method: DiscoveryMethod::GridAgent,
            rtsp_url: "rtsp://192.168.1.50:554/stream1".into(),
            model: None,
            reachable: true,
        };

        let config = candidate.to_config("cam-back", "Back Yard");
        assert!(config.has_agent);
    }
}
