//! OpenCV integration — real CV algorithms, not hand-rolled placeholders.
//!
//! This module provides:
//! 1. Conversions between `image` crate types and `opencv::core::Mat`
//! 2. Real implementations of ORB features, Canny edges, optical flow
//! 3. Background subtraction for the on-device triage pipeline
//!
//! All functions take and return our types (FeaturePoint, EdgeMap, FlowField).
//! OpenCV is an implementation detail — callers don't need to know about Mat.

use crate::features::FeaturePoint;
use crate::frame::{EdgeMap, FlowField};
use opencv::prelude::*;
use opencv::core::{Mat, Size, Vector, CV_8UC1, BORDER_DEFAULT, AlgorithmHint};
use opencv::imgproc;
use opencv::features2d;
use opencv::video;
use opencv::videoio;

// ── Mat <-> image crate conversions ────────────────────────────────

/// Convert an `image::GrayImage` to an OpenCV `Mat` (8UC1).
/// Copies the data into an owned Mat — safe, works with all OpenCV functions.
pub fn gray_to_mat(grey: &image::GrayImage) -> Mat {
    let (w, h) = grey.dimensions();
    let mut mat = Mat::zeros(h as i32, w as i32, CV_8UC1).unwrap().to_mat().unwrap();
    let data = grey.as_raw();
    if let Ok(mat_data) = mat.data_bytes_mut() {
        let copy_len = data.len().min(mat_data.len());
        mat_data[..copy_len].copy_from_slice(&data[..copy_len]);
    }
    mat
}

/// Convert an OpenCV `Mat` (8UC1) back to `image::GrayImage`.
pub fn mat_to_gray(mat: &Mat) -> image::GrayImage {
    let w = mat.cols() as u32;
    let h = mat.rows() as u32;
    let data = mat.data_bytes().unwrap_or(&[]);
    image::GrayImage::from_raw(w, h, data.to_vec())
        .unwrap_or_else(|| image::GrayImage::new(w, h))
}

// ── Feature extraction (ORB) ───────────────────────────────────────

/// Extract ORB keypoints + descriptors from a greyscale image.
/// Returns our FeaturePoint type — OpenCV is an implementation detail.
pub fn extract_orb_features(grey: &image::GrayImage, max_features: usize) -> Vec<FeaturePoint> {
    let mat = gray_to_mat(grey);

    let mut orb = match features2d::ORB::create(
        max_features as i32,
        1.2,  // scale factor
        8,    // nlevels (pyramid)
        31,   // edge threshold
        0,    // first level
        2,    // WTA_K
        features2d::ORB_ScoreType::HARRIS_SCORE,
        31,   // patch size
        20,   // FAST threshold
    ) {
        Ok(orb) => orb,
        Err(_) => return Vec::new(),
    };

    let mut keypoints = Vector::new();
    let mut descriptors = Mat::default();
    let mask = Mat::default();

    if orb.detect_and_compute(&mat, &mask, &mut keypoints, &mut descriptors, false).is_err() {
        return Vec::new();
    }

    let mut features = Vec::with_capacity(keypoints.len());
    for i in 0..keypoints.len() {
        let kp = keypoints.get(i).unwrap();
        let descriptor = if !descriptors.empty() {
            let row = descriptors.row(i as i32).unwrap();
            row.data_bytes().unwrap_or(&[]).to_vec()
        } else {
            vec![0u8; 32]
        };

        features.push(FeaturePoint {
            pixel: (kp.pt().x as f64, kp.pt().y as f64),
            descriptor,
            track_id: None,
        });
    }

    features
}

// ── Edge detection (Canny) ─────────────────────────────────────────

/// Run Canny edge detection. Returns our EdgeMap type.
pub fn compute_canny_edges(grey: &image::GrayImage, low_threshold: f64, high_threshold: f64) -> EdgeMap {
    let mat = gray_to_mat(grey);
    let (w, h) = grey.dimensions();

    // Gaussian blur first (reduces noise, standard practice)
    let mut blurred = Mat::default();
    if imgproc::gaussian_blur(
        &mat, &mut blurred,
        Size::new(5, 5), 1.4, 1.4,
        BORDER_DEFAULT,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    ).is_err() {
        return EdgeMap { width: w, height: h, data: vec![0; (w * h) as usize] };
    }

    let mut edges = Mat::default();
    if imgproc::canny(&blurred, &mut edges, low_threshold, high_threshold, 3, false).is_err() {
        return EdgeMap { width: w, height: h, data: vec![0; (w * h) as usize] };
    }

    let data = edges.data_bytes().unwrap_or(&[]).to_vec();
    EdgeMap { width: w, height: h, data }
}

