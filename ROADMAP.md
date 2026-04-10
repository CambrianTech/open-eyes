# open-eyes Roadmap

**The path from "algorithms work on synthetic data" to "cameras running on your property."**

Each phase builds on the last. No skipping. VDD validation at every stage.

---

## Phase 0: Core Pipeline (DONE)

**Status: Complete.** 92 tests, all green.

- [x] Frame lazy-eval (OnceLock pattern from CBAR)
- [x] OpenCV integration (ORB, Canny, Farneback, Lucas-Kanade, background diff)
- [x] MotionDetectorNode (normalized flow, resolution-independent)
- [x] BackgroundSubNode (running average, adaptive alpha)
- [x] EdgeDensityNode (8x6 grid, catches stationary presence)
- [x] FFI crate (extern "C", cbindgen header)
- [x] Platform bindings (Swift/Kotlin/Flutter/Python)
- [x] VDD eval harness (`oe-eval --dataset synthetic`)
- [x] Synthetic test sequences with ground truth
- [x] GPU compute module (lazy texture handles, architecture only)

---

## Phase 1: Real Video (NEXT)

**Goal: Pipeline works on real surveillance footage, not just synthetic rectangles.**

- [ ] Download CAVIAR / UCF-Crime / CDnet datasets
- [ ] Video decoder (ffmpeg-next) — MP4/AVI → frame iterator
- [ ] `oe-eval --dataset cdnet` produces real F1 scores against ground truth masks
- [ ] `oe-eval --dataset ucf-crime` produces temporal-IoU against annotated events
- [ ] Tune node parameters on real data (thresholds, alpha, grid size)
- [ ] VDD: F1 > 0.85 on CDnet baseline, temporal-IoU > 0.70 on UCF-Crime subset
- [ ] Benchmark: latency per frame on Mac (target: < 15ms at 720p)

**Key deliverable:** `oe-eval` runs on real surveillance video and produces attested metrics.

---

## Phase 2: Camera Input

**Goal: Live camera feed → pipeline → events. Phone camera first, RTSP second.**

- [ ] Phone camera source (AVFoundation iOS, CameraX Android) via FFI
- [ ] Flutter app: camera preview + live motion magnitude readout
- [ ] RTSP client (ffmpeg-next) — connect to IP camera streams
- [ ] `CameraSource` trait implemented for phone + RTSP
- [ ] Multi-camera: 2+ sources feeding the same pipeline simultaneously
- [ ] VDD: phone pointed at a room, person walks in → motion event within 500ms

**Key deliverable:** Point your phone at a room. See motion events in real time.

---

## Phase 3: On-Device Agent

**Goal: Rust binary running on a $20 camera's ARM SoC.**

- [ ] Cross-compile for ARM (armv7-unknown-linux-gnueabihf)
- [ ] V4L2 frame capture (ISP direct)
- [ ] Quarter-res triage pipeline (background → flow → edge density)
- [ ] Event emission to grid node (binary protocol over TCP/Tailscale)
- [ ] RTSP server (H.264 hardware encode, on-demand streaming)
- [ ] Pre-event buffer (5 seconds of H.264 keyframes in RAM)
- [ ] Power state reporting (battery ADC, solar GPIO, temperature)
- [ ] Triage tier management (Hibernate → Minimal → Standard → Full → Maximum)
- [ ] OpenIPC firmware image builder (kernel + agent + tailscale)
- [ ] SD card flash onboarding (write image, insert, needle-reset, done)
- [ ] VDD: cross-compiled binary, runs on Raspberry Pi, detects motion from USB camera

**Key deliverable:** Flash an SD card, put it in a camera, it joins your network and detects motion.

---

## Phase 4: Continuum Grid Integration

**Goal: Cameras are grid nodes. Foreman orchestrates. Personas analyze.**

### 4a: Grid Protocol
- [ ] `open-eyes-grid` crate — grid node registration, event routing
- [ ] Camera events → continuum Events system (`Events.emit('camera:motion', ...)`)
- [ ] Grid node discovery (mDNS/SSDP + Tailscale)
- [ ] Foreman camera management commands (`camera/register`, `camera/status`, `camera/stream`)
- [ ] Coverage map (zone geometry → camera FOV cones → dead zone detection)

### 4b: Commands Integration
- [ ] `open-eyes/scene/view` — get current scene state as geometry
- [ ] `open-eyes/camera/list` — list all cameras with status
- [ ] `open-eyes/camera/stream` — start/stop RTSP from a specific camera
- [ ] `open-eyes/events/subscribe` — subscribe to camera events
- [ ] `open-eyes/detect/track` — get entity tracks across cameras
- [ ] CommandGenerator specs for all commands (discoverable, self-documenting)

### 4c: Persona Security Team
- [ ] SecurityAnalyst persona — watches camera events, assesses threats
- [ ] Threat assessment via RAG (event history + zone rules + time of day)
- [ ] Alert routing — which events notify the user vs log quietly
- [ ] Natural language queries: "what happened at the back door last night?"
- [ ] Camera-aware reasoning: "camera 3 is offline, coverage gap at garage"

