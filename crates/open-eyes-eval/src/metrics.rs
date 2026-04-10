//! CV evaluation metrics — the math behind VDD.
//!
//! Each metric takes predictions + ground truth and returns a score.
//! All metrics are deterministic — same input always produces same output.
//! That's what makes them attestable via forge-alloy.

use serde::{Serialize, Deserialize};

/// Complete results for one benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub name: String,
    pub metric: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    pub total_frames: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_sequences: Option<u32>,
    /// Per-sequence breakdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_sequence: Option<std::collections::HashMap<String, f64>>,
    /// Confusion matrix (for detection metrics)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_positives: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_positives: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_negatives: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_negatives: Option<u64>,
    /// Latency stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_mean_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_p99_ms: Option<f64>,
    /// Hardware the eval ran on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware: Option<String>,
    /// sha256 of the per-frame results log
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_hash: Option<String>,
}

/// Complete eval output — what oe-eval produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResults {
    pub dataset: String,
    pub subset: Option<String>,
    pub pipeline_version: String,
    pub pipeline_commit: Option<String>,
    pub benchmarks: Vec<BenchmarkResult>,
    pub passed: bool,
    pub duration_seconds: f64,
    pub dataset_hash: Option<String>,
}

// ── Metric computation functions ──────────────────────────────────

/// F1 score from precision and recall.
pub fn f1(precision: f64, recall: f64) -> f64 {
    if precision + recall == 0.0 {
        return 0.0;
    }
    2.0 * precision * recall / (precision + recall)
}

/// Precision from confusion matrix counts.
pub fn precision(tp: u64, fp: u64) -> f64 {
    if tp + fp == 0 { return 0.0; }
    tp as f64 / (tp + fp) as f64
}

/// Recall from confusion matrix counts.
pub fn recall(tp: u64, fn_count: u64) -> f64 {
    if tp + fn_count == 0 { return 0.0; }
    tp as f64 / (tp + fn_count) as f64
}

/// Per-pixel background subtraction evaluation.
/// Compares predicted foreground mask against ground truth mask.
/// Returns (true_positives, false_positives, false_negatives, true_negatives).
pub fn pixel_confusion(predicted: &[u8], ground_truth: &[u8]) -> (u64, u64, u64, u64) {
    let mut tp = 0u64;
    let mut fp = 0u64;
    let mut fn_ = 0u64;
    let mut tn = 0u64;

    for (p, g) in predicted.iter().zip(ground_truth.iter()) {
        let pred_fg = *p > 128;
        let gt_fg = *g > 128;
        match (pred_fg, gt_fg) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_ += 1,
            (false, false) => tn += 1,
        }
    }

    (tp, fp, fn_, tn)
}

/// Optical flow endpoint error (EPE).
/// L2 distance between predicted and ground truth flow vectors.
pub fn endpoint_error(predicted: &[(f32, f32)], ground_truth: &[(f32, f32)]) -> FlowStats {
    let mut errors: Vec<f64> = predicted.iter().zip(ground_truth.iter())
        .map(|((px, py), (gx, gy))| {
            let dx = (*px as f64) - (*gx as f64);
            let dy = (*py as f64) - (*gy as f64);
            (dx * dx + dy * dy).sqrt()
        })
        .collect();

    errors.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = errors.len();
    if n == 0 {
        return FlowStats { mean: 0.0, median: 0.0, p95: 0.0, max: 0.0 };
    }

    let mean = errors.iter().sum::<f64>() / n as f64;
    let median = errors[n / 2];
    let p95 = errors[(n as f64 * 0.95) as usize];
    let max = errors[n - 1];

    FlowStats { mean, median, p95, max }
}