// ── Optical flow (Farneback dense) ─────────────────────────────────

/// Compute dense optical flow between two greyscale frames (Farneback).
/// Returns our FlowField type — per-pixel (dx, dy) vectors.
///
/// This is the CBAR heartbeat algorithm. Run at quarter-res for real-time.
pub fn compute_dense_flow(
    prev: &image::GrayImage,
    curr: &image::GrayImage,
) -> FlowField {
    let prev_mat = gray_to_mat(prev);
    let curr_mat = gray_to_mat(curr);
    let (w, h) = curr.dimensions();

    let mut flow = Mat::default();
    match video::calc_optical_flow_farneback(
        &prev_mat, &curr_mat,
        &mut flow,
        0.5,   // pyr_scale
        3,     // levels
        15,    // winsize
        3,     // iterations
        5,     // poly_n
        1.1,   // poly_sigma
        0,     // flags
    ) {
        Ok(_) => {},
        Err(e) => {
            #[cfg(test)]
            eprintln!("Farneback failed: {:?}", e);
            return FlowField { width: w, height: h, vectors: vec![(0.0, 0.0); (w * h) as usize] };
        }
    }

    // Debug removed — Farneback confirmed working

    // Extract flow vectors from the 2-channel Mat
    let mut vectors = Vec::with_capacity((w * h) as usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let pixel = flow.at_2d::<opencv::core::Vec2f>(y, x);
            match pixel {
                Ok(v) => vectors.push((v[0], v[1])),
                Err(_) => vectors.push((0.0, 0.0)),
            }
        }
    }

    FlowField { width: w, height: h, vectors }
}

// ── Sparse optical flow (Lucas-Kanade) ─────────────────────────────

/// Compute sparse optical flow tracking existing points (Lucas-Kanade).
/// More efficient than dense — tracks specific feature points frame-to-frame.
/// This is what runs on-device at the camera's ARM SoC.
pub fn compute_sparse_flow(
    prev: &image::GrayImage,
    curr: &image::GrayImage,
    prev_points: &[(f64, f64)],
) -> Vec<(f64, f64, f64, f64)> {
    // (x, y, dx, dy) for each tracked point
    if prev_points.is_empty() {
        return Vec::new();
    }

    let prev_mat = gray_to_mat(prev);
    let curr_mat = gray_to_mat(curr);

    // Convert points to OpenCV format
    let mut prev_pts = Vector::<opencv::core::Point2f>::new();
    for &(x, y) in prev_points {
        prev_pts.push(opencv::core::Point2f::new(x as f32, y as f32));
    }

    let mut next_pts = Vector::<opencv::core::Point2f>::new();
    let mut status = Vector::<u8>::new();
    let mut err = Vector::<f32>::new();

    let win_size = Size::new(21, 21);
    let max_level = 3;
    let criteria = opencv::core::TermCriteria::new(
        opencv::core::TermCriteria_Type::COUNT as i32 | opencv::core::TermCriteria_Type::EPS as i32,
        30,
        0.01,
    ).unwrap();

    if video::calc_optical_flow_pyr_lk(
        &prev_mat, &curr_mat,
        &prev_pts, &mut next_pts,
        &mut status, &mut err,
        win_size, max_level,
        criteria, 0, 1e-4,
    ).is_err() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(prev_points.len());
    for i in 0..prev_points.len() {
        if i < status.len() && status.get(i).unwrap_or(0) == 1 {
            let prev_pt = prev_pts.get(i).unwrap();
            let next_pt = next_pts.get(i).unwrap();
            results.push((
                prev_pt.x as f64,
                prev_pt.y as f64,
                (next_pt.x - prev_pt.x) as f64,
                (next_pt.y - prev_pt.y) as f64,
            ));
        }
    }

    results
}

// ── Background subtraction ─────────────────────────────────────────

