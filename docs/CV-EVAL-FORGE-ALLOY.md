# CV Eval via Forge-Alloy — VDD for Computer Vision

**Status**: Design spec. Ready for implementation.

The forge-alloy pipeline already handles: `profile → prune → quant → eval → publish`. We extend it to handle CV pipeline validation using the same contract. Same factory queue, same heartbeat, same attestation. If we have to build separate eval infrastructure, we failed.

---

## The Alloy Shape

A CV eval alloy looks like any other alloy — source, stages, results, attestation:

```json
{
  "name": "open-eyes-triage-v0.1-cdnet-baseline",
  "version": "0.1.0",
  "description": "VDD eval of open-eyes triage pipeline against CDnet 2014 baseline",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "triage-pipeline",
    "revision": "c2832f3"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "CDnet2014",
      "subset": "baseline",
      "source": "http://jacarini.dinf.usherbrooke.ca/dataset2014/",
      "frameFormat": "jpeg-sequence",
      "resolution": "320x240",
      "fps": 30,
      "totalFrames": 4200
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-triage",
      "pipelineConfig": {
        "enableFlow": true,
        "enableBackgroundSub": true,
        "enableEdgeDensity": true,
        "flowResolution": "quarter",
        "backgroundAlpha": 0.005,
        "motionThreshold": 0.02
      },
      "benchmarks": [
        {
          "name": "background-subtraction",
          "metric": "f1",
          "groundTruth": "cdnet-baseline-gt-masks",
          "notes": "Per-pixel foreground/background against CDnet ground truth masks"
        },
        {
          "name": "event-detection",
          "metric": "temporal-iou",
          "groundTruth": "cdnet-baseline-temporal-annotations",
          "notes": "Did the pipeline fire motion events in the correct time windows?"
        },
        {
          "name": "latency",
          "metric": "ms-per-frame",
          "notes": "Wall clock time per frame on the eval hardware"
        }
      ],
      "acceptanceCriteria": {
        "background-subtraction": { "min": 0.85 },
        "event-detection": { "min": 0.90 },
        "latency": { "max": 15.0 }
      }
    }
  ]
}
```

---

## New Stage Types

### `cv-ingest` — Dataset Loading

Downloads / locates the dataset, validates its integrity, prepares frame sequences.

```python
class CVIngestStage(BaseModel):
    type: Literal["cv-ingest"] = "cv-ingest"
    dataset: str                              # Dataset name (CDnet2014, UCF-Crime, Middlebury, etc.)
    subset: Optional[str] = None              # Subset/category (baseline, night, dynamic-bg, etc.)
    source: Optional[str] = None              # Download URL or local path
    frame_format: Literal[
        "jpeg-sequence", "png-sequence",
        "video-mp4", "video-avi",
        "flo-pairs",                          # Middlebury .flo ground truth
    ] = Field(alias="frameFormat")
    resolution: Optional[str] = None          # "320x240", "1280x720", etc.
    fps: Optional[int] = None
    total_frames: Optional[int] = Field(default=None, alias="totalFrames")
    max_sequences: Optional[int] = Field(default=None, alias="maxSequences")
    ground_truth_format: Optional[str] = Field(default=None, alias="groundTruthFormat")
    dataset_hash: Optional[str] = Field(default=None, alias="datasetHash")
    notes: Optional[str] = None

    model_config = {"populate_by_name": True, "extra": "allow"}
```

### `cv-eval` — Pipeline Evaluation

Pushes frames through the open-eyes pipeline, collects events/outputs, compares against ground truth.

```python
class CVBenchmarkDef(BaseModel):
    """A single CV benchmark — what to measure and how."""
    name: str                                 # "background-subtraction", "optical-flow", "event-detection"
    metric: str                               # "f1", "precision", "recall", "epe", "temporal-iou", "ms-per-frame"
    ground_truth: Optional[str] = Field(default=None, alias="groundTruth")
    tolerance: Optional[float] = None         # Acceptable error margin (e.g., 0.5px for flow EPE)
    notes: Optional[str] = None

    model_config = {"populate_by_name": True, "extra": "allow"}


class CVAcceptanceCriterion(BaseModel):
    """Pass/fail gate for one metric."""
    min: Optional[float] = None               # Score must be >= this (for F1, precision, recall, IoU)
    max: Optional[float] = None               # Score must be <= this (for latency, EPE, error)
    anchor_delta: Optional[float] = Field(default=None, alias="anchorDelta")

    model_config = {"populate_by_name": True, "extra": "allow"}


class CVEvalStage(BaseModel):
    type: Literal["cv-eval"] = "cv-eval"
    pipeline: str                             # "open-eyes-triage", "open-eyes-full", etc.
    pipeline_config: dict = Field(default_factory=dict, alias="pipelineConfig")
    pipeline_version: Optional[str] = Field(default=None, alias="pipelineVersion")
    pipeline_commit: Optional[str] = Field(default=None, alias="pipelineCommit")
    benchmarks: list[CVBenchmarkDef]
    acceptance_criteria: dict[str, CVAcceptanceCriterion] = Field(
        default_factory=dict, alias="acceptanceCriteria"
    )
    # Hardware constraint — eval latency is hardware-dependent
    target_hardware: Optional[str] = Field(default=None, alias="targetHardware")
    notes: Optional[str] = None

    model_config = {"populate_by_name": True, "extra": "allow"}
```

