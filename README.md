# open-eyes

### Palantir-level security that you fully own, from cameras you can afford.

N cheap cameras → unified 3D scene understanding → AI-powered threat detection and tracking → **your hardware, your data, your rules.**

<p align="center">
<a href="https://github.com/CambrianTech/continuum"><img src="https://img.shields.io/badge/Grid-continuum-00d4ff.svg" alt="Continuum Grid"/></a>
<a href="https://github.com/CambrianTech/forge-alloy"><img src="https://img.shields.io/badge/Models-forge--alloy-orange.svg" alt="Forge-Alloy"/></a>
<a href="https://www.gnu.org/licenses/agpl-3.0"><img src="https://img.shields.io/badge/License-AGPL--3.0-blue.svg" alt="AGPL-3.0"/></a>
<a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-nightly-orange.svg" alt="Rust"/></a>
</p>

---

## This Is Not Another Security Camera App

Every security product on the market — Ring, SimpliSafe, ADT, Nest — sends your video to **their cloud**, processes it on **their servers**, stores it on **their infrastructure**, and charges you **monthly** for the privilege of accessing your own data. They've been breached repeatedly. They share footage with law enforcement without warrants. They own the intelligence derived from your cameras. You rent access to your own home.

**open-eyes is the opposite.**

Your cameras. Your hardware. Your processing. Your 3D scene model. Your AI personas doing the threat analysis. Nothing leaves your network unless **you** choose to share it. No cloud. No subscription. No corporation between you and your own security.

