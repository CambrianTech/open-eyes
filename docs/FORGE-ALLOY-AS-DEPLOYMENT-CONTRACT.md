# Forge-Alloy as Deployment Contract

**Status**: Architecture. The unifying insight.

forge-alloy isn't just for model forging. It's the universal contract for any compute pipeline that needs configuration, evaluation, and attestation. Camera deployments, CV pipeline configs, model forges, eval runs — all the same shape.

---

## The Shape

Every alloy is: **source → stages → eval → results → attestation.**

| | Model Forge | CV Eval | Camera Deployment |
|---|---|---|---|
| **Source** | Mixtral-8x7B | open-eyes-core v0.1 | 4 cameras at Joel's house |
| **Stages** | profile → prune → quant | ingest → pipeline → compare | configure → calibrate → triage |
| **Eval** | PPL 8.97 (Δ +10.2%) | F1 0.93 on CDnet | False positive rate < 5% |
| **Results** | GGUF on HuggingFace | Metrics JSON | Running deployment |
| **Attestation** | Model hash + alloy hash | Dataset hash + pipeline hash | Config hash + eval hash |

Same schema. Same factory queue. Same `verify/#hash` URL.

---

## Camera Deployment Alloy

```json
{
  "name": "joels-house-exterior",
  "version": "0.1.0",
  "description": "4-camera exterior security with solar-powered backyard",
  "source": {
    "baseModel": "open-eyes-core",
    "architecture": "triage-pipeline",
    "revision": "95448b6"
  },
  "deployment": {
    "nodeId": "joels-house",
    "grid": "ws://continuum-core:9001",
    "zones": [
      {
        "id": "exterior",
        "privacy": { "record": true, "retentionHours": 720, "shareWithMesh": false }
      },
      {
        "id": "vehicle",
        "privacy": { "record": true, "retentionHours": 168 }
      }
    ]
  },
  "cameras": [
    {
      "id": "front-door",
      "url": "rtsp://192.168.1.50:554/stream1",
      "zone": "exterior",
      "power": "wall",
      "triageTier": "full"
    },
    {
      "id": "backyard",
      "url": "rtsp://192.168.1.51:554/stream1",
      "zone": "exterior",
      "power": "solar",
      "triageTier": "adaptive"
    },
    {
      "id": "driveway",
      "url": "rtsp://192.168.1.52:554/stream1",
      "zone": "vehicle",
      "power": "wall",
      "triageTier": "maximum"
    },
    {
      "id": "side-yard",
      "url": "openeyes://192.168.1.53",
      "zone": "exterior",
      "power": "solar",
      "hasAgent": true,
      "triageTier": "adaptive"
    }
  ],
  "stages": [
    {
      "type": "cv-triage",
      "nodes": ["background-sub", "motion-detector", "edge-density", "scene-hash"],
      "config": {
        "motionThreshold": 0.005,
        "backgroundAlpha": 0.005,
        "edgeDensityThreshold": 0.03,
        "sceneHashThreshold": 0.2,
        "quarterRes": true
      },
      "notes": "All thresholds normalized 0-1. Resolution-independent."
    },
    {
      "type": "cv-eval",
      "schedule": "daily",
      "benchmarks": [
        {
          "name": "false-positive-rate",
          "metric": "rate",
          "notes": "Motion events on static scenes / total static frames"
        },
        {
          "name": "detection-latency",
          "metric": "ms-per-frame",
          "notes": "Average triage pipeline time"
        },
        {
          "name": "coverage",
          "metric": "fraction",
          "notes": "Fraction of zone area covered by at least one camera"
        }
      ],
      "acceptanceCriteria": {
        "false-positive-rate": { "max": 0.05 },
        "detection-latency": { "max": 15.0 },
        "coverage": { "min": 0.85 }
      }
    }
  ],
  "detectionModels": [
    {
      "name": "person-detector-arm",
      "alloyRef": "continuum-ai/person-detector-96x96-npu.alloy.json",
      "runOn": ["side-yard"],
      "notes": "Only on cameras with NPU agent"
    },
    {
      "name": "yolo-nano-grid",
      "alloyRef": "continuum-ai/yolo-nano-forged.alloy.json",
      "runOn": "grid",
      "notes": "Full YOLO on grid node, triggered by triage events"
    }
  ]
}
```

---

## How It Works at Runtime

### Boot
1. Foreman reads the deployment alloy
2. Connects to each camera (RTSP or agent)
3. Applies triage config per camera
4. Loads detection models from alloy references
5. Starts the triage pipeline

