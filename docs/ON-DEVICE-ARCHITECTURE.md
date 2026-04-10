# On-Device Architecture — The Camera Agent

**Status**: Architecture. Ready for hardware validation this weekend.

The open-eyes agent is a single Rust binary (~2-5MB) that runs on the camera's ARM SoC. It does **triage** — decides what's worth sending to the grid. Every frame that doesn't leave the camera is bandwidth saved, grid cycles saved, storage saved.

---

## The Hardware Reality

| Spec | Budget camera ($20) | Mid-range ($40) | NPU camera ($30-50) |
|---|---|---|---|
| **SoC** | Hi3516Ev200, SSD202 | Hi3516Cv500 | T31, T40, SSC338Q |
| **CPU** | ARM Cortex-A7, 500MHz-1GHz | ARM Cortex-A7, 1.2GHz | MIPS/ARM, 1GHz+ |
| **RAM** | 32-64MB | 64-128MB | 64-256MB |
| **NPU** | No | No | Yes (0.5-2 TOPS) |
| **Flash** | 8-16MB (NOR) | 16-32MB | 16-32MB |
| **H.264** | Hardware encoder (free) | Hardware H.264+H.265 | Hardware H.264+H.265 |
| **ISP** | Yes (always running) | Yes | Yes |
| **WiFi** | 2.4GHz | 2.4/5GHz | 2.4/5GHz |
| **Mic** | Usually yes | Yes | Yes |
| **PIR** | Some models | Some models | Some models |
| **IR LEDs** | Yes (night vision) | Yes | Yes |
| **Power** | USB 5V/1A (wall) or battery | USB/PoE | USB/PoE |

**Our compute budget:** ~15ms per frame at 30fps = 50% CPU utilization, leaving headroom for OS + WiFi + RTSP. On a 500MHz A7 that's ~7.5 million cycles per frame. Plenty for the triage pipeline.

---

## Two Operating Modes

The agent detects which mode to run based on hardware (PIR sensor present = burst mode).

### Continuous Mode (wall-powered, no PIR)

The camera is always on. The agent runs the full triage pipeline at frame rate. Most frames are discarded locally — only events and interesting frames reach the grid.

```
ISP captures frame (hardware, always running)
    │
    ├─ [0.1ms] Light level — average intensity of quarter-res
    │   └─ Dark + no IR → skip everything, camera is blind
    │
    ├─ [1ms] Background model — running average, pixel diff, threshold
    │   └─ Nothing changed → skip flow, emit nothing, sleep until next frame
    │
    ├─ [5ms] Optical flow at quarter-res — the CBAR heartbeat
    │   ├─ Low magnitude → static, no event
    │   ├─ High magnitude → MOTION event to grid
    │   ├─ Global shift → DRIFT event (camera bumped/moved)
    │   └─ Directional → quadrant + direction in event payload
    │
    ├─ [3ms] Edge density — Sobel on quarter-res, count edge pixels
    │   └─ Flow static BUT edges changed → PRESENCE event
    │       (catches stationary people that flow missed)
    │
    ├─ [1ms] Scene hash — pHash of quarter-res
    │   └─ Sudden large change → SCENE_CHANGE event
    │       (door opened, lights on/off, car pulled into frame)
    │
    ├─ [5ms] Audio classifier (if mic present, parallel with video)
    │   └─ FFT on 1-second window → {glass_break, scream, bark, doorbell, car_alarm, silence}
    │       URGENT events bypass all other logic → immediate grid alert
    │
    └─ [20ms on NPU, parallel] Person detector (if NPU present)
        └─ Binary classifier at 96x96: person / not-person
            PERSON event → grid prioritizes this camera's RTSP stream

Total CPU time per frame: 10-15ms (without NPU)
NPU runs in parallel: +0ms CPU cost
Audio runs on its own cadence: +0ms video cost
```

### Burst Mode (battery-powered, PIR present)

The camera sleeps. PIR (passive infrared) sensor watches for warm bodies using microamps. When PIR triggers, the SoC wakes, captures a burst, our agent triages, then sleeps.