### 4d: Docker Deployment
- [ ] `docker-compose.yml` for grid node with open-eyes
- [ ] GPU passthrough for detection models
- [ ] Volume mounts for recorded footage + scene state
- [ ] Multi-node: separate containers for camera processing vs AI reasoning
- [ ] Health checks (camera connectivity, pipeline latency, storage)

**Key deliverable:** `docker compose up` starts a grid node that manages your cameras, detects motion, tracks entities, and alerts you via continuum chat.

---

## Phase 5: Multi-Camera Fusion

**Goal: N cameras → unified 3D scene → cross-camera entity tracking.**

- [ ] Camera registration (phone AR scan locates cameras in 3D)
- [ ] Cross-camera feature matching (ORB across views)
- [ ] Entity re-identification (same person tracked across camera boundaries)
- [ ] Unified 3D entity positions (triangulate from multiple views)
- [ ] Trajectory prediction (where is this entity heading?)
- [ ] Zone crossing detection (entity moved from driveway to back door)
- [ ] VDD: EPFL multi-camera dataset, MOTA > 0.60

**Key deliverable:** "A person walked from the driveway (cam 2) to the back door (cam 5), paused for 30s, then left via the side yard (cam 3)."

---

## Phase 6: Gaussian Splats

**Goal: Navigable 3D model of your property, updated in real time.**

- [ ] `open-eyes-splat` crate — wgpu-based splat renderer
- [ ] Incremental splat updates from camera frames
- [ ] Navigate the 3D scene from any viewpoint (fly-through)
- [ ] Entity visualization in the splat (tracked people/vehicles as markers)
- [ ] Camera FOV visualization (see what each camera sees)
- [ ] Timeline scrubbing (rewind the 3D scene to any point in history)
- [ ] Flutter integration (render splat view in Texture widget)
- [ ] VDD: splat renders at 30fps on Apple Silicon, entities correctly placed in 3D

**Key deliverable:** Open the app, fly through a 3D model of your house, see where everyone is.

---

## Phase 7: Detection Models

**Goal: Forged detection models running on-device and on grid nodes.**

- [ ] Person/vehicle/animal detection (forged YOLO variant via sentinel-ai)
- [ ] Tiny binary classifier for NPU cameras (96x96, person yes/no, 100KB)
- [ ] Audio classifier (glass break, scream, doorbell, bark — 50KB int8)
- [ ] License plate recognition (grid node, on-demand)
- [ ] Face recognition (opt-in, local only, for "known vs unknown" entity classification)
- [ ] Model forge alloys (same pipeline as LLM forges)
- [ ] VDD: mAP > 0.70 on COCO-person, inference < 20ms on T31 NPU

**Key deliverable:** Camera knows it's a person (not a cat), grid knows it's your neighbor (not a stranger).

---

## Phase 8: Mixed Reality

**Goal: AR/VR integration — see your security system in 3D space.**

- [ ] Vision Pro passthrough — cameras overlaid in your field of view
- [ ] Meta Quest passthrough — same for Quest headsets
- [ ] AR annotations — entity labels, trajectories, threat levels overlaid on real world
- [ ] Immersive monitoring — "step into" the 3D scene from your desk
- [ ] Continuum personas visible in the security view
- [ ] CBAR-style AR compositing (from cb-mobile-sdk lineage)

**Key deliverable:** Put on Vision Pro, see your property with AI annotations overlaid on the real world.

---

## Phase 9: Community Mesh

**Goal: Neighbors link their open-eyes installations into a community watch.**

- [ ] Opt-in mesh sharing (explicit consent per zone per neighbor)
- [ ] Cross-property entity tracking (vehicle seen at house A appears at house B)
- [ ] Community alerts (suspicious vehicle circling the block)
- [ ] Privacy architecture (what's shared vs private, zone-level granularity)
- [ ] Governance (community votes on sharing rules, no central authority)

**Key deliverable:** Your neighborhood has better coverage than Ring + Nextdoor combined, with zero cloud dependency.

---

## Principles (non-negotiable across all phases)

1. **Forge-alloy is the deployment contract.** One alloy describes the whole deployment: cameras, zones, thresholds, models, quality gates. Single source of truth, attestable, portable. See [docs/FORGE-ALLOY-AS-DEPLOYMENT-CONTRACT.md](docs/FORGE-ALLOY-AS-DEPLOYMENT-CONTRACT.md).
2. **All compute in Rust.** Platform bindings are thin wrappers. No scripting languages in the hot path.
2. **Never rasterize.** Geometry crosses every boundary. Pixels stay on GPU.
3. **Normalize everything.** 0-1 space. No pixel counts. No hardcoded resolutions.
4. **VDD every algorithm.** Known input → known correct output → validate within tolerance.
5. **Forge-alloy attestation.** Every eval result is cryptographically verifiable.
6. **No Euler angles.** Rotation matrices and quaternions only.
7. **Power-aware.** Every algorithm has a power budget. Triage tiers adapt.
8. **No cloud.** Everything runs on your hardware. No subscriptions. No data leaves your network.
9. **OpenCV for CV.** Don't hand-roll what 25 years of optimization already solved.
10. **Event-driven.** Cameras emit events. The grid orchestrates. Humans get notified when needed.
