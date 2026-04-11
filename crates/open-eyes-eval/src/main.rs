//! oe-eval — VDD evaluation harness for open-eyes pipelines.
//!
//! Loads datasets (CDnet, Middlebury, UCF-Crime, CAVIAR, EPFL),
//! pushes frames through the pipeline, computes metrics against ground truth,
//! outputs forge-alloy compatible JSON results.
//!
//! This is a Rust binary — links directly to open-eyes-core, no FFI boundary.
//! No Python. No ctypes. Just Rust calling Rust.
//!
//! Usage:
//!   oe-eval --dataset cdnet --subset baseline --output results.json
//!   oe-eval --dataset middlebury --output flow-results.json
//!   oe-eval --alloy path/to/cv-eval.alloy.json

mod datasets;
mod metrics;
mod runner;
pub mod synthetic;
pub mod compat;

use std::path::PathBuf;

fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "--check-camera" => {
            let app = args.iter().position(|a| a == "--app")
                .and_then(|i| args.get(i + 1));
            let soc = args.iter().position(|a| a == "--soc")
                .and_then(|i| args.get(i + 1));

            let result = if let Some(app_name) = app {
                compat::check_by_app(app_name)
            } else if let Some(soc_name) = soc {
                compat::check_by_soc(soc_name)
            } else {
                eprintln!("Usage: oe-eval --check-camera --app <AppName>");
                eprintln!("       oe-eval --check-camera --soc <SoCName>");
                return;
            };

            compat::print_report(&result);
        }
        "--video" => {
            // Direct video file evaluation — quickest path to real data
            let video_path = args.get(2).expect("Usage: oe-eval --video <path.mp4>");
            run_video_file(video_path);
        }
        "--alloy" => {
            // Run from forge-alloy recipe
            let alloy_path = args.get(2).expect("Usage: oe-eval --alloy <path>");
            run_alloy(PathBuf::from(alloy_path));
        }
        "--dataset" => {
            // Run a single dataset eval directly
            let dataset = args.get(2).expect("Usage: oe-eval --dataset <name>");
            let subset = args.iter()
                .position(|a| a == "--subset")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            let output = args.iter()
                .position(|a| a == "--output")
                .and_then(|i| args.get(i + 1))
                .map(|s| PathBuf::from(s));
            let data_dir = args.iter()
                .position(|a| a == "--data-dir")
                .and_then(|i| args.get(i + 1))
                .map(|s| PathBuf::from(s));

            run_dataset(dataset, subset, output, data_dir);
        }
        _ => {
            print_usage();
        }
    }
}

fn print_usage() {
    eprintln!("oe-eval — VDD evaluation harness for open-eyes");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  oe-eval --video <path.mp4>                     Process any video file");
    eprintln!("  oe-eval --alloy <path.alloy.json>              Run from forge-alloy recipe");
    eprintln!("  oe-eval --dataset <name> [--subset <sub>]       Run single dataset eval");
    eprintln!("          [--output <path.json>]");
    eprintln!("          [--data-dir <path>]");
    eprintln!();
    eprintln!("Datasets: synthetic, cdnet, middlebury, ucf-crime, caviar, epfl");
}

