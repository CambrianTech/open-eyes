# Onboarding — From Download to Secured Property

**Status**: Product design. The user journey from "I want security cameras" to "my property is monitored."

---

## The Funnel

Each step is optional. Each adds value. Nobody is forced to do anything to get started.

### Step 1: Download the App

iOS or Android. Free. No account creation. No email. No subscription.

The app IS the product. It runs the open-eyes pipeline directly on your phone via the Rust FFI. No server needed. An iPad on your counter can be your entire security system.

### Step 2: Find Your Cameras

Open the app. It scans your local network automatically:
- Probes common RTSP ports (554, 8554)
- Tries common URL patterns (stream1, ch0, Hikvision, Dahua, ONVIF)
- Tries default credentials (admin/admin, admin/empty)
- Shows all discovered cameras with a live preview thumbnail

Tap a camera to add it. That's it. **Working security in under 60 seconds.**

If your cameras don't expose RTSP (some cheap ones don't until you enable it via the manufacturer's app), the app tells you which setting to change and links to the manufacturer's app.

### Step 3: Position Your Cameras

Point your phone at each mounted camera. The app uses ARKit/ARCore to:
- Locate the camera in 3D space (feature matching against the RTSP stream)
- Record its position and orientation
- Compute its field of view
- Show coverage on a floor plan

This step is optional but makes the coverage map accurate. Without it, the app just shows camera feeds in a grid.

### Step 4: Monitor

The app shows:
- Live camera feeds (tap any camera for full screen)
- Motion events with timestamps
- Coverage map (which areas are watched)
- Entity tracking (person walked from A to B)
- Historical timeline (scrub back to any past event)

Everything processes on your phone. No cloud. No subscription.

### Step 5: Upgrade Firmware (Optional)

The app suggests this after a few days of use:

> "Your cameras work great! Want better privacy and less WiFi usage? Upgrade 2 of your cameras to open-eyes firmware."

The user taps "Upgrade" and the app:
1. Identifies the camera model (from RTSP stream metadata or web interface scrape)
2. Downloads the right firmware image (~5MB)
3. Prompts: "Plug in an SD card reader" (links to Amazon for a $10 USB-C reader)
4. Writes the firmware to SD card (30 seconds)
5. Shows: "Insert the SD card into [Front Door Camera] and press the reset button with a pin"
6. Camera reboots with open-eyes firmware
7. App auto-discovers the upgraded camera (mDNS)
8. Previous recordings + position preserved

**What changes after upgrade:**
- Camera stops phoning home to manufacturer's cloud
- On-device triage: only interesting frames cross WiFi (90% bandwidth reduction)
- Battery status + solar charge monitoring (for wireless cameras)
- Firmware updates from your grid, not the manufacturer
- Locked-down security (no open ports, no default passwords)

### Step 6: Add a Grid Node (Optional, Power Users)

For 24/7 monitoring when your phone is away:

> "Want monitoring even when you leave the house? Set up a grid node."

Options:
- **Raspberry Pi** ($35) — runs Docker, low power, always on
- **Old laptop** — already have one? Install Docker, run `./setup.sh`
- **Mini PC** ($150) — NUC or equivalent, best balance
- **BigMama-class** ($2K) — GPU for YOLO, splats, persona reasoning

The grid node runs the same open-eyes pipeline but 24/7. Your phone becomes a viewer, not the processor. The Foreman on the grid node manages the cameras, handles power balancing, and alerts you via continuum chat when something happens.

### Step 7: Community Mesh (Optional)

> "Your neighbor Sarah also uses open-eyes. Want to share coverage of the shared driveway?"

Tap to invite. Both users approve. The mesh links camera zones across properties. Cross-property entity tracking: "A vehicle was seen at 123 Main (Sarah's), then appeared at 125 Main (yours) 30 seconds later."

Fully opt-in. Per-zone privacy controls. Either party can disconnect anytime.

---

## Hardware Shopping List

**Minimum (phone only):**
- 1-4 cheap WiFi cameras ($15-25 each) — Amazon, any brand
- Your phone/iPad

**Recommended:**
- 2-6 cameras ($15-25 each)
- Micro SD cards ($5 each) — for firmware upgrade
- USB-C SD card reader ($10) — one-time purchase
- Raspberry Pi or mini PC ($35-150) — optional grid node

**Full setup:**
- 4-8 cameras with solar panels ($30-50 each)
- Grid node with GPU (for detection models + splats)
- Tailscale for remote access

**Total cost for "better than Ring":** $60-150 + your phone. $0/month forever.

---

## What We Never Ask

- Never ask for an email address
- Never ask for a credit card
- Never ask for a "cloud account"
- Never require internet access (works fully offline on LAN)
- Never require a server (phone is sufficient)
- Never require firmware flashing (stock RTSP works)
- Never silently add cameras (human approves every one)
- Never send data off the local network

Every "advanced" feature (firmware, grid node, community mesh) is opt-in and clearly explained. The base experience is: download app → find cameras → monitor. Under 60 seconds.

---

## Competitive Comparison

| | Ring | SimpliSafe | open-eyes |
|---|---|---|---|
| **Cost to start** | $100 camera + $100/yr subscription | $250 kit + $180/yr monitoring | $15 camera + free app |
| **Monthly cost** | $10-20/month | $15-25/month | **$0/month** |
| **Setup time** | 15 min (account + WiFi + QR + app) | 30 min (base station + sensors) | **60 seconds** (app finds cameras) |
| **Cloud required** | Yes (all video goes to Amazon) | Yes (monitoring goes to their NOC) | **No** (everything local) |
| **Works offline** | No | Limited | **Yes** |
| **Own your data** | No (Amazon stores it) | No (SimpliSafe stores it) | **Yes** |
| **3D scene** | No (flat video feeds) | No | **Yes** (navigable 3D model) |
| **Cross-camera tracking** | No (per-camera silos) | No | **Yes** (unified 3D trajectory) |
| **AI analysis** | "Person detected" (cloud) | "Motion detected" | **On-device triage + grid AI** |
| **Community mesh** | Neighbors app (Amazon-controlled) | No | **Opt-in peer mesh** |
| **Open source** | No | No | **AGPL-3.0** |
