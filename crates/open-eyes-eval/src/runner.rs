//! Eval runners — push dataset frames through the open-eyes pipeline,
//! compare against ground truth, return metrics.
//!
//! Each runner calls open-eyes-core directly (Rust-to-Rust, no FFI).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use open_eyes_core::frame::{Pipeline, FlowField};
use open_eyes_core::CameraIntrinsics;
use crate::datasets;
use crate::metrics::{self, BenchmarkResult, EvalResults};

fn default_intrinsics(width: u32, height: u32) -> Arc<CameraIntrinsics> {
    Arc::new(CameraIntrinsics {
        fx: width as f64 * 0.8,
        fy: height as f64 * 0.8,
        cx: width as f64 / 2.0,
        cy: height as f64 / 2.0,
        width, height,
        distortion: [0.0; 3],
    })
}

/// Run CDnet 2014 background subtraction evaluation.
pub fn run_cdnet(data_root: &Path, subset: Option<&str>) -> EvalResults {
    let subset_name = subset.unwrap_or("baseline");
    let subset_dir = data_root.join("cdnet2014").join(subset_name);

    tracing::info!("CDnet eval: {:?}", subset_dir);

    let mut pipeline = Pipeline::new();
    let mut benchmarks = Vec::new();
    let mut total_tp = 0u64;
    let mut total_fp = 0u64;
    let mut total_fn = 0u64;
    let mut total_tn = 0u64;
    let mut total_frames = 0u64;
    let mut latencies = Vec::new();

    let start = Instant::now();

    // Iterate over sequences in the subset directory
    if subset_dir.exists() {
        let sequences: Vec<_> = std::fs::read_dir(&subset_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        for seq_entry in &sequences {
            let seq_dir = seq_entry.path();
            let seq_name = seq_dir.file_name().unwrap().to_string_lossy().to_string();
            let input_dir = seq_dir.join("input");
            let gt_dir = seq_dir.join("groundtruth");

            if !input_dir.exists() || !gt_dir.exists() { continue; }

            let frames = datasets::load_frame_sequence(&input_dir);
            let gt_frames = datasets::load_frame_sequence(&gt_dir);

            tracing::info!("  Sequence {}: {} frames", seq_name, frames.len());

            for (frame_path, gt_path) in frames.iter().zip(gt_frames.iter()) {
                let img = match image::open(frame_path) {
                    Ok(img) => img.to_rgb8(),
                    Err(_) => continue,
                };
                let (w, h) = img.dimensions();
                let intrinsics = default_intrinsics(w, h);

                let frame_start = Instant::now();
                let _events = pipeline.process_frame(
                    img, &seq_name, intrinsics, total_frames as f64 / 30.0,
                );
                latencies.push(frame_start.elapsed().as_secs_f64() * 1000.0);

                // Compare against ground truth mask
                if let Some(gt_mask) = datasets::load_cdnet_gt_mask(gt_path) {
                    // Use edges as a proxy for foreground detection
                    // TODO: replace with proper background subtraction output
                    let predicted = vec![0u8; gt_mask.len()]; // placeholder
                    let (tp, fp, fn_, tn) = metrics::pixel_confusion(&predicted, &gt_mask);
                    total_tp += tp;
                    total_fp += fp;
                    total_fn += fn_;
                    total_tn += tn;
                }

                total_frames += 1;
            }
        }
    } else {
        tracing::warn!("Dataset directory not found: {:?}", subset_dir);
        tracing::info!("Download CDnet 2014 from http://jacarini.dinf.usherbrooke.ca/dataset2014/");
        tracing::info!("Extract to {:?}", subset_dir);
    }

    let prec = metrics::precision(total_tp, total_fp);
    let rec = metrics::recall(total_tp, total_fn);
    let f1 = metrics::f1(prec, rec);
    let lat = metrics::latency_stats(&latencies);

    benchmarks.push(BenchmarkResult {
        name: "background-subtraction".into(),
        metric: "f1".into(),
        score: f1,
        base_score: None,
        delta: None,
        total_frames,
        total_sequences: None,
        per_sequence: None,
        true_positives: Some(total_tp),
        false_positives: Some(total_fp),
        false_negatives: Some(total_fn),
        true_negatives: Some(total_tn),
        latency_mean_ms: Some(lat.mean),
        latency_p99_ms: Some(lat.p99),
        hardware: None,
        result_hash: None,
    });

    benchmarks.push(BenchmarkResult {
        name: "latency".into(),
        metric: "ms-per-frame".into(),
        score: lat.mean,
        base_score: None, delta: None,
        total_frames,
        total_sequences: None, per_sequence: None,
        true_positives: None, false_positives: None,
        false_negatives: None, true_negatives: None,
        latency_mean_ms: Some(lat.mean),
        latency_p99_ms: Some(lat.p99),
        hardware: None, result_hash: None,
    });

    EvalResults {
        dataset: "CDnet2014".into(),
        subset: Some(subset_name.into()),
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks,
        passed: f1 >= 0.85,
        duration_seconds: start.elapsed().as_secs_f64(),
        dataset_hash: None,
    }
}

/// Run Middlebury optical flow evaluation.
pub fn run_middlebury(data_root: &Path) -> EvalResults {
    let dataset_dir = data_root.join("middlebury");
    tracing::info!("Middlebury flow eval: {:?}", dataset_dir);

    let start = Instant::now();
    let mut benchmarks = Vec::new();
    let mut total_frames = 0u64;

    // TODO: iterate over frame pairs, compute flow, compare against .flo ground truth
    // For now, return empty results that indicate "dataset not loaded"
    if !dataset_dir.exists() {
        tracing::warn!("Dataset not found: {:?}", dataset_dir);
        tracing::info!("Download from https://vision.middlebury.edu/flow/data/");
    }

    benchmarks.push(BenchmarkResult {
        name: "optical-flow".into(),
        metric: "epe".into(),
        score: 0.0,
        base_score: None, delta: None,
        total_frames,
        total_sequences: None, per_sequence: None,
        true_positives: None, false_positives: None,
        false_negatives: None, true_negatives: None,
        latency_mean_ms: None, latency_p99_ms: None,
        hardware: None, result_hash: None,
    });

    EvalResults {
        dataset: "Middlebury".into(),
        subset: None,
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks,
        passed: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        dataset_hash: None,
    }
}

/// Run UCF-Crime event detection evaluation.
pub fn run_ucf_crime(data_root: &Path, subset: Option<&str>) -> EvalResults {
    let dataset_dir = data_root.join("ucf-crime");
    tracing::info!("UCF-Crime eval: {:?}", dataset_dir);
    let start = Instant::now();

    if !dataset_dir.exists() {
        tracing::warn!("Dataset not found: {:?}", dataset_dir);
        tracing::info!("Download from https://www.crcv.ucf.edu/projects/real-world/");
    }

    // TODO: decode MP4 videos, push through pipeline, compare event timestamps
    EvalResults {
        dataset: "UCF-Crime".into(),
        subset: subset.map(|s| s.into()),
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks: Vec::new(),
        passed: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        dataset_hash: None,
    }
}

/// Run CAVIAR event detection evaluation.
pub fn run_caviar(data_root: &Path) -> EvalResults {
    let dataset_dir = data_root.join("caviar");
    tracing::info!("CAVIAR eval: {:?}", dataset_dir);
    let start = Instant::now();

    if !dataset_dir.exists() {
        tracing::warn!("Dataset not found: {:?}", dataset_dir);
        tracing::info!("Download from https://homepages.inf.ed.ac.uk/rbf/CAVIAR/");
    }

    // TODO: load CAVIAR sequences + XML annotations, push through pipeline
    EvalResults {
        dataset: "CAVIAR".into(),
        subset: None,
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks: Vec::new(),
        passed: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        dataset_hash: None,
    }
}

/// Run EPFL multi-camera tracking evaluation.
pub fn run_epfl(data_root: &Path, subset: Option<&str>) -> EvalResults {
    let dataset_dir = data_root.join("epfl");
    tracing::info!("EPFL multi-cam eval: {:?}", dataset_dir);
    let start = Instant::now();

    if !dataset_dir.exists() {
        tracing::warn!("Dataset not found: {:?}", dataset_dir);
        tracing::info!("Download from https://www.epfl.ch/labs/cvlab/data/data-pom-index-php/");
    }

    // TODO: load 4-camera synchronized sequences, push through fusion pipeline
    EvalResults {
        dataset: "EPFL".into(),
        subset: subset.map(|s| s.into()),
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks: Vec::new(),
        passed: false,
        duration_seconds: start.elapsed().as_secs_f64(),
        dataset_hash: None,
    }
}