```
DEEP SLEEP (99.9% of time)
    ├─ PIR sensor active (hardware, ~10μA)
    ├─ SoC powered off
    ├─ WiFi off
    ├─ Agent not running
    └─ Power draw: ~0.01W
        │
        PIR triggers (warm body detected)
        │
WAKE SEQUENCE (~2-3 seconds)
    ├─ SoC boots from flash (~1s)
    ├─ Agent starts, loads background reference from flash
    ├─ WiFi reconnects (~1-2s)
    └─ Camera ISP starts capturing
        │
BURST CAPTURE (5-10 seconds)
    ├─ Same triage pipeline as continuous mode
    ├─ But also: compare against stored background (was this a real event?)
    ├─ PIR false positive rate is ~20-30% (wind, sun heating, animals)
    ├─ If false positive: log it, update background, go back to sleep
    ├─ If real: emit events, stream H.264 burst to grid
    └─ Store new background reference to flash
        │
DEEP SLEEP again
    └─ Power draw during burst: ~1.5W for 5-10s
        On a 5Wh battery: ~100,000 burst events before dead
        At 10 events/day: ~27 years (battery self-discharge kills it first)
```

---

## The Agent Binary

```rust
// crates/open-eyes-camera/src/agent/

/// On-camera agent — the single Rust binary that runs on the SoC.
/// Cross-compiled for ARM (armv7-unknown-linux-gnueabihf)
/// or MIPS (mipsel-unknown-linux-gnu).

struct Agent {
    mode: OperatingMode,          // Continuous or Burst
    camera: V4L2Source,           // ISP frame capture
    rtsp: RtspServer,             // H.264 stream to grid (on-demand)
    transport: GridTransport,     // Tailscale or direct LAN
    triage: TriagePipeline,       // The decision tree
    config: AgentConfig,          // Loaded from /etc/openeyes.conf
}

enum OperatingMode {
    /// Wall-powered, always on. Full triage pipeline at frame rate.
    Continuous,
    /// Battery, PIR wake. Triage during burst, sleep between.
    Burst { pir_gpio: u32 },
}
```

### Triage Pipeline

```rust
/// The triage pipeline — decides what's worth sending to the grid.
/// Runs entirely on the camera's ARM CPU (+ NPU if available).
/// Output is EVENTS (geometry), not frames (pixels).

struct TriagePipeline {
    background: BackgroundModel,
    flow: QuarterResFlow,
    edge_density: EdgeDensityTracker,
    scene_hash: PerceptualHash,
    audio: Option<AudioClassifier>,
    person_detector: Option<NpuDetector>,

    // Thresholds — adaptive based on light level and time of day
    motion_threshold: f32,
    edge_threshold: f32,
    scene_hash_threshold: u32,
}

/// What the triage pipeline emits. Pure geometry. No pixels.
enum TriageEvent {
    /// Something moved. Magnitude + direction + quadrant.
    Motion {
        magnitude: f32,
        direction: (f32, f32),  // normalized flow vector
        quadrant: u8,           // which part of frame (0-3)
    },
    /// Camera was bumped or moved. Recalibration needed.
    Drift {
        shift_pixels: f32,
        direction: (f32, f32),
    },
    /// Flow is static but edge density changed. Someone standing still.
    Presence {
        region: (f32, f32, f32, f32),  // bounding box in normalized coords
    },
    /// Major scene change (lights, door, vehicle entering frame).
    SceneChange {
        hash_distance: u32,
        light_delta: f32,
    },
    /// Audio event detected.
    Audio {
        class: AudioClass,
        confidence: f32,
    },
    /// Person detected by NPU.
    Person {
        bbox: (f32, f32, f32, f32),  // normalized coords
        confidence: f32,
    },
    /// Heartbeat — camera is alive, scene is static.
    Heartbeat {
        uptime_s: u64,
        temperature_c: f32,
        wifi_rssi: i8,
        light_level: f32,
    },
}

enum AudioClass {
    Silence,
    GlassBreak,
    Scream,
    DogBark,
    Doorbell,
    CarAlarm,
    Gunshot,
    Speech,
    Unknown,
}
```

---

## Background Model

The foundation of the triage pipeline. If nothing changed, nothing else runs.

