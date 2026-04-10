# Target Devices

Every device that has a camera is a potential open-eyes sensor. The architecture treats them all as `CameraSource` implementations — same pipeline, different adapters.

## Priority 1 — Cheap Wireless Cameras (the floor)

The whole point: cameras anyone can afford, from brands the security industry ignores.

| Device | Price | Protocol | Notes |
|---|---|---|---|
| **Bekamtron** wireless | ~$20-30 | RTSP over Wi-Fi | Joel's primary test hardware |
| **Virtavo** egg cameras | ~$15-25 | RTSP over Wi-Fi | Joel has ~10 of these (dad's gift), white egg form factor |
| Generic Chinese Wi-Fi cams | $10-30 | RTSP, ONVIF, or HTTP MJPEG | Amazon basics tier. Most expose `rtsp://IP:554/stream1` |
| Wyze Cam v3/v4 | $20-35 | RTSP (via firmware mod) or Wyze API | Huge install base, RTSP enableable |
| Tapo C200/C210 | $25-30 | ONVIF + RTSP | TP-Link, common, documented RTSP |

**Why cheap cameras are BETTER than Ring/Nest:**
- No proprietary cloud dependency — just an RTSP stream on your LAN
- No subscription — $0/month, forever
- No phone-home firmware — or if it phones home, the binary-on-device route replaces it
- Dumb sensor + smart grid > smart sensor + dumb cloud
- More cameras per dollar = denser 3D coverage = better scene understanding

**Integration path:** RTSP client in `open-eyes-camera` crate connects to the stream URL, decodes H.264/H.265 frames via ffmpeg-next, feeds `CameraFrame` into the pipeline.

## Priority 2 — Phones (mobile viewpoints)

Phones are cameras that move. They add close-range detail and a human-controlled viewpoint to the static camera mesh.

| Device | Capabilities | Adapter |
|---|---|---|
| **iPhone 12+** (LiDAR) | RGB + depth + ARKit pose + IMU | ARKit adapter — use Apple's tracking for free, supplement with CBAR features |
| **iPhone 7+** (no LiDAR) | RGB + ARKit pose + IMU | ARKit adapter — the original react-home-ar target, proven at 60fps |
| **Android (ARCore)** | RGB + ARCore pose + IMU | ARCore adapter — same pattern as ARKit |
| **Android (no ARCore)** | RGB only | Pure-CV adapter — CBAR feature tracking for pose, no platform dependency |

**Why phones matter:**
- A person walking through the scene adds close-range 3D detail that static cameras can't provide
- The phone is BOTH a sensor (contributing frames) AND a display (showing the 3D view)
- Existing phone install base = zero hardware cost for the mobile viewpoint
- iPhone proven at 60fps in react-home-ar — this is not speculative

**Integration path:** WebRTC or native app sends camera frames + platform pose to the nearest grid node. The phone's `CameraSource` adapter wraps ARKit/ARCore and falls back to pure-CV when unavailable.

## Priority 3 — AR/VR Headsets (immersive viewpoints)

Headsets are phones with better tracking and a head-mounted display. Same `CameraSource` adapter pattern.

| Device | Capabilities | Adapter |
|---|---|---|
| **Apple Vision Pro** | RGB + depth + sub-mm head tracking + eye tracking + hand tracking | visionOS adapter |
| **Meta Quest 3/Pro** | RGB passthrough + inside-out tracking + hand tracking | WebXR adapter or native |
| **Meta Quest 2** | B&W passthrough + tracking + hands | WebXR adapter (limited) |

**Why headsets matter:**
- The human walks through the 3D reconstructed scene IN the scene — full immersion
- Head tracking provides the highest-quality moving-viewpoint pose source
- The headset IS the continuum mixed-reality interface (see `MIXED-REALITY-CBAR-INTEGRATION.md`)
- Eye tracking (Vision Pro) provides the LoD attention signal: render high-quality splats where the human is looking, coarse elsewhere

**Integration path:** WebXR API for cross-platform, native SDK adapters for platform-specific features (eye tracking, depth). The headset's `CameraSource` adapter provides frames + pose; the grid fuses them with the static camera mesh.

## Priority 4 — USB/PoE IP Cameras (the prosumer tier)

For users who want higher quality or wired reliability.

| Device | Price | Protocol | Notes |
|---|---|---|---|
| USB webcams (Logitech, etc.) | $30-80 | V4L2 / UVC | Wired, reliable, low latency |
| PoE IP cameras (Reolink, Amcrest) | $40-100 | RTSP + ONVIF | Wired power + data, outdoor rated |
| Raspberry Pi Camera Module | $25-35 | CSI direct | Lowest latency, tightest integration |

**Integration path:** V4L2 for USB, RTSP/ONVIF for IP, picamera2 bindings or direct CSI for RPi.

## Priority 5 — Reversed Proprietary Cameras

For users who already own Ring/Nest/Blink and want to liberate their hardware.

| Device | Status | Notes |
|---|---|---|
| **Ring** (Amazon) | Partially reversed | Local RTSP not officially supported; some models hackable via modified firmware |
| **Nest** (Google) | Difficult | Tightly integrated with Google Home; RTSP removed in newer models |
| **Blink** (Amazon) | Partially reversed | Battery-powered, limited stream access |
| **Arlo** | RTSP available on some models | Check per-model compatibility |

**Philosophy:** we don't wait for permission. If the camera has a sensor and a network connection, it's a potential open-eyes source. The binary-on-device route (replacing stock firmware with a minimal open-eyes agent) is the nuclear option for cameras whose manufacturers refuse to expose local streams.

---

## The Device Ladder

Same concept as continuum's model quant ladder — the system works at every tier, with quality scaling to hardware:

```
$10 Chinese Wi-Fi cam     → 720p, 15fps, RTSP, one viewpoint
$25 Virtavo/Bekamtron ×10 → multi-camera 3D reconstruction
$35 + Raspberry Pi        → on-device CBAR pipeline, grid node
$200 iPhone               → mobile viewpoint with depth + ARKit
$3500 Vision Pro          → full immersive walkthrough of the 3D scene
```

**The floor is $10.** Everything above that adds quality, not capability. The $10 camera contributes to the same 3D scene model as the $3500 headset. Your power is the sum of every sensor on your grid — not the most expensive one.

---

## What We'll Test First

1. **Virtavo egg cameras** (Joel has ~10) — connect via RTSP, feed into RTOS pipeline, verify feature extraction + optical flow work on real frames
2. **Cross-camera calibration** — mount 2+ Virtavo cameras with overlapping FOV, run the fusion engine's calibration, verify extrinsic solve
3. **Motion detection** — optical flow heartbeat detects person walking through the scene across cameras
4. **iPhone** — add a moving viewpoint to the static mesh, verify fusion handles mixed static + mobile sources
