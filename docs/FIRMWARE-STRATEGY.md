# Firmware Strategy — OpenIPC + open-eyes Agent

**Status**: Architecture. The cheap camera firmware path for open-eyes.

---

## The Stack

```
┌─────────────────────────────────────────┐
│         open-eyes agent (Rust)          │  ← OUR CODE
│  RTSP server → grid node only           │
│  Tailscale encrypted transport          │
│  Optional: lightweight CBAR tier-1      │
│  (optical flow at quarter-res on ARM)   │
├─────────────────────────────────────────┤
│         OpenIPC / Thingino              │  ← OPEN-SOURCE BASE
│  Minimal Linux (busybox + drivers)      │
│  Camera sensor driver (ISP pipeline)    │
│  H.264/H.265 hardware encoder          │
│  WiFi driver                            │
├─────────────────────────────────────────┤
│         SoC Hardware                    │  ← THE CHIP
│  ARM Cortex-A7/A9 (500MHz-1.2GHz)      │
│  HW video encoder (H.264/H.265)        │
│  Optional NPU (some newer SoCs)         │
│  8-64MB RAM, 8-16MB flash              │
└─────────────────────────────────────────┘
```

**We don't write an OS. We write a small Rust agent that runs ON OpenIPC.** Same pattern as continuum: we write the intelligence, the community provides the substrate.

---

## Why OpenIPC + Thingino

| | Stock firmware | OpenIPC/Thingino | open-eyes on OpenIPC |
|---|---|---|---|
| **Phones home** | Yes (China) | No | No |
| **RTSP exposed** | Sometimes | Always | Yes, grid-node-only |
| **SSH access** | No | Yes | Yes |
| **Encryption** | None or weak | Configurable | Tailscale (WireGuard) |
| **Custom binaries** | No | Yes (cross-compile) | The whole point |
| **Update mechanism** | Cloud-dependent | Local flash | Git-based OTA |
| **Camera ISP** | Full access | Full access | Full access |
| **Resource overhead** | Bloated (cloud client, P2P, app server) | Minimal (busybox) | Minimal + our agent |

**OpenIPC** is the base for HiSilicon and multi-platform support. **Thingino** is the base for Ingenic T-series (Wyze-class). Both are production-grade, actively maintained, and support 100+ camera models. We pick whichever matches the SoC.

---

## The open-eyes Agent (what we write)

A single Rust binary, cross-compiled for ARM, that runs on the camera:

```rust
// Conceptual — the actual agent is in crates/open-eyes-camera/
struct OnCameraAgent {
    // Tier 1: the heartbeat — runs at full frame rate
    flow_detector: QuarterResFlowDetector,

    // Output: RTSP stream to grid node
    rtsp_server: LocalRtspServer,

    // Security: encrypted tunnel to grid
    transport: TailscaleOrDirect,

    // Events: motion detected, camera drift, health
    event_emitter: GridEventEmitter,
}
```

**What it does:**
1. **Captures frames** from the camera's ISP pipeline (V4L2 or direct memory-mapped)
2. **Runs tier-1 optical flow** at quarter resolution on the ARM CPU — this is the CBAR heartbeat. At quarter-res (160x120 from 640x480), optical flow on an ARM Cortex-A7 at 500MHz runs in ~5ms per frame = 200fps theoretical, 30fps practical with overhead. **No framerate compromise.**
3. **Emits motion events** to the grid when flow magnitude exceeds threshold — the grid node doesn't have to watch every frame, it gets signaled
4. **Streams RTSP** to the grid node (and ONLY the grid node) — H.264 hardware-encoded by the SoC's dedicated encoder, zero CPU cost for encoding
5. **Tailscale-encrypted** — the camera joins the Tailscale mesh as a headless node, all traffic is WireGuard-encrypted
6. **Health reporting** — temperature, uptime, WiFi signal, frame drops