---

## CV Benchmark Metrics

| Metric | What it measures | Used for | Ground truth needed |
|---|---|---|---|
| **f1** | Harmonic mean of precision + recall | Background subtraction, event detection | Per-pixel masks or temporal annotations |
| **precision** | True positives / (true positives + false positives) | Event detection (false alarm rate) | Temporal annotations |
| **recall** | True positives / (true positives + false negatives) | Event detection (miss rate) | Temporal annotations |
| **epe** | Endpoint error (L2 distance between predicted and ground truth flow) | Optical flow accuracy | Dense flow fields (.flo) |
| **temporal-iou** | Intersection over union of predicted event windows vs annotated windows | Event timing accuracy | Start/end frame annotations |
| **ms-per-frame** | Wall clock latency per frame | Real-time performance | None (measured) |
| **fps** | Frames per second sustained over the sequence | Throughput | None (measured) |
| **change-detection-rate** | Fraction of annotated changes detected by background model | Background subtraction sensitivity | Temporal annotations |
| **false-alarm-rate** | Fraction of static frames that triggered false events | Background subtraction specificity | Temporal annotations |

---

## CV Benchmark Results

Extends the existing `BenchmarkResult` type — same fields, CV-specific extras:

```python
class CVBenchmarkResult(BenchmarkResult):
    """CV-specific benchmark result with spatial/temporal detail."""
    # Per-sequence breakdown (not just aggregate)
    per_sequence: Optional[dict[str, float]] = Field(default=None, alias="perSequence")
    # Confusion matrix for event detection
    true_positives: Optional[int] = Field(default=None, alias="truePositives")
    false_positives: Optional[int] = Field(default=None, alias="falsePositives")
    false_negatives: Optional[int] = Field(default=None, alias="falseNegatives")
    true_negatives: Optional[int] = Field(default=None, alias="trueNegatives")
    # Flow-specific
    epe_mean: Optional[float] = Field(default=None, alias="epeMean")
    epe_median: Optional[float] = Field(default=None, alias="epeMedian")
    epe_95th: Optional[float] = Field(default=None, alias="epe95th")
    # Latency
    latency_mean_ms: Optional[float] = Field(default=None, alias="latencyMeanMs")
    latency_p99_ms: Optional[float] = Field(default=None, alias="latencyP99Ms")
    latency_max_ms: Optional[float] = Field(default=None, alias="latencyMaxMs")
    # What was tested
    total_frames: Optional[int] = Field(default=None, alias="totalFrames")
    total_sequences: Optional[int] = Field(default=None, alias="totalSequences")
    hardware: Optional[str] = None

    model_config = {"populate_by_name": True, "extra": "allow"}
```

---

## Example Alloys

### 1. Background Subtraction Validation (CDnet 2014)

```json
{
  "name": "open-eyes-bgsub-cdnet-baseline",
  "version": "0.1.0",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "triage-pipeline"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "CDnet2014",
      "subset": "baseline",
      "frameFormat": "jpeg-sequence",
      "groundTruthFormat": "png-masks"
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-triage",
      "pipelineConfig": {
        "enableBackgroundSub": true,
        "backgroundAlpha": 0.005,
        "changeThreshold": 25
      },
      "benchmarks": [
        { "name": "background-subtraction", "metric": "f1", "groundTruth": "cdnet-masks" },
        { "name": "background-subtraction", "metric": "precision", "groundTruth": "cdnet-masks" },
        { "name": "background-subtraction", "metric": "recall", "groundTruth": "cdnet-masks" }
      ],
      "acceptanceCriteria": {
        "background-subtraction": { "min": 0.85 }
      }
    }
  ]
}
```