#[derive(Debug, Clone)]
pub struct FlowStats {
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

/// Temporal IoU — overlap between predicted event windows and ground truth windows.
/// Each window is (start_frame, end_frame).
pub fn temporal_iou(
    predicted: &[(u64, u64)],
    ground_truth: &[(u64, u64)],
) -> f64 {
    if ground_truth.is_empty() {
        return if predicted.is_empty() { 1.0 } else { 0.0 };
    }

    let mut total_iou = 0.0;
    let mut matched = 0;

    for &(gt_start, gt_end) in ground_truth {
        let mut best_iou = 0.0;
        for &(pred_start, pred_end) in predicted {
            let inter_start = gt_start.max(pred_start);
            let inter_end = gt_end.min(pred_end);
            if inter_start < inter_end {
                let intersection = (inter_end - inter_start) as f64;
                let union = (gt_end - gt_start + pred_end - pred_start) as f64 - intersection;
                let iou = intersection / union;
                if iou > best_iou {
                    best_iou = iou;
                }
            }
        }
        if best_iou > 0.0 {
            total_iou += best_iou;
            matched += 1;
        }
    }

    if ground_truth.is_empty() { 0.0 } else { total_iou / ground_truth.len() as f64 }
}

/// Latency statistics from a vector of per-frame durations.
pub fn latency_stats(durations_ms: &[f64]) -> LatencyStats {
    if durations_ms.is_empty() {
        return LatencyStats { mean: 0.0, p50: 0.0, p99: 0.0, max: 0.0 };
    }

    let mut sorted = durations_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let n = sorted.len();
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let p50 = sorted[n / 2];
    let p99 = sorted[((n as f64) * 0.99) as usize];
    let max = sorted[n - 1];

    LatencyStats { mean, p50, p99, max }
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub mean: f64,
    pub p50: f64,
    pub p99: f64,
    pub max: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdd_f1_perfect() {
        assert!((f1(1.0, 1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn vdd_f1_zero_recall() {
        assert!((f1(1.0, 0.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn vdd_f1_balanced() {
        // precision=0.8, recall=0.6 → F1 = 2*0.8*0.6/(0.8+0.6) ≈ 0.6857
        assert!((f1(0.8, 0.6) - 0.6857).abs() < 0.001);
    }

    #[test]
    fn vdd_pixel_confusion_perfect_match() {
        let pred = vec![255u8, 0, 255, 0];
        let gt = vec![255u8, 0, 255, 0];
        let (tp, fp, fn_, tn) = pixel_confusion(&pred, &gt);
        assert_eq!(tp, 2);
        assert_eq!(fp, 0);
        assert_eq!(fn_, 0);
        assert_eq!(tn, 2);
    }

    #[test]
    fn vdd_pixel_confusion_all_false_positives() {
        let pred = vec![255u8, 255, 255, 255];
        let gt = vec![0u8, 0, 0, 0];
        let (tp, fp, fn_, tn) = pixel_confusion(&pred, &gt);
        assert_eq!(tp, 0);
        assert_eq!(fp, 4);
        assert_eq!(fn_, 0);
        assert_eq!(tn, 0);
    }

    #[test]
    fn vdd_epe_zero_for_identical() {
        let pred = vec![(1.0, 2.0), (3.0, 4.0)];
        let gt = vec![(1.0, 2.0), (3.0, 4.0)];
        let stats = endpoint_error(&pred, &gt);
        assert!(stats.mean < 0.001);
    }

    #[test]
    fn vdd_epe_known_displacement() {
        // All vectors off by (3, 4) → EPE = 5.0 for each
        let pred = vec![(3.0, 4.0), (3.0, 4.0)];
        let gt = vec![(0.0, 0.0), (0.0, 0.0)];
        let stats = endpoint_error(&pred, &gt);
        assert!((stats.mean - 5.0).abs() < 0.001);
    }

    #[test]
    fn vdd_temporal_iou_perfect_overlap() {
        let pred = vec![(100, 200)];
        let gt = vec![(100, 200)];
        assert!((temporal_iou(&pred, &gt) - 1.0).abs() < 0.001);
    }

    #[test]
    fn vdd_temporal_iou_half_overlap() {
        // pred: 100-200, gt: 150-250
        // intersection: 150-200 = 50 frames
        // union: 100-250 = 150 frames
        // IoU = 50/150 = 0.333
        let pred = vec![(100, 200)];
        let gt = vec![(150, 250)];
        assert!((temporal_iou(&pred, &gt) - 0.333).abs() < 0.01);
    }

    #[test]
    fn vdd_temporal_iou_no_overlap() {
        let pred = vec![(100, 200)];
        let gt = vec![(300, 400)];
        assert!((temporal_iou(&pred, &gt) - 0.0).abs() < 0.001);
    }

    #[test]
    fn vdd_latency_stats_known() {
        let durations = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = latency_stats(&durations);
        assert!((stats.mean - 3.0).abs() < 0.001);
        assert!((stats.p50 - 3.0).abs() < 0.001);
        assert!((stats.max - 5.0).abs() < 0.001);
    }
}
