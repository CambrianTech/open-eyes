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

The core is a Rust adaptation of the [CBAR](https://github.com/CambrianTech/react-home-ar) (Cambrian AR) layer — originally built for real-time AR on mobile phones. The key techniques:

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

## Hardware

**Minimum:** one camera + one computer. That's it.

| Component | Budget Option | Better Option |
|---|---|---|
| **Cameras** | $20 Chinese wireless (Wi-Fi, RTSP) | Ring/Wyze (reversed), PoE IP cameras |
| **Processing** | Raspberry Pi 4/5 | Old laptop, mini PC |
| **GPU (optional)** | None — CPU inference | GTX 1060+, Apple Silicon |
| **Storage** | SD card, USB drive | NAS, HDD |

**Scales infinitely.** Add cameras → the 3D scene gets denser. Add processing nodes → the grid handles more cameras. Add GPU → detection runs faster and the splat view renders smoother. There is no ceiling.

## Architecture

```
open-eyes/
├── crates/
│   ├── open-eyes-core/     # 3D reconstruction (Rust CBAR adaptation)
│   │   ├── geometry/       # 3D math, transforms, projections
│   │   ├── scene/          # SceneState, accumulation, persistence
│   │   ├── features/       # ORB, optical flow, feature matching
│   │   └── fusion/         # Multi-camera fusion, temporal interpolation
│   │
│   ├── open-eyes-camera/   # Camera drivers (RTSP, ONVIF, USB, wireless)
│   ├── open-eyes-grid/     # Continuum grid integration
│   ├── open-eyes-splat/    # Gaussian splatting renderer (wgpu)
│   └── open-eyes-detect/   # Detection + tracking (forged models)
│
├── docs/                   # Architecture, camera compatibility, setup
├── Cargo.toml              # Workspace manifest
└── README.md
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

**Pre-alpha.** Architecture designed, crate structure created, core types defined. The 3D reconstruction math exists in [react-home-ar](https://github.com/CambrianTech/react-home-ar) (TypeScript/C++) and needs Rust adaptation. Camera integration, grid wiring, and the splat renderer are next.

## Contributing

Same rules as [continuum](https://github.com/CambrianTech/continuum#contributing) — pre-alpha, building in the open, human and AI contributors welcome. If you have cheap cameras and want to help test, [join the Discord](https://discord.gg/arfbCV2H).

---

<div align="center">

*Your cameras. Your hardware. Your 3D model. Your AI. Your security.*

**SimpliSafe charges you monthly to watch your own door. We charge you nothing to understand your entire property in 3D.**

**They will be disrupted.**

</div>