### 2. Optical Flow Accuracy (Middlebury)

```json
{
  "name": "open-eyes-flow-middlebury",
  "version": "0.1.0",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "optical-flow"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "Middlebury",
      "subset": "eval",
      "frameFormat": "flo-pairs"
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-flow",
      "pipelineConfig": {
        "method": "farneback",
        "pyrScale": 0.5,
        "levels": 3,
        "winsize": 15
      },
      "benchmarks": [
        { "name": "optical-flow", "metric": "epe", "groundTruth": "middlebury-flo" }
      ],
      "acceptanceCriteria": {
        "optical-flow": { "max": 1.5 }
      },
      "notes": "EPE < 1.5px is competitive with published Farneback results"
    }
  ]
}
```

### 3. Event Detection on Surveillance Footage (UCF-Crime)

```json
{
  "name": "open-eyes-events-ucf-crime",
  "version": "0.1.0",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "triage-pipeline"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "UCF-Crime",
      "subset": "test",
      "frameFormat": "video-mp4",
      "maxSequences": 100
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-triage",
      "pipelineConfig": {
        "enableFlow": true,
        "enableBackgroundSub": true,
        "enableEdgeDensity": true,
        "enableAudio": false,
        "motionThreshold": 0.03
      },
      "benchmarks": [
        { "name": "anomaly-detection", "metric": "temporal-iou", "groundTruth": "ucf-crime-temporal" },
        { "name": "anomaly-detection", "metric": "precision" },
        { "name": "anomaly-detection", "metric": "recall" },
        { "name": "latency", "metric": "ms-per-frame" }
      ],
      "acceptanceCriteria": {
        "anomaly-detection": { "min": 0.70 },
        "latency": { "max": 15.0 }
      },
      "targetHardware": "ARM-Cortex-A7-500MHz",
      "notes": "Event detection must work within on-device triage budget (15ms/frame)"
    }
  ]
}
```

### 4. Multi-Camera Tracking (EPFL)

```json
{
  "name": "open-eyes-multicam-epfl",
  "version": "0.1.0",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "fusion-pipeline"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "EPFL-Pedestrian",
      "subset": "terrace",
      "frameFormat": "jpeg-sequence",
      "resolution": "360x288",
      "fps": 25
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-fusion",
      "pipelineConfig": {
        "cameras": 4,
        "enableCrossCamera": true,
        "enableTracking": true
      },
      "benchmarks": [
        { "name": "cross-camera-tracking", "metric": "mota", "groundTruth": "epfl-person-ids" },
        { "name": "cross-camera-tracking", "metric": "idf1", "groundTruth": "epfl-person-ids" },
        { "name": "registration-accuracy", "metric": "reprojection-error" }
      ],
      "acceptanceCriteria": {
        "cross-camera-tracking": { "min": 0.60 }
      },
      "notes": "MOTA > 0.60 with 4 cameras is competitive for unsupervised cross-view tracking"
    }
  ]
}
```

### 5. Full Triage Pipeline on ARM (Latency Validation)

```json
{
  "name": "open-eyes-arm-latency",
  "version": "0.1.0",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "triage-pipeline"
  },
  "stages": [
    {
      "type": "cv-ingest",
      "dataset": "CAVIAR",
      "frameFormat": "jpeg-sequence",
      "resolution": "384x288"
    },
    {
      "type": "cv-eval",
      "pipeline": "open-eyes-triage",
      "pipelineConfig": {
        "enableFlow": true,
        "enableBackgroundSub": true,
        "enableEdgeDensity": true,
        "flowResolution": "quarter"
      },
      "benchmarks": [
        { "name": "latency-total", "metric": "ms-per-frame" },
        { "name": "latency-background", "metric": "ms-per-frame" },
        { "name": "latency-flow", "metric": "ms-per-frame" },
        { "name": "latency-edges", "metric": "ms-per-frame" },
        { "name": "memory-peak", "metric": "mb" }
      ],
      "acceptanceCriteria": {
        "latency-total": { "max": 15.0 },
        "latency-background": { "max": 1.0 },
        "latency-flow": { "max": 8.0 },
        "memory-peak": { "max": 6.0 }
      },
      "targetHardware": "ARM-Cortex-A7-500MHz-64MB",
      "notes": "Must fit the on-device power/memory budget from ON-DEVICE-ARCHITECTURE.md"
    }
  ]
}
```

---

## How It Runs on BigMama

Same factory queue. Same daemon. The factory doesn't care what's inside the alloy — it reads stages, dispatches the executor, reports events.