```rust
/// Simple running-average background model.
/// Maintains a reference frame (quarter-res grayscale).
/// Each new frame: diff against reference, threshold, count changed pixels.
/// If changed_pixels < threshold: scene is static, skip everything.
///
/// Adapts slowly to gradual changes (sunlight moving, shadows shifting)
/// but triggers on sudden changes (person entering frame).
///
/// Memory: 1 quarter-res frame = 160x120 bytes = ~19KB.
/// Compute: subtract + threshold = ~0.5ms at quarter-res.

struct BackgroundModel {
    reference: Vec<u8>,          // quarter-res grayscale
    width: u32,
    height: u32,
    alpha: f32,                  // learning rate (0.001 = slow adapt)
    change_threshold: u8,        // per-pixel diff threshold
    area_threshold: f32,         // fraction of pixels that must change
    last_update: u64,            // frame index of last reference update
}

impl BackgroundModel {
    /// Returns the fraction of pixels that changed (0.0 = identical, 1.0 = everything).
    fn diff(&self, frame: &[u8]) -> f32 { /* subtract, threshold, count */ }

    /// Blend current frame into reference (slow adaptation).
    fn update(&mut self, frame: &[u8]) { /* reference = alpha * frame + (1-alpha) * reference */ }

    /// Hard-set reference (after confirmed static period or on boot).
    fn set_reference(&mut self, frame: &[u8]) { /* copy */ }
}
```

**Why not just use optical flow?** Flow costs 5ms. Background diff costs 0.5ms. On a quiet night, background diff rejects 99.9% of frames before flow ever runs. That's 4.5ms saved per frame × 30fps = 135ms/s of CPU time not wasted. On a 500MHz ARM that matters.

---

## Optical Flow (Quarter-Res)

The CBAR heartbeat, ported to bare ARM. Runs at quarter resolution (160x120 from 640x480, or 320x240 from 1280x720).

```rust
/// Lucas-Kanade sparse optical flow at quarter resolution.
/// Tracks ~50-100 feature points frame-to-frame.
///
/// Why sparse, not dense?
/// - Dense Farneback at 160x120 = ~8ms on ARM A7
/// - Sparse LK tracking 100 points = ~3ms on ARM A7
/// - We don't need per-pixel flow. We need: "did something move? which direction? how fast?"
/// - Sparse points answer those questions at half the cost.

struct QuarterResFlow {
    prev_gray: Vec<u8>,          // previous frame, quarter-res
    prev_points: Vec<(f32, f32)>, // tracked points in previous frame
    flow_vectors: Vec<(f32, f32, f32, f32)>, // (x, y, dx, dy) per tracked point
    magnitude: f32,              // 75th percentile flow magnitude
    dominant_direction: (f32, f32),
    grid_interval: u32,          // re-detect features every N frames
}
```

**Key detail from CBAR:** the flow runs at quarter-res but reports in full-res coordinates (multiply by 4). The grid node receives motion events in the camera's native resolution coordinate system. No coordinate transform confusion downstream.

---

## Edge Density

Catches what flow misses — stationary people. Someone standing in frame produces zero optical flow but their silhouette changes the edge density of the region they occupy.

```rust
/// Sobel edge detection at quarter-res, tracked as a density grid.
/// Divide the quarter-res frame into an 8x6 grid (20x20 pixel cells).
/// Count edge pixels per cell. Compare against running average.
/// If a cell's edge density changes significantly but flow is low → presence detected.

struct EdgeDensityTracker {
    grid: [[f32; 8]; 6],           // running average edge density per cell
    alpha: f32,                    // learning rate
    change_threshold: f32,         // how much change triggers an event
}
```

---

## Audio Classifier

Many cheap cameras have a built-in mic. Use it. Audio detects events cameras can't see — glass breaking behind a wall, someone screaming in another room, a doorbell.

```rust
/// Tiny audio classifier running on 1-second FFT windows.
/// Input: 8kHz mono PCM from the camera's mic (ALSA / V4L2 audio).
/// Pipeline: 1s window → FFT (256-point) → mel filterbank (40 bins) → classifier.
///
/// The classifier is a tiny fully-connected network (~50KB):
/// Input: 40 mel bins × 8 time steps = 320 features
/// Hidden: 64 neurons
/// Output: 9 classes (silence, glass_break, scream, bark, doorbell, car_alarm, gunshot, speech, unknown)
///
/// Trained on ESC-50 + AudioSet subsets, quantized to int8.
/// Runs in ~2ms on ARM A7 for inference.
/// The FFT is the expensive part (~3ms) but runs on its own thread at 1Hz.

struct AudioClassifier {
    mel_filterbank: Vec<Vec<f32>>,  // precomputed filterbank weights
    model_weights: Vec<u8>,         // quantized int8 model (~50KB)
    ring_buffer: Vec<i16>,          // 1 second of 8kHz PCM = 16KB
    write_pos: usize,
}
```

