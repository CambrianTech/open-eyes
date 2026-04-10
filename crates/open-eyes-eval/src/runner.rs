//! Eval runners — push dataset frames through the open-eyes pipeline,
//! compare against ground truth, return metrics.
//!
//! Each runner calls open-eyes-core directly (Rust-to-Rust, no FFI).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use open_eyes_core::frame::{Pipeline, FlowField, PipelineEvent};
use open_eyes_core::nodes::background::BackgroundSubNode;
use open_eyes_core::nodes::motion::MotionDetectorNode;
use open_eyes_core::nodes::edge_density::EdgeDensityNode;
use open_eyes_core::CameraIntrinsics;
use crate::datasets;
use crate::metrics::{self, BenchmarkResult, EvalResults};
use crate::synthetic;

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

/// Run synthetic sequence evaluation — no downloads needed.
/// Tests the full triage pipeline against known ground truth.
pub fn run_synthetic(subset: Option<&str>) -> EvalResults {
    let start = Instant::now();
    let subset_name = subset.unwrap_or("all");

    tracing::info!("Synthetic eval: subset={}", subset_name);

    let sequences: Vec<(&str, Vec<synthetic::SyntheticFrame>)> = match subset_name {
        "moving-rectangle" => vec![("moving-rectangle", synthetic::moving_rectangle(320, 240, 100))],
        "lighting-change" => vec![("lighting-change", synthetic::lighting_change(320, 240, 100))],
        "person-enters" => vec![("person-enters", synthetic::person_enters(320, 240, 150))],
        _ => vec![
            ("moving-rectangle", synthetic::moving_rectangle(320, 240, 100)),
            ("lighting-change", synthetic::lighting_change(320, 240, 100)),
            ("person-enters", synthetic::person_enters(320, 240, 150)),
        ],
    };

    let mut benchmarks = Vec::new();
    let mut total_frames = 0u64;

    for (seq_name, seq) in &sequences {
        tracing::info!("  Sequence: {} ({} frames)", seq_name, seq.len());

        let mut pipeline = Pipeline::new();
        pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
        pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));
        pipeline.add_node(Box::new(EdgeDensityNode::full_res(0.03)));

        let mut tp = 0u64;  // motion frame correctly detected
        let mut fp = 0u64;  // static frame falsely detected
        let mut fn_ = 0u64; // motion frame missed
        let mut tn = 0u64;  // static frame correctly quiet
        let mut latencies = Vec::new();

        for frame in seq {
            let intrinsics = default_intrinsics(320, 240);
            let frame_start = Instant::now();

            let events = pipeline.process_frame(
                frame.image.clone(),
                seq_name,
                intrinsics,
                frame.frame_index as f64 / 30.0,
            );

            latencies.push(frame_start.elapsed().as_secs_f64() * 1000.0);

            let detected = events.iter().any(|e| matches!(e, PipelineEvent::Motion { .. }));

            // Skip first 3 frames (pipeline initialization)
            if frame.frame_index >= 3 {
                match (detected, frame.has_motion) {
                    (true, true) => tp += 1,
                    (true, false) => fp += 1,
                    (false, true) => fn_ += 1,
                    (false, false) => tn += 1,
                }
            }

            total_frames += 1;
        }

        let prec = metrics::precision(tp, fp);
        let rec = metrics::recall(tp, fn_);
        let f1 = metrics::f1(prec, rec);
        let lat = metrics::latency_stats(&latencies);

        tracing::info!(
            "  Results: TP={} FP={} FN={} TN={} P={:.3} R={:.3} F1={:.3} lat={:.1}ms",
            tp, fp, fn_, tn, prec, rec, f1, lat.mean
        );

        benchmarks.push(BenchmarkResult {
            name: format!("synthetic-{}", seq_name),
            metric: "f1".into(),
            score: f1,
            base_score: None,
            delta: None,
            total_frames: seq.len() as u64,
            total_sequences: Some(1),
            per_sequence: None,
            true_positives: Some(tp),
            false_positives: Some(fp),
            false_negatives: Some(fn_),
            true_negatives: Some(tn),
            latency_mean_ms: Some(lat.mean),
            latency_p99_ms: Some(lat.p99),
            hardware: None,
            result_hash: None,
        });
    }

    let duration = start.elapsed().as_secs_f64();
    let all_passed = benchmarks.iter().all(|b| b.score > 0.5);

    EvalResults {
        dataset: "synthetic".into(),
        subset: Some(subset_name.into()),
        pipeline_version: env!("CARGO_PKG_VERSION").into(),
        pipeline_commit: None,
        benchmarks,
        passed: all_passed,
        duration_seconds: duration,
        dataset_hash: None,
    }
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
