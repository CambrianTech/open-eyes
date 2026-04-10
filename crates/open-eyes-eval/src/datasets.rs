//! Dataset loaders — read standard CV benchmark datasets into frames.
//!
//! Each loader reads the dataset's native format (JPEG sequences, .flo files,
//! MP4 video, annotation XML/JSON) and produces an iterator of frames + ground truth.

use std::path::{Path, PathBuf};
use image::RgbImage;
use serde::{Serialize, Deserialize};

/// A single frame from a dataset with optional ground truth.
pub struct DatasetFrame {
    pub sequence_name: String,
    pub frame_index: u64,
    pub image: RgbImage,
    pub ground_truth: Option<GroundTruth>,
}

/// Ground truth can be per-pixel masks, flow fields, or temporal annotations.
pub enum GroundTruth {
    /// Per-pixel foreground/background mask (CDnet, SBI)
    ForegroundMask(Vec<u8>),
    /// Dense optical flow field (Middlebury, KITTI, Sintel)
    FlowField { vectors: Vec<(f32, f32)>, width: u32, height: u32 },
    /// Temporal event annotation (UCF-Crime, CAVIAR, CUHK Avenue)
    TemporalEvent { class: String, start_frame: u64, end_frame: u64 },
    /// Bounding box track (VIRAT, EPFL)
    BoundingBox { id: u64, x: f32, y: f32, w: f32, h: f32, class: String },
}

/// Dataset metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub name: String,
    pub subset: Option<String>,
    pub total_frames: u64,
    pub total_sequences: u32,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub has_ground_truth: bool,
}

/// Load a JPEG/PNG frame sequence directory.
/// Expects: dir/frame_0001.jpg, frame_0002.jpg, ... or similar naming.
pub fn load_frame_sequence(dir: &Path) -> Vec<PathBuf> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("jpg" | "jpeg" | "png" | "bmp")
            )
        })
        .collect();
    frames.sort();
    frames
}

/// Load a Middlebury .flo file (dense ground truth flow).
/// Format: magic (202021.25 as f32), width (i32), height (i32), then w*h*(dx,dy) as f32 pairs.
pub fn load_flo_file(path: &Path) -> Option<(Vec<(f32, f32)>, u32, u32)> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 { return None; }

    // Check magic number
    let magic = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if (magic - 202021.25).abs() > 0.01 { return None; }

    let width = i32::from_le_bytes([data[4], data[5], data[6], data[7]]) as u32;
    let height = i32::from_le_bytes([data[8], data[9], data[10], data[11]]) as u32;

    let pixel_count = (width * height) as usize;
    let expected_size = 12 + pixel_count * 8; // 2 f32 per pixel
    if data.len() < expected_size { return None; }

    let mut vectors = Vec::with_capacity(pixel_count);
    for i in 0..pixel_count {
        let offset = 12 + i * 8;
        let dx = f32::from_le_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
        let dy = f32::from_le_bytes([data[offset+4], data[offset+5], data[offset+6], data[offset+7]]);
        vectors.push((dx, dy));
    }

    Some((vectors, width, height))
}

/// Load CDnet 2014 ground truth mask (PNG, single channel).
/// 0=static, 50=shadow (treated as static), 170=unknown (ignored), 255=foreground.
pub fn load_cdnet_gt_mask(path: &Path) -> Option<Vec<u8>> {
    let img = image::open(path).ok()?.to_luma8();
    Some(img.into_raw())
}

/// Load UCF-Crime temporal annotations.
/// Format varies — typically a text file with: video_name start_frame end_frame anomaly_type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalAnnotation {
    pub video_name: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub class: String,
}

pub fn load_temporal_annotations(path: &Path) -> Vec<TemporalAnnotation> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    content.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                Some(TemporalAnnotation {
                    video_name: parts[0].to_string(),
                    start_frame: parts[1].parse().ok()?,
                    end_frame: parts[2].parse().ok()?,
                    class: parts[3..].join(" "),
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_flo_magic_number() {
        // Construct a minimal .flo file: magic + 1x1 + one flow vector
        let mut data = Vec::new();
        data.extend_from_slice(&202021.25f32.to_le_bytes()); // magic
        data.extend_from_slice(&1i32.to_le_bytes());         // width
        data.extend_from_slice(&1i32.to_le_bytes());         // height
        data.extend_from_slice(&3.5f32.to_le_bytes());       // dx
        data.extend_from_slice(&(-1.2f32).to_le_bytes());    // dy

        let tmp = std::env::temp_dir().join("test_flow.flo");
        std::fs::write(&tmp, &data).unwrap();

        let (vectors, w, h) = load_flo_file(&tmp).unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(vectors.len(), 1);
        assert!((vectors[0].0 - 3.5).abs() < 0.001);
        assert!((vectors[0].1 - (-1.2)).abs() < 0.001);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn vdd_flo_bad_magic_returns_none() {
        let mut data = Vec::new();
        data.extend_from_slice(&0.0f32.to_le_bytes()); // wrong magic
        data.extend_from_slice(&1i32.to_le_bytes());
        data.extend_from_slice(&1i32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());

        let tmp = std::env::temp_dir().join("test_bad_flow.flo");
        std::fs::write(&tmp, &data).unwrap();
        assert!(load_flo_file(&tmp).is_none());
        std::fs::remove_file(&tmp).ok();
    }
}