**Why this matters for security:** a camera pointed at a door can't see someone breaking a window on the other side of the house. But if that camera has a mic, it hears the glass break. Audio events are URGENT — they bypass the normal triage and alert the grid immediately.

---

## NPU Person Detector (cameras with neural processing unit)

For T31/T40/SSC338Q cameras with a small NPU (0.5-2 TOPS). NOT a full YOLO model. A binary "person-shaped blob" classifier at 96x96.

```rust
/// Tiny person detector for on-camera NPU.
/// Input: 96x96 RGB crop (downscaled from camera frame)
/// Output: single float — P(person in frame)
///
/// Architecture: MobileNet-v2 stem (3 depthwise-separable blocks) + global avg pool + sigmoid
/// Size: ~100KB quantized int8
/// Inference: ~20ms on T31 NPU (runs in parallel with CPU triage)
///
/// This is NOT object detection. No bounding box. Just: "is there a person? yes/no."
/// If yes, the grid node runs the full YOLO detector on the RTSP stream.
/// The camera did the cheap filter, the grid does the expensive understanding.

struct NpuDetector {
    model: NpuModel,  // loaded into NPU memory at boot
    input_size: (u32, u32),  // 96x96
    threshold: f32,   // P(person) above this → emit event
}
```

**Forged models.** The person detector model is produced by the same forge-alloy pipeline that compacts LLMs. A MobileNet-v2 pruned to 100KB via the same calibration-aware methodology. The model ships with the firmware image. OTA updates can push improved models over the grid mesh.

---

## RTSP Streaming Strategy

The camera's H.264 encoder runs in hardware at zero CPU cost. But we don't stream 24/7 — that wastes bandwidth and grid storage. The agent controls when RTSP flows.

```
RTSP states:

IDLE (default)
    H.264 encoder is running (hardware, free) but output goes nowhere.
    Agent triage runs on ISP frames, not on encoded H.264.
    Bandwidth: 0.
    │
    ├─ Motion/Presence/Person event triggers:
    │
STREAMING (event-driven)
    RTSP stream flows to grid node.
    Grid records + analyzes.
    Duration: event + 10 second tail (configurable).
    Bandwidth: 1-4 Mbps (720p H.264).
    │
    ├─ 10 seconds of silence after last event:
    │
IDLE again

ON-DEMAND (grid requests)
    Grid node says "start streaming camera 3" (user opened dashboard,
    or AI persona wants to look).
    Streams until grid says stop.
    │
    This mode exists so you can always pull up a live view.
    But the DEFAULT is idle — no streaming unless triggered.
```

**Pre-event buffer.** The agent maintains a 5-second circular buffer of H.264 keyframes in RAM (~2-5MB depending on resolution). When an event triggers streaming, the pre-event buffer is sent first. The grid gets video from BEFORE the motion started. This is how Ring/Wyze do "5 seconds before motion" — we do the same, on-device, no cloud.

---

## Grid Communication

The agent talks to the grid node via a simple event protocol over the Tailscale mesh (or direct LAN if no Tailscale).

```rust
/// Events sent from camera to grid node.
/// Serialized as compact binary (not JSON — every byte counts on WiFi).
///
/// Header: [magic: u16] [camera_id: u16] [timestamp_ms: u64] [event_type: u8] [payload_len: u16]
/// Payload: event-specific binary data
///
/// Total overhead per event: 13 bytes header + payload.
/// A motion event: 13 + 12 = 25 bytes.
/// At 10 events/second during activity: 250 bytes/second.
/// The WiFi radio doesn't even notice.

struct EventWireFormat {
    magic: u16,        // 0x0E01 ("OE" + version 1)
    camera_id: u16,
    timestamp_ms: u64, // monotonic milliseconds since boot
    event_type: u8,
    payload_len: u16,
    // payload follows
}
```

**Grid node commands back to camera:**

| Command | What it does |
|---|---|
| `START_STREAM` | Start RTSP (include pre-event buffer) |
| `STOP_STREAM` | Stop RTSP (return to idle) |
| `SET_THRESHOLD` | Adjust motion/audio thresholds remotely |
| `RECALIBRATE` | Force background model reset |
| `REBOOT` | Reboot camera (firmware update applied) |
| `HEALTH_CHECK` | Request immediate heartbeat |