/// Simple background diff — absolute difference between two frames,
/// threshold, return fraction of pixels that changed.
/// This is the cheapest possible motion detector (~0.5ms on ARM).
pub fn background_diff(reference: &image::GrayImage, current: &image::GrayImage, threshold: u8) -> f32 {
    let ref_mat = gray_to_mat(reference);
    let cur_mat = gray_to_mat(current);

    let mut diff = Mat::default();
    if opencv::core::absdiff(&ref_mat, &cur_mat, &mut diff).is_err() {
        return 0.0;
    }

    let mut thresholded = Mat::default();
    if imgproc::threshold(&diff, &mut thresholded, threshold as f64, 255.0, imgproc::THRESH_BINARY).is_err() {
        return 0.0;
    }

    let nonzero = opencv::core::count_non_zero(&thresholded).unwrap_or(0);
    let total = reference.width() * reference.height();
    if total == 0 { return 0.0; }

    nonzero as f32 / total as f32
}

// ── Downscale (for quarter-res processing) ─────────────────────────

/// Downsample a greyscale image by a factor (e.g., 4 for quarter-res).
/// Uses INTER_AREA for downsampling — proper anti-aliased reduction.
pub fn downsample(grey: &image::GrayImage, factor: u32) -> image::GrayImage {
    let mat = gray_to_mat(grey);
    let new_w = (grey.width() / factor).max(1) as i32;
    let new_h = (grey.height() / factor).max(1) as i32;

    let mut resized = Mat::default();
    if imgproc::resize(
        &mat, &mut resized,
        Size::new(new_w, new_h),
        0.0, 0.0,
        imgproc::INTER_AREA,
    ).is_err() {
        return image::GrayImage::new(new_w as u32, new_h as u32);
    }

    mat_to_gray(&resized)
}

// ── Video file reader ──────────────────────────────────────────

/// Read frames from a video file (MP4, AVI, RTSP URL, etc.)
/// Uses OpenCV's VideoCapture — handles all formats ffmpeg/GStreamer support.
///
/// Returns an iterator of (frame_index, RgbImage).
/// The caller pushes frames into the pipeline at their own pace.
pub struct VideoReader {
    cap: videoio::VideoCapture,
    frame_index: u64,
    width: u32,
    height: u32,
    fps: f64,
    total_frames: u64,
}

impl VideoReader {
    /// Open a video file or RTSP URL.
    pub fn open(path: &str) -> Result<Self, String> {
        let cap = videoio::VideoCapture::from_file(path, videoio::CAP_ANY)
            .map_err(|e| format!("Failed to open video: {}", e))?;

        if !cap.is_opened().unwrap_or(false) {
            return Err(format!("Could not open: {}", path));
        }

        let width = cap.get(videoio::CAP_PROP_FRAME_WIDTH).unwrap_or(0.0) as u32;
        let height = cap.get(videoio::CAP_PROP_FRAME_HEIGHT).unwrap_or(0.0) as u32;
        let fps = cap.get(videoio::CAP_PROP_FPS).unwrap_or(30.0);
        let total_frames = cap.get(videoio::CAP_PROP_FRAME_COUNT).unwrap_or(0.0) as u64;

        Ok(Self { cap, frame_index: 0, width, height, fps, total_frames })
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn fps(&self) -> f64 { self.fps }
    pub fn total_frames(&self) -> u64 { self.total_frames }
    pub fn frame_index(&self) -> u64 { self.frame_index }

    /// Read the next frame as an RgbImage. Returns None at end of video.
    pub fn next_frame(&mut self) -> Option<image::RgbImage> {
        let mut mat = Mat::default();
        if !self.cap.read(&mut mat).unwrap_or(false) {
            return None;
        }
        if mat.empty() {
            return None;
        }

        let w = mat.cols() as u32;
        let h = mat.rows() as u32;

        // OpenCV reads as BGR by default — convert to RGB
        let mut rgb_mat = Mat::default();
        if imgproc::cvt_color_def(&mat, &mut rgb_mat, imgproc::COLOR_BGR2RGB).is_err() {
            return None;
        }

        let data = rgb_mat.data_bytes().ok()?.to_vec();
        let img = image::RgbImage::from_raw(w, h, data)?;

        self.frame_index += 1;
        Some(img)
    }

    /// Timestamp of the current frame in seconds.
    pub fn timestamp(&self) -> f64 {
        if self.fps > 0.0 {
            self.frame_index as f64 / self.fps
        } else {
            self.frame_index as f64 / 30.0
        }
    }
}

/// Convenience: read all frames from a video file.
/// For small test videos only — large videos should use the iterator.
pub fn read_all_frames(path: &str) -> Result<Vec<image::RgbImage>, String> {
    let mut reader = VideoReader::open(path)?;
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_frame() {
        frames.push(frame);
    }
    Ok(frames)
}
