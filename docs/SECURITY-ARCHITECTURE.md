# Security Architecture — Zero Trust from Silicon to Mesh

**Status**: Architecture. The security model for open-eyes cameras.

open-eyes doesn't add security on top of an insecure system. Security is the architecture. Every layer, from the firmware to the mesh transport, is zero-trust by design.

---

## The Problem We Saw

We bought 20+ cameras. We opened them. Here's what we found:

- Ultrasonically welded shut (can't inspect)
- LiteOS (can't flash, can't audit)
- No RTSP (video locked behind proprietary P2P to China)
- USB is power only (no data interface)
- Undocumented Bluetooth radios
- Certificate pinning (can't see what they send)
- "Security camera" that is itself a surveillance device

This is the industry standard. Millions of these are in bedrooms and nurseries.

**open-eyes exists because this is unacceptable.**

---

## Principles

1. **No stock firmware.** We flash our own. If we can't flash it, we don't support it.
2. **No internet.** Cameras connect to the mesh. The mesh doesn't route to the internet.
3. **No passwords.** FIDO2 cryptographic identity. Physical presence to register.
4. **No cloud.** Processing is local. Data stays on your network.
5. **No trust.** Every message is authenticated. Every hop is encrypted. Every device is verified.
6. **Revocable.** Remove a camera's credential, it's dead to the mesh instantly.
7. **Auditable.** Open source firmware. Open source pipeline. Read every line.

---

## Identity: FIDO2 on Every Camera

Every camera is a FIDO2 authenticator. Same cryptographic model that secures bank logins.

### Registration (one-time, physical)

```
1. Flash open-eyes firmware to SD card
2. Insert SD card, needle-reset camera
3. Camera boots, generates Ed25519 keypair
4. Camera's LED blinks registration pattern
5. Point phone at camera (physical proximity = proof of ownership)
6. Phone scans LED pattern OR connects via temporary BLE
7. Camera sends public key to phone
8. Phone registers public key with grid node
9. Camera is now a trusted member of the mesh
```

**The needle-reset IS the authenticator gesture.** Same as touching a YubiKey — physical presence proves you're standing next to the device. No cloud server verifies you. No app "claims" the device. You're there. That's enough.

### Authentication (every message)

```
Camera → signs event with private key → mesh transport → 
Grid node → verifies signature against registered public key →
Accept or reject

Private key never leaves the camera.
No shared secrets. No passwords. No tokens to steal.
```

### Revocation (instant)

```
Phone → Commands.execute('open-eyes/camera/revoke', { cameraId }) →
Grid node removes public key from trust store →
Camera's messages are rejected by every node on the mesh →
Camera is dead. Instantly. No propagation delay.
```

The camera can keep broadcasting. Nobody will listen. The credential is gone.

---

## Transport: Reticulum Mesh

No internet. No DNS. No IP addresses. Just cryptographic identities on a mesh.

### Why Not WiFi + Internet?

| | WiFi + Internet | Reticulum Mesh |
|---|---|---|
| Camera phones home? | Yes (manufacturer cloud) | **Impossible** (no internet route) |
| Compromised camera exfiltrates? | Yes (it has internet) | **Impossible** (mesh is the only network) |
| ISP sees your camera traffic? | Yes | **No** (no internet) |
| Works without internet? | No | **Yes** |
| Government subpoenas your video? | From the cloud | **Can't** (no cloud, data is on your hardware) |
| Range | WiFi range (~30m) | **LoRa: kilometers** |

### How It Works

```
Camera ←→ Reticulum ←→ Grid Node ←→ Phone App

Reticulum transports:
  - WiFi (local network, no internet needed)
  - LoRa (long range, low bandwidth, solar cameras)
  - Bluetooth (short range, setup only)
  - Tailscale (when you CHOOSE to enable internet access)

Every hop is encrypted. Addresses ARE public keys.
No DNS. No certificates. No certificate authorities.
```

### LoRa for Remote Cameras

Solar camera on a barn 2 miles away. No WiFi. No internet. Just a LoRa radio.

```
Camera triage event: 25 bytes
LoRa at 1200 baud: 0.17 seconds per event
10 events/minute: 4.2 seconds of airtime = 0.7% duty cycle

The radio is idle 99.3% of the time.
Video stays on the camera's SD card.
Syncs over WiFi when you're in range (or never — your choice).
```

---

## Firmware: Our Code, Our Rules

### What Our Firmware Does

- OpenIPC base (Linux kernel + drivers)
- open-eyes Rust agent (triage pipeline)
- Reticulum transport (mesh networking)
- FIDO2 identity module (Ed25519 keypair)
- Locked-down iptables (no outbound except mesh)

### What Our Firmware Does NOT Do

- No HTTP server (attack surface)
- No UPnP (attack surface)
- No DNS resolution (can't phone home)
- No internet route (can't exfiltrate)
- No default passwords (FIDO2 only)
- No manufacturer cloud client (deleted)
- No telemetry (we don't spy either)

### Verified Boot (where SoC supports it)

On chips that support it, the bootloader verifies the firmware signature before executing. This prevents:
- Evil maid attacks (someone replaces your SD card)
- Supply chain attacks (firmware tampered in transit)
- Rollback attacks (downgrading to vulnerable firmware)

We use verified boot to protect YOU, not to lock you out. The signing key is published. You can build and sign your own firmware. The verification is for integrity, not for control.

**This is the opposite of Wyze v4's secure boot** — they use it to prevent you from running your own firmware. We use it to prevent attackers from running theirs.

---

## Data: Yours, Always

### Where Your Data Lives

| Data | Location | Encrypted | You Control |
|---|---|---|---|
| Live video | Camera → mesh → grid node | Yes (Reticulum) | Yes |
| Recorded video | Camera SD card + grid node storage | Yes (at rest) | Yes |
| Motion events | Grid node + mesh broadcast | Yes | Yes |
| Entity tracks | Grid node | Yes | Yes |
| Scene model | Grid node | Yes | Yes |

### Where Your Data Does NOT Live

- Not on our servers (we don't have servers)
- Not on any cloud (there is no cloud)
- Not on the manufacturer's infrastructure (we replaced their firmware)
- Not accessible to law enforcement without a warrant served to YOU (not to a company)

### Deletion

Delete means delete. Not "we'll remove it from the UI but keep it in our database for 7 years." When you delete a recording, it's gone. `shred` on the file, zero the blocks.

---

## Threat Model

### What We Defend Against

| Threat | Defense |
|---|---|
| Manufacturer spying | Our firmware, no internet route |
| Network eavesdropping | Reticulum encryption on every hop |
| Camera theft | FIDO2 credential is device-bound, no secrets to extract |
| Compromised camera | Revoke credential, camera is dead to mesh |
| Evil maid (firmware swap) | Verified boot where SoC supports |
| Government subpoena | No cloud = nothing to subpoena. Warrant goes to you. |
| Supply chain compromise | Open source firmware, reproducible builds, community audit |
| Neighbor snooping on mesh | Per-camera encryption, zone-level access control |

### What We Don't Defend Against

- Physical access to a running camera (if they have your camera, they have the current frame)
- Compromise of your phone (the phone holds the registration authority)
- Compromise of your grid node (it processes the video)
- A determined nation-state with physical access to your property

We're not pretending to be NSA-proof. We're making it impossible for a $15 camera to betray you. That's the bar.

---

## See Also

- [FIRMWARE-STRATEGY.md](FIRMWARE-STRATEGY.md) — SD card flash, OpenIPC, on-device agent
- [COMPATIBLE-CAMERAS.md](COMPATIBLE-CAMERAS.md) — cameras we can flash
- [ON-DEVICE-ARCHITECTURE.md](ON-DEVICE-ARCHITECTURE.md) — triage pipeline, power management
- [CONTINUUM-INTEGRATION.md](CONTINUUM-INTEGRATION.md) — grid events, commands, personas