**What it does NOT do:**
- No cloud connectivity
- No HTTP server (the stock firmware's #1 attack surface)
- No UPnP (the stock firmware's #2 attack surface)
- No default passwords (Tailscale auth replaces credentials)
- No heavy inference (that's the grid node's job)

---

## The CBAR Split — What Runs Where

The key insight: the CBAR pipeline's two-tier design tells us exactly what runs on the camera vs the grid.

**On camera (ARM, ~500MHz, 32-64MB RAM):**
- Optical flow at quarter-res (the heartbeat) — ~5ms/frame
- Frame capture + H.264 encode (hardware, zero CPU)
- Motion event emission (when flow exceeds threshold)
- RTSP server (just forwarding the H.264 stream)
- Camera drift detection (global flow shift = camera bumped)

**On grid node (x86/ARM64, GPU, GB of RAM):**
- Feature extraction (ORB/FAST) — tier 2, on-demand
- Surface normals (CNN) — runs rarely, cached
- Semantic segmentation — runs rarely, cached
- Entity detection (YOLO) — triggered by motion events
- Cross-camera fusion (the N-camera piece)
- Splat rendering (GPU)
- Persona reasoning (AI inference)

**The camera does 1% of the compute and sends 99% of the signal.** The grid node does 99% of the compute on frames the camera flagged as interesting. Frames with no motion never leave the camera's local buffer.

---

## Cross-Compilation

The agent is a Rust binary cross-compiled for the camera's ARM architecture:

```bash
# For HiSilicon Hi3516 (ARM Cortex-A7, hard-float)
rustup target add armv7-unknown-linux-gnueabihf
cargo build --target armv7-unknown-linux-gnueabihf --release \
    -p open-eyes-camera

# For Ingenic T31 (MIPS — some Thingino cameras)
rustup target add mipsel-unknown-linux-gnu
cargo build --target mipsel-unknown-linux-gnu --release \
    -p open-eyes-camera

# The resulting binary is ~2-5MB, fits in the camera's flash
```

**The binary is self-contained.** No runtime dependencies beyond libc (provided by OpenIPC's busybox). No Python, no Node, no JVM. Just a statically-linked Rust binary that starts on boot and runs forever.

---

## Deployment Path

### Phase 1: Raw RTSP (no flash required)
Many cameras expose RTSP on their stock firmware. Start here:
1. Find the camera's RTSP URL (usually `rtsp://IP:554/stream1`)
2. Point open-eyes-camera's RTSP client at it
3. Grid node processes the stream
4. **Works TODAY, no firmware modification**

### Phase 2: OpenIPC flash
For cameras where stock firmware is unacceptable (phones home, no RTSP, etc.):
1. Identify the SoC (open the camera, read chip markings)
2. Check OpenIPC/Thingino compatibility
3. Flash via UART or SD card (OpenIPC has per-SoC guides)
4. SSH in, deploy the open-eyes agent binary
5. Camera is now a locked-down grid sensor

### Phase 3: open-eyes firmware image
Package OpenIPC + our agent into a single flashable image:
1. OpenIPC base layer (kernel + drivers + busybox)
2. open-eyes agent (Rust binary)
3. Tailscale (headless node)
4. Auto-discovery (the camera announces itself to the grid on boot)
5. **One-flash setup: flash the image, camera joins the grid automatically**

---

## Security Practices (non-negotiable)

Per the project's political positioning (open-source disruption of surveillance capitalism):

1. **No outbound connections except to the grid mesh** — iptables rules baked into the firmware image
2. **No HTTP server** — nothing listening on any port except the grid transport
3. **No default credentials** — Tailscale auth is the only access mechanism
4. **Encrypted storage** — if the camera has local storage (SD card), encrypt it
5. **Verified boot** (where SoC supports it) — prevent firmware tampering
6. **OTA updates via grid** — the Foreman pushes firmware updates to cameras over the encrypted mesh, not via cloud download
7. **Minimal attack surface** — the agent binary + tailscale + busybox. Nothing else runs. `ps` shows 5 processes, not 50.

**Speed is not compromised by security.** The Tailscale encryption is WireGuard which runs at line speed on these SoCs (the crypto is simple enough for ARM). The RTSP stream is hardware-encoded by the SoC's dedicated H.264 engine — zero CPU cost regardless of security layer. The optical flow heartbeat runs in 5ms per frame on bare ARM — encryption overhead is invisible at that scale.

---

## Supported SoCs (via OpenIPC)

Check [openipc.org/supported-hardware](https://openipc.org/supported-hardware) for the full list. Key families:

| SoC | Architecture | RAM | NPU | OpenIPC | Thingino | Notes |
|---|---|---|---|---|---|---|
| Hi3516Ev200 | ARM Cortex-A7 | 64MB | No | ✅ | — | Most common budget IP cam |
| Hi3518Ev200 | ARM926EJ-S | 64MB | No | ✅ | — | Older, very common |
| T31 | MIPS | 64-128MB | Yes (small) | ✅ | ✅ | Wyze v3 SoC, good NPU |
| T40 | MIPS | 256MB | Yes | ✅ | ✅ | Higher-end Ingenic |
| SSD202 | ARM Cortex-A7 | 64MB | No | ✅ | — | Sigmastar, ultra-cheap |
| SSC338Q | ARM Cortex-A7 | 128MB | Yes | ✅ | — | Sigmastar with NPU |

**Cameras with NPUs** (T31, T40, SSC338Q) could potentially run tiny detection models ON the camera — a forged nano-YOLO or a person-vs-not classifier. This is the ultimate edge: the camera does coarse detection locally, only sends frames that matter to the grid. But this is Phase 3+ optimization; Phase 1-2 work without any NPU.

---

## What Joel Needs To Do First

1. **Grab a Virtavo egg camera and pop the shell** — usually 2-4 screws or snap clips
2. **Photo the main board** — focus on the largest chip (the SoC) and read its markings
3. **Paste the chip ID here** — I'll check OpenIPC compatibility immediately
4. **Also check if UART pads are visible** — they're usually labeled TX/RX/GND near the edge of the board

That's 5 minutes of hardware time and it determines the entire firmware strategy.

---

## See also

- [OpenIPC](https://openipc.org) — the camera firmware we build on
- [Thingino](https://thingino.com) — Ingenic-specific alternative
- `crates/open-eyes-camera/` — the camera driver crate (will host the on-camera agent)
- `docs/TARGET-DEVICES.md` — full device compatibility list
- `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — the two-tier compute model that splits camera vs grid