/// Process a single video file through the full triage pipeline.
/// The quickest path from "I have a video" to "I see events."
fn run_video_file(path: &str) {
    use open_eyes_core::cv::VideoReader;
    use open_eyes_core::frame::{Pipeline, PipelineEvent};
    use open_eyes_core::nodes::background::BackgroundSubNode;
    use open_eyes_core::nodes::motion::MotionDetectorNode;
    use open_eyes_core::nodes::edge_density::EdgeDensityNode;
    use std::sync::Arc;
    use open_eyes_core::CameraIntrinsics;

    let mut reader = match VideoReader::open(path) {
        Ok(r) => r,
        Err(e) => { eprintln!("Error: {}", e); return; }
    };

    tracing::info!(
        "Video: {}x{} @ {:.1}fps, {} frames",
        reader.width(), reader.height(), reader.fps(), reader.total_frames()
    );

    let mut pipeline = Pipeline::new();
    pipeline.add_node(Box::new(BackgroundSubNode::full_res(25, 0.02)));
    pipeline.add_node(Box::new(MotionDetectorNode::full_res(0.005)));
    pipeline.add_node(Box::new(EdgeDensityNode::full_res(0.03)));

    let intrinsics = Arc::new(CameraIntrinsics {
        fx: reader.width() as f64 * 0.8,
        fy: reader.height() as f64 * 0.8,
        cx: reader.width() as f64 / 2.0,
        cy: reader.height() as f64 / 2.0,
        width: reader.width(),
        height: reader.height(),
        distortion: [0.0; 3],
    });

    let start = std::time::Instant::now();
    let mut event_count = 0u64;
    let mut frame_count = 0u64;

    while let Some(frame) = reader.next_frame() {
        let events = pipeline.process_frame(
            frame, "video", intrinsics.clone(), reader.timestamp(),
        );

        for event in &events {
            match event {
                PipelineEvent::Motion { camera_id, magnitude } => {
                    if event_count < 20 || event_count % 100 == 0 {
                        tracing::info!(
                            "frame {} | MOTION magnitude={:.4}",
                            frame_count, magnitude
                        );
                    }
                    event_count += 1;
                }
                PipelineEvent::CameraDrift { drift_pixels, .. } => {
                    tracing::warn!("frame {} | DRIFT {:.1}px", frame_count, drift_pixels);
                    event_count += 1;
                }
                _ => {}
            }
        }

        frame_count += 1;
        if frame_count % 100 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            tracing::info!("  {} frames, {:.1} fps, {} events", frame_count, fps, event_count);
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frame_count as f64 / elapsed;

    println!();
    println!("━━━ Results ━━━");
    println!("Frames:     {}", frame_count);
    println!("Events:     {}", event_count);
    println!("Duration:   {:.1}s", elapsed);
    println!("Throughput: {:.1} fps", fps);
    println!("Latency:    {:.1} ms/frame", elapsed * 1000.0 / frame_count.max(1) as f64);
}

fn run_alloy(alloy_path: PathBuf) {
    tracing::info!("Loading alloy from {:?}", alloy_path);
    let alloy_json = std::fs::read_to_string(&alloy_path)
        .expect("Failed to read alloy file");
    let alloy: serde_json::Value = serde_json::from_str(&alloy_json)
        .expect("Failed to parse alloy JSON");

    let stages = alloy["stages"].as_array().expect("Alloy must have stages array");

    for stage in stages {
        let stage_type = stage["type"].as_str().unwrap_or("");
        match stage_type {
            "cv-ingest" => {
                let dataset = stage["dataset"].as_str().unwrap_or("unknown");
                let subset = stage["subset"].as_str();
                tracing::info!("Ingesting dataset: {} (subset: {:?})", dataset, subset);
                // TODO: download/locate dataset, verify hash
            }
            "cv-eval" => {
                tracing::info!("Running CV evaluation");
                let benchmarks = stage["benchmarks"].as_array();
                if let Some(benchmarks) = benchmarks {
                    for bench in benchmarks {
                        let name = bench["name"].as_str().unwrap_or("unknown");
                        let metric = bench["metric"].as_str().unwrap_or("unknown");
                        tracing::info!("  Benchmark: {} ({})", name, metric);
                    }
                }
                // TODO: run pipeline, compute metrics, check acceptance criteria
            }
            _ => {
                tracing::warn!("Unknown stage type: {}", stage_type);
            }
        }
    }
}

fn run_dataset(dataset: &str, subset: Option<&str>, output: Option<PathBuf>, data_dir: Option<PathBuf>) {
    tracing::info!(
        "Running eval: dataset={}, subset={:?}, output={:?}",
        dataset, subset, output
    );

    let data_root = data_dir.unwrap_or_else(|| PathBuf::from("data/datasets"));

    let results = match dataset {
        "synthetic" => runner::run_synthetic(subset),
        "cdnet" => runner::run_cdnet(&data_root, subset),
        "middlebury" => runner::run_middlebury(&data_root),
        "ucf-crime" => runner::run_ucf_crime(&data_root, subset),
        "caviar" => runner::run_caviar(&data_root),
        "epfl" => runner::run_epfl(&data_root, subset),
        _ => {
            eprintln!("Unknown dataset: {}. Options: synthetic, cdnet, middlebury, ucf-crime, caviar, epfl", dataset);
            return;
        }
    };

    let json = serde_json::to_string_pretty(&results).unwrap();
    if let Some(output_path) = output {
        std::fs::write(&output_path, &json).expect("Failed to write results");
        tracing::info!("Results written to {:?}", output_path);
    } else {
        println!("{}", json);
    }
}