---

## Memory Budget (32MB camera)

| Component | RAM | Notes |
|---|---|---|
| Linux kernel + busybox | ~4MB | OpenIPC minimal |
| ISP driver + buffers | ~8MB | Camera sensor DMA buffers |
| H.264 encoder buffers | ~4MB | Hardware encoder working memory |
| WiFi driver | ~1MB | |
| **open-eyes agent** | **~6MB** | See breakdown below |
| Tailscale | ~4MB | WireGuard state + routing |
| Headroom | ~5MB | Safety margin |
| **Total** | **~32MB** | Fits in budget |

**Agent memory breakdown (6MB):**

| Component | RAM | Notes |
|---|---|---|
| Binary code + static data | ~2MB | The Rust binary in memory |
| Background model (quarter-res) | ~19KB | 160x120 grayscale |
| Flow state (points + vectors) | ~8KB | 100 tracked points |
| Edge density grid | ~200B | 8x6 float grid |
| Audio ring buffer | ~16KB | 1 second at 8kHz |
| Audio classifier model | ~50KB | Quantized int8 |
| Pre-event H.264 buffer | ~3MB | 5 seconds of keyframes |
| Scene hash history | ~1KB | Last 10 hashes |
| Event send buffer | ~4KB | Outbound event queue |
| Stack + heap overhead | ~500KB | Rust allocator |
| **Total** | **~6MB** | |

**For 64MB cameras:** double the pre-event buffer to 10 seconds, add the NPU model (~100KB), more headroom.

---

## Power Budget

### Wall-Powered (continuous mode)

| Component | Power | Duty cycle | Average |
|---|---|---|---|
| SoC baseline (ISP + encoder) | 0.8W | 100% | 0.8W |
| WiFi radio | 0.3W | 100% | 0.3W |
| IR LEDs (night) | 0.5W | ~50% (night hours) | 0.25W |
| **ARM CPU (our agent)** | **0.3W** | **~30%** (skips static frames) | **0.1W** |
| Tailscale crypto | 0.05W | ~1% (event bursts) | ~0.001W |
| **Total** | | | **~1.45W** |

Our agent adds ~0.1W average to a camera that already draws 1.35W. The USB adapter provides 5W. We're at 29% of available power. Thermal is not a concern.

### Battery (burst mode)

| State | Power | Duration | Energy per event |
|---|---|---|---|
| Deep sleep (PIR only) | 0.01W | 99.9% of time | — |
| Wake + connect | 1.5W | ~3 seconds | 4.5 mWh |
| Triage + stream | 1.5W | ~7 seconds | 10.5 mWh |
| **Total per event** | | **~10 seconds** | **~15 mWh** |

On a 5Wh battery: ~333 events before dead. At 10 events/day: ~33 days.
With a 10Wh battery (common in battery cams): ~66 days.

For comparison, Ring Stick Up Cam battery lasts ~30-60 days with similar event rates. We're in the same ballpark with more on-device intelligence.

---

## Build & Deploy

```bash
# Cross-compile for ARM (most cameras)
rustup target add armv7-unknown-linux-gnueabihf
cargo build --target armv7-unknown-linux-gnueabihf --release \
    -p open-eyes-camera --features on-device
# Binary: target/armv7-unknown-linux-gnueabihf/release/open-eyes-agent (~2-5MB)

# Cross-compile for MIPS (Ingenic T31/T40 cameras)
rustup target add mipsel-unknown-linux-gnu
cargo build --target mipsel-unknown-linux-gnu --release \
    -p open-eyes-camera --features on-device

# Package into firmware image (OpenIPC base + our agent)
./scripts/build-firmware.sh --soc hi3516ev200 --agent-features "audio,flow"
# Output: open-eyes-hi3516ev200.bin (flash to SD card)

# Flash to SD card
dd if=open-eyes-hi3516ev200.bin of=/dev/sdX bs=1M
# Insert SD card, needle-reset camera, done.
```