And it's not just "YOLO on a video feed." It's **real-time 3D scene reconstruction from multiple cameras** — the same technology that powered augmented reality on an iPhone 7 at 60fps, now applied to security with the full power of the [continuum](https://github.com/CambrianTech/continuum) AI grid behind it.

## What It Does

```
Camera 1 ──┐
Camera 2 ──┤                    ┌─────────────────────────────────┐
Camera 3 ──┼── open-eyes ──────►│  Navigable 3D scene             │
Camera 4 ──┤   (your hardware)  │  Entity tracking across cameras │
  ...      │                    │  AI threat assessment            │
Camera N ──┘                    │  Wildlife/motion alerts          │
                                │  Full history, you own it        │
                                └─────────────────────────────────┘
```

| What Ring does | What open-eyes does |
|---|---|
| 2D video from one camera | **3D scene** from N cameras fused together |
| Motion detection (binary) | **Entity tracking** — person, vehicle, animal identified and followed across cameras in 3D |
| Cloud-processed | **Local-processed** on your grid node |
| $10-20/month forever | **$0/month forever** |
| Company owns your data | **You own your data** |
| One viewpoint per camera | **Any viewpoint** — navigate the 3D scene from any angle |
| "Someone was at your door" | **"A person walked from the driveway (cam 2) to the back door (cam 5), paused for 30s, then left via the side yard (cam 3)"** — full 3D trajectory, cross-camera tracked |

## The Technology

### Rust CBAR — 3D Scene Reconstruction

The core is a Rust adaptation of [CBAR](https://github.com/CambrianTech/cb-mobile-sdk) (Cambrian Augmented Reality) — a 50,000-line C++ engine that shipped real-time 3D scene understanding on iPhone 7 at 60fps. The key techniques:

- **Multi-camera pose estimation** — know where each camera is in 3D space
- **Feature tracking** — ORB/optical flow variants across frames and cameras
- **Surface normal estimation** — understand surface orientation for lighting and geometry
- **Point cloud accumulation** — build persistent 3D geometry over time
- **Temporal interpolation** — smooth the reconstruction across frames for stability

The original ran all of this at 60fps on an iPhone 7. With stationary cameras and a GPU-backed grid node, it's easier, not harder.

### Gaussian Splatting — Navigable 3D View

The accumulated 3D scene is rendered as [Gaussian splats](https://arxiv.org/abs/2308.04079) — the same technology that revolutionized 3D rendering in 2023. Instead of a flat grid of camera feeds, you get a **navigable 3D world** you can fly through from any angle. Updated in real time as cameras capture new data.

### Continuum Grid Integration

Every open-eyes installation is a **grid node** in the [continuum](https://github.com/CambrianTech/continuum) mesh:

- **Camera feeds become grid events** — any persona on the mesh can subscribe
- **The 3D scene is a shared resource** — accessible from any continuum client
- **AI personas form security teams** — the same PersonaUser architecture that powers chat personas powers security analysts
- **Detection models are forged** — via the [forge-alloy](https://github.com/CambrianTech/forge-alloy) pipeline, YOLO and tracking models are compacted to run on consumer hardware
- **Encrypted mesh transport** — Tailscale + Reticulum, same as continuum's grid

### Detection & Tracking

Not just "is there motion?" but:

- **What** — person, vehicle, animal, package, unknown object
- **Where** — 3D world position, not just 2D bounding box
- **Moving how** — velocity vector, trajectory prediction
- **Across which cameras** — unified tracking in the 3D model, not per-camera silos
- **How threatening** — AI persona team assesses context (time of day, behavior pattern, known vs unknown entity)

Detection models run locally via forged YOLO variants optimized for consumer hardware. The same [forge-alloy](https://github.com/CambrianTech/forge-alloy) pipeline that compacts frontier LLMs to run on a gaming PC compacts detection models to run on a Raspberry Pi.

## Setup — Walk Through Your Space

Setup is the app. The app is setup. Same engine, same view, different phase.

### Step 1: Walk Through (2 minutes)

Open the open-eyes app on your phone. Walk through your home. Your phone's ARKit/ARCore/CBAR SLAM builds a 3D map automatically — rooms, doors, windows, hallways, floors. No manual floor plan drawing. No measurements. Just walk.

```
You walk through your house with your phone.
The app builds this automatically:

  ┌─────────────────────────────────┐
  │          Second Floor            │
  │  ┌──────┐  ┌──────┐  ┌──────┐  │
  │  │Master│  │ Bath │  │ Kid's│  │
  │  │      ├──┤      ├──┤      │  │
  │  └──┬───┘  └──────┘  └──┬───┘  │
  │     │      Hallway       │      │
  ├─────┴────────────────────┴──────┤
  │          Ground Floor            │
  │  ┌──────┐  ┌──────┐  ┌──────┐  │
  │  │Living│  │Kitchen│  │Entry │  │
  │  │      ├──┤       ├──┤  ◉   │  │
  │  └──────┘  └──────┘  └──────┘  │
  └─────────────────────────────────┘
  ◉ = front door (auto-detected)
```

### Step 2: Find Your Cameras (30 seconds each)

Point your phone at each mounted camera. The app finds it using the same AirTag-style precision finding that Apple built — your phone's ARKit matches the camera in 3D space against your scan. **You now know exactly where each camera is, what direction it faces, and what it can see.**

The tech: your phone's rich features (ORB/visual SLAM) match against what the camera sees (its RTSP stream). The phone knows where IT is (ARKit pose). The camera is visible in the phone's view. Feature matching pins the camera's exact 3D position and orientation in your scan. **Same CBAR pipeline, just running in reverse — instead of the camera finding things in the world, the phone finds the camera.**

### Step 3: See Your Coverage (instant)

The app shows your floor plan with camera coverage overlaid — **Starlink-style**.

```
  Green  = camera covers this area
  Yellow = partial coverage (one camera, oblique angle)
  Dark   = no coverage (dead zone)
  ◉      = camera position + FOV cone

  ┌─────────────────────────────────┐
  │  ◉━━━━━━━━━━━━━━━━━━━━━━━━━━◉  │
  │  ┃██████████████████████████┃  │
  │  ┃██████████████████████████┃  │
  │  ┃██████████░░░░████████████┃  │
  │  ┃██████████░░░░████████████┃  │ ░ = dead zone
  │  ┃██████████████████████████┃  │     "Add a camera here"
  │  ◉━━━━━━━━━━━━━━━━━━━━━━━━━━┃  │
  └─────────────────────────────────┘
```

Tap a dead zone → the app suggests where to mount a camera and what it would cover. Buy one more $20 camera, mount it, point your phone at it → coverage gap fills in real time.

### Step 4: Done. You're secure.

Same app, same view, now in monitoring mode. Bird's eye, floor plan, walk-through, timeline — all the navigation modes work on the model you just built by walking through your house. **Setup and monitoring are the same tool.**

### Bonus: Complete 3D House Scan

The walk-through scan isn't just for security setup — **it's a full 3D model of your home.** This is the same scan that continuum's mixed reality layer uses:

- **AR furniture placement** — see how that couch would look before you buy it
- **Immersive continuum sessions** — your AI personas appear in YOUR actual living room via AR overlay. Helper sits on the couch next to you. Teacher stands at the whiteboard. They know where the furniture is because you already scanned it.
- **Insurance documentation** — a complete 3D record of your home's contents
- **Renovations** — measure any wall, door, window from the model without a tape measure
- **Real estate** — generate a virtual tour from the same scan

One walk-through, multiple uses. The security setup is just the first thing it's good for.

---

## Hardware

**Minimum:** one camera + one computer. That's it.

| Component | Budget Option | Better Option |
|---|---|---|
| **Cameras** | $20 Chinese wireless (Wi-Fi, RTSP) | Ring/Wyze (reversed), PoE IP cameras |
| **Processing** | Raspberry Pi 4/5 | Old laptop, mini PC |
| **GPU (optional)** | None — CPU inference | GTX 1060+, Apple Silicon |
| **Storage** | SD card, USB drive | NAS, HDD |

**Scales infinitely.** Add cameras → the 3D scene gets denser. Add processing nodes → the grid handles more cameras. Add GPU → detection runs faster and the splat view renders smoother. There is no ceiling.

### Adding a Camera

**Tier 1: Stock firmware (zero effort)**

Open the app. It scans your network and finds cameras that expose RTSP. Tap to add. Done. Works with most cheap cameras out of the box — no flashing, no firmware, no SD card. Your phone or grid node does all the processing.

Limitations: the camera still phones home to the manufacturer's cloud, no on-device triage (all frames cross WiFi), and the manufacturer could push a firmware update that breaks RTSP access.

**Tier 2: open-eyes firmware (full control)**

```
1. Write the open-eyes firmware to a micro SD card
2. Pop it into the camera
3. Needle-reset (pin in the reset hole)
4. Walk away
```

The camera boots from SD, flashes itself with OpenIPC + the open-eyes Rust agent, announces itself on your network via mDNS. No phoning home. On-device triage means only interesting frames cross WiFi. The agent reports battery level, temperature, and health to the grid. Locked-down iptables — nothing talks to the internet.

**Most users start with Tier 1.** When you care about privacy, bandwidth, or on-device intelligence, upgrade to Tier 2 camera by camera. The app works with both — it doesn't care how the frames arrive. Pull the SD card anytime and the camera reverts to stock.

Point your phone at any camera (Tier 1 or 2) to locate it in 3D space. The setup app's AR finds it via feature matching. Coverage map updates instantly.

## Architecture

```
open-eyes/
├── crates/
│   ├── open-eyes-core/     # 3D reconstruction (Rust CBAR adaptation)
│   │   ├── geometry/       # 3D math, transforms, projections, RANSAC
│   │   ├── scene/          # Navigable world model (rooms, floors, entities)
│   │   ├── features/       # ORB, optical flow, feature matching
│   │   ├── fusion/         # N-camera registration + cross-camera tracking
│   │   ├── gpu/            # Lazy-eval GPU compute (textures, not pixels)
│   │   ├── frame/          # OnceLock lazy-eval data bus (from CBAR)
│   │   ├── stitch/         # Multi-camera stitching, zones, entity trails
│   │   └── rtos/           # Async pipeline with backpressure
│   │
│   ├── open-eyes-ffi/      # C ABI boundary (extern "C", cbindgen → .h)
│   ├── open-eyes-camera/   # Camera drivers (RTSP, ONVIF, on-device agent)
│   ├── open-eyes-detect/   # Detection + tracking (forged models)
│   ├── open-eyes-grid/     # Continuum grid integration
│   └── open-eyes-splat/    # Gaussian splatting renderer (wgpu)
│
├── bindings/
│   ├── openeyes.h          # Auto-generated C header (cbindgen)
│   ├── ios/                # Swift wrapper (AVFoundation, ARKit, visionOS)
│   ├── android/            # Kotlin wrapper + JNI bridge (CameraX, ARCore, Quest)
│   └── flutter/            # Flutter plugin (UI only — Dart never sees pixels)
│
├── app/                    # Flutter app (dashboard, setup scan, scene viewer)
├── docs/                   # Architecture, firmware, target devices
└── Cargo.toml              # Workspace manifest
```

## Use Cases

### Security (the primary case)
Full property coverage from cheap cameras, with AI-powered 3D threat detection and tracking. Better than ADT at $0/month.

### Wildlife Photography
Your wife's deer. Your neighbor's coyotes. Track animals through your yard in 3D. Time-lapse their patterns. Beautiful renders from the splat view. Same tech, gentler purpose.

### Home Automation
Know which rooms are occupied (privacy-aware heat mapping), track pets, monitor elderly family members (with consent), automate lights/HVAC based on presence. The 3D scene IS the smart home sensor.

### Community Watch (opt-in)
Neighbors who choose to share can link their open-eyes nodes into a neighborhood mesh. Cross-property tracking of unknown vehicles or suspicious activity. Opt-in, community-governed, no corporate middleman.

## Privacy Architecture

- **All processing is local.** Frames never leave your network.
- **No cloud dependency.** Works offline. No subscription. No API keys.
- **You control sharing.** Opt-in to community mesh, opt-out anytime.
- **Encrypted mesh.** Grid traffic is Tailscale-encrypted.
- **AI personas are local.** Security analysis runs on your hardware.
- **Auditable.** Open source — read the code, verify no exfiltration.
- **AGPL-3.0.** Improvements stay open. No one can fork and close it.

## Relationship to Continuum

open-eyes is a **grid application** built on [continuum](https://github.com/CambrianTech/continuum). It demonstrates that the continuum grid isn't just for AI chat — it's a general-purpose distributed computing platform. Security cameras are the first non-AI-chat grid application. The same architecture handles any sensor → processing → intelligence pipeline.

| Continuum provides | open-eyes uses it for |
|---|---|
| Grid mesh (Tailscale + Reticulum) | Camera node discovery + encrypted transport |
| PersonaUser (autonomous AI agents) | Security team personas that analyze the 3D scene |
| Forge-alloy (model compaction) | Detection models forged for consumer hardware |
| Events system | Camera frame events, detection alerts, threat assessments |
| Commands system | `open-eyes/scene/view`, `open-eyes/detect/track`, etc. |
| Factory widget | Forge detection models from the UI |

## Status

**Pre-alpha — core engine functional, cameras next.**

| Module | Lines | Tests | Status |
|---|---|---|---|
| **geometry** | 250+ | 3 | ✅ Projection, triangulation, RANSAC plane fitting |
| **features** | 200+ | 7 | ✅ Optical flow, ORB matching, drift detection |
| **frame** | 350+ | 5 | ✅ Lazy-eval data bus (CBARFrame port), ProcessNode trait, Pipeline |
| **fusion** | 300+ | 4 | ✅ N-camera registration, cross-camera matching, self-regulating calibration |
| **rtos** | 200+ | 2 | ✅ Async pipeline executor with backpressure + shutdown |
| **stitch** | 300+ | 6 | ✅ Zones, top-down maps, entity trails, privacy per zone |
| **scene** | 350+ | 3 | ✅ Full 3D world model — rooms, floors, doors, occupancy, live entities |
| **camera** | 180+ | — | 🟡 CameraSource trait, RTSP/discovery/agent module stubs |
| **grid** | stub | — | ⬜ Continuum grid integration |
| **splat** | stub | — | ⬜ Gaussian splatting renderer |
| **detect** | stub | — | ⬜ Detection + tracking with forged models |

**30 tests passing.** The 3D reconstruction pipeline from [cb-mobile-sdk](https://github.com/CambrianTech/cb-mobile-sdk) (the production C++ CBAR engine) is being ported to Rust — lazy-eval frames, priority-tiered QueueThread RTOS, pluggable analyzer pipeline, and image-type-agnostic surface accumulation. Next: connect real cameras via RTSP and run actual frames through the pipeline.

## Contributing

Same rules as [continuum](https://github.com/CambrianTech/continuum#contributing) — pre-alpha, building in the open, human and AI contributors welcome. If you have cheap cameras and want to help test, [join the Discord](https://discord.gg/arfbCV2H).

---

<div align="center">

*Your cameras. Your hardware. Your 3D model. Your AI. Your security.*

**SimpliSafe charges you monthly to watch your own door. We charge you nothing to understand your entire property in 3D.**

**They will be disrupted.**

</div>