### Daily Eval
1. Factory queue picks up the `cv-eval` stage on schedule
2. Runs false-positive evaluation on recorded static periods
3. Measures detection latency
4. Computes coverage from camera FOVs vs zone boundaries
5. Checks acceptance criteria
6. If any metric fails → alert + optional auto-adjust thresholds
7. Results written back to alloy → attested → verifiable

### Threshold Tuning
The deployment alloy is the SINGLE SOURCE OF TRUTH for all thresholds:
- `motionThreshold: 0.005` — change it in the alloy, the pipeline picks it up
- `triageTier: adaptive` — the Foreman adjusts based on power state
- `acceptanceCriteria.false-positive-rate.max: 0.05` — the quality gate

No hunting through code for magic numbers. No different configs on different nodes.
The alloy is the config. The config is the alloy.

---

## Detection Model References

A deployment alloy REFERENCES model alloys. It doesn't contain the model — it points to it:

```
deployment.alloy.json
  └─ detectionModels[0].alloyRef: "person-detector-96x96-npu.alloy.json"
      └─ This is a SEPARATE alloy that was FORGED:
          source: MobileNet-v2
          stages: prune → quantize-int8 → eval
          results: mAP 0.72, size 100KB, inference 20ms on T31 NPU
          attestation: model hash + eval hash + pipeline hash
```

The deployment alloy says "use this model." The model alloy proves "this model was tested and meets its quality bar." Two alloys, linked by reference, independently attested.

This is the same pattern as software dependencies. Your `package.json` references `react@18.2.0`. React's npm package has its own integrity hash. Your project's lockfile links the two. forge-alloy does the same for compute artifacts.

---

## The Factory Runs Everything

BigMama's factory queue doesn't care what's inside an alloy:

```
Queue:
  Job 1: Mixtral-8x22B forge         [LLM prune + quant + eval]
  Job 2: person-detector-96x96 forge  [CV model prune + int8 + eval]
  Job 3: joels-house daily eval       [camera false-positive check]
  Job 4: open-eyes-flow-middlebury    [flow algorithm VDD]
```

Same heartbeat, same event emission, same result.json, same attestation.
The Foreman schedules. The factory executes. forge-alloy is the contract.

---

## The Unifying Insight

**forge-alloy is a high-level language for compute pipelines.**

- A recipe alloy is a PROGRAM (what to do)
- The factory is the RUNTIME (executes the program)
- The result is the OUTPUT (metrics, artifacts, deployments)
- The attestation is the PROOF (cryptographic chain of custody)

This works for:
- **LLM forging** — prune, quantize, evaluate, publish
- **CV model forging** — prune detection models for ARM NPUs
- **CV pipeline validation** — run against benchmarks, check quality gates
- **Camera deployment** — configure, calibrate, monitor quality
- **Audio model forging** — compress TTS/STT for on-device
- **Training runs** — LoRA fine-tuning with curriculum from Academy
- **Grid operations** — node provisioning, health checks, capacity planning

Every one of these is: source → stages → eval → results → attestation.
forge-alloy already has the schema. We just use it.

---

## Implementation Priority

1. **Deployment alloy schema** — add `deployment`, `cameras`, `detectionModels` fields to forge-alloy types.py
2. **`cv-triage` stage type** — reads camera config, applies thresholds
3. **Daily eval scheduling** — factory queue picks up `schedule: "daily"` stages
4. **Model references** — `alloyRef` field links deployment to model alloys
5. **Threshold hot-reload** — Foreman pushes updated alloy → pipeline picks up new thresholds without restart

---

## See Also

- [CV-EVAL-FORGE-ALLOY.md](CV-EVAL-FORGE-ALLOY.md) — CV eval as forge-alloy stages
- [CONTINUUM-INTEGRATION.md](CONTINUUM-INTEGRATION.md) — grid events, commands, Docker
- [ON-DEVICE-ARCHITECTURE.md](ON-DEVICE-ARCHITECTURE.md) — triage tiers, power management
- [ROADMAP.md](../ROADMAP.md) — 10-phase plan
- [forge-alloy types.py](https://github.com/CambrianTech/forge-alloy/blob/main/python/forge_alloy/types.py) — the schema
- [Many-Worlds abstract §V.6.6](https://github.com/CambrianTech/continuum/blob/main/docs/papers/MANY-WORLDS-ABSTRACT.md) — forge-alloy as a language