**Feature flags.** The agent binary is compiled with feature flags for optional capabilities:
- `audio` — include audio classifier (adds ~50KB to binary)
- `npu-t31` — include T31 NPU person detector
- `npu-ssc338` — include SSC338Q NPU support
- `flow` — include optical flow (on by default)
- `tailscale` — include Tailscale transport (vs direct LAN only)

This keeps the binary small for cameras with limited flash. A minimal build (flow only, direct LAN) is ~2MB. A full build (flow + audio + NPU + tailscale) is ~5MB.

---

## Weekend Hardware Plan

### What we need

1. **One Virtavo egg camera** — pop the shell, read the SoC markings
2. **USB-UART adapter** — $5, connects to the camera's UART pads for console access
3. **Micro SD card** — for firmware flash
4. **The SoC identification** — determines which OpenIPC image to start from

### The sequence

```
Saturday morning:
  1. Open camera, photograph board, identify SoC
  2. Check OpenIPC compatibility for that SoC
  3. Flash OpenIPC via SD card
  4. SSH in, verify Linux boots, camera sensor works
  5. Cross-compile open-eyes agent for the right ARM target
  6. scp the binary to camera, run it
  7. Verify: agent starts, captures frames via V4L2, computes flow

Saturday afternoon:
  8. Background model working — verify static frames are rejected
  9. Wave hand in front of camera — verify motion events emit
  10. Connect to grid node — verify events arrive
  11. RTSP streaming on-demand — verify grid can pull video

Sunday:
  12. Pre-event buffer — verify "5 seconds before motion" works
  13. Audio classifier (if mic exists) — glass break test
  14. Second camera — verify multi-camera discovery
  15. Phone app (Flutter) — verify camera appears in setup scan
```

### What success looks like

A $20 camera on the shelf, running our firmware, detecting motion via optical flow, sending events to a grid node on the local network, streaming H.264 on demand, consuming 6MB of RAM and 0.1W of power. No cloud. No subscription. No app pairing. Just a camera that understands its scene and talks to your grid.

---

## Key Design Decisions

### Why quarter-res for everything?

640x480 → 160x120. That's 16× fewer pixels. Every operation (diff, flow, Sobel, hash) runs 16× faster. On a 500MHz ARM, this is the difference between "runs at 30fps" and "runs at 2fps."

The grid node gets the full-resolution H.264 stream when it needs it. The camera's triage doesn't need full resolution — "something moved in the upper-left quadrant" is the same answer at 160x120 as at 1920x1080.

### Why background model before flow?

Background diff costs 0.5ms. Flow costs 5ms. On a quiet night, 99.9% of frames have zero change. Background diff rejects them in 0.5ms. Without it, flow wastes 5ms per frame × 30fps = 150ms/second of CPU for nothing.

The background model is the bouncer at the door. Flow is the detective inside. Don't let the detective look at every person walking past on the sidewalk.

### Why sparse flow, not dense?

Dense Farneback at 160x120 ≈ 8ms on ARM A7. Sparse Lucas-Kanade tracking 100 points ≈ 3ms. We need "did something move, where, how fast" — not per-pixel displacement fields. Sparse answers the question at 60% of the cost.

### Why not just run YOLO on-device?

Even a tiny YOLO-nano at 160x160 takes ~200ms on an ARM A7 without NPU. That's 5fps and 100% CPU. The camera becomes a space heater that can barely keep up with real-time. Our triage pipeline does 90% of YOLO's job (detecting that something interesting is happening) at 3% of the cost, and lets the grid node run the real YOLO on the 1% of frames that matter.

On NPU cameras (T31, SSC338Q), we CAN run a tiny binary classifier at 96x96 because the NPU handles it in parallel with zero CPU cost. But it's still not YOLO — it's "person or not?" The grid does the rest.

### Why not stream 24/7 and let the grid handle everything?

720p H.264 at 2Mbps = 900MB/hour = 21GB/day per camera. With 8 cameras that's 168GB/day of storage and continuous WiFi traffic. The grid node needs to decode and analyze all of it. This is what Ring/Nest do, except they ship it to the cloud and charge you monthly for the storage.

Our approach: the camera sends ~1KB/hour during quiet periods (heartbeats). During activity, it streams H.264 for the duration of the event plus a 10-second tail. Average bandwidth per camera: maybe 50MB/day instead of 21GB/day. That's a **420× reduction** in bandwidth and storage.

The intelligence is distributed, not centralized. The camera is smart enough to know what's boring.