```
BigMama factory queue:

Job 1: Mixtral 8x22B forge         [running — expert-activation-profile stage]
Job 2: open-eyes-bgsub-cdnet       [queued — cv-eval on CDnet baseline]
Job 3: open-eyes-flow-middlebury   [queued — flow accuracy on Middlebury]
Job 4: open-eyes-events-ucf-crime  [queued — event detection on UCF-Crime]
```

The cv-eval executor:
1. Downloads dataset (or uses cached copy on `/mnt/cold/datasets/`)
2. Builds the open-eyes pipeline with the specified config
3. Pushes every frame through `oe_push_frame`
4. Collects events and outputs
5. Compares against ground truth
6. Computes metrics (F1, EPE, latency, etc.)
7. Writes `BenchmarkResult` entries to the alloy
8. Checks acceptance criteria — pass or rework
9. Attestation: hashes the dataset, the pipeline binary, and the results

The result is a completed alloy with cryptographically attested CV metrics. "This pipeline achieved F1=0.91 on CDnet baseline" is a verifiable claim, same as "PPL 8.97 on wikitext-2-raw."

---

## The Attestation Chain

```
Dataset hash:     sha256 of CDnet2014 baseline frames
Pipeline hash:    sha256 of open-eyes-core binary at commit c2832f3
Config hash:      sha256 of pipelineConfig JSON
Results hash:     sha256 of per-frame event log
Ground truth:     sha256 of CDnet ground truth masks
                  ↓
              Alloy hash: sha256 of all the above
              Verify URL: cambriantech.github.io/forge-alloy/verify/#<hash>
              Trust: self-attested (same as model forges)
```

Anyone can download the same dataset, build the same pipeline at the same commit, run the same eval, and verify the hashes match. Reproducible CV eval. Same principle as reproducible model forges.

---

## Implementation Plan

1. **Add `CVIngestStage` and `CVEvalStage` to `forge-alloy/python/forge_alloy/types.py`**
2. **Add `CVBenchmarkResult` extending `BenchmarkResult`**
3. **Write the cv-eval executor in `sentinel-ai/scripts/stages/cv_eval.py`**
4. **Write dataset loaders for CDnet, Middlebury, UCF-Crime, CAVIAR, EPFL**
5. **Write metric computation (F1, EPE, temporal-IoU, latency)**
6. **Wire into `alloy_executor.py` stage dispatch**
7. **Seed alloy recipes for each dataset**
8. **Run on BigMama, verify results, attest**

Steps 1-2 are forge-alloy changes. Steps 3-6 are sentinel-ai. Steps 7-8 are operational.

The cv-eval executor calls open-eyes via the FFI — it loads `libopeneyes.so`, calls `oe_create`, pushes frames via `oe_push_frame`, polls events via `oe_poll_events`. Python is the orchestrator, Rust is the compute. Same pattern as the LLM forge: Python orchestrates, the model does the work.

---

## Cross-Family Anchor Table (Extended)

The anchor table now spans both model forging AND CV pipeline validation:

| Row | Artifact | Type | Key Metric | Status |
|---|---|---|---|---|
| 1 | qwen3-coder-30b-a3b-compacted | LLM forge | pass@1 | ✅ Published |
| 2 | Mixtral 8x7B compacted | LLM forge | PPL 8.97 | ✅ Published |
| 3 | Mixtral 8x22B compacted | LLM forge | PPL TBD | 🔄 Forging |
| **4** | **open-eyes triage vs CDnet** | **CV eval** | **F1 TBD** | **⬜ Spec ready** |
| **5** | **open-eyes flow vs Middlebury** | **CV eval** | **EPE TBD** | **⬜ Spec ready** |
| **6** | **open-eyes events vs UCF-Crime** | **CV eval** | **temporal-IoU TBD** | **⬜ Spec ready** |
| **7** | **open-eyes multicam vs EPFL** | **CV eval** | **MOTA TBD** | **⬜ Spec ready** |

| **8** | **joels-house daily eval** | **Deployment** | **FP rate TBD** | **⬜ Spec ready** |

Same table, same methodology, same attestation. Forge-alloy doesn't know the difference between an LLM, a CV pipeline, or a camera deployment. It runs stages, measures results, attests the chain. That's the whole point.

See also: [FORGE-ALLOY-AS-DEPLOYMENT-CONTRACT.md](FORGE-ALLOY-AS-DEPLOYMENT-CONTRACT.md) — the full deployment alloy pattern.
