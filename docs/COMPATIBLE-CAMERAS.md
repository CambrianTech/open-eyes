# Compatible Cameras

**Compatibility = we can flash it with open-eyes firmware.**

We do not support stock firmware. Stock firmware on cheap cameras phones home to manufacturer clouds, uses proprietary P2P protocols, and can't be audited. open-eyes runs our firmware or it doesn't run.

---

## What Makes a Camera Compatible

1. **Supported SoC** — OpenIPC or Thingino has a working image for the chip
2. **SD card flashable** — no soldering, no UART, no desoldering secure boot chips
3. **Linux bootloader** — the stock bootloader accepts firmware updates via SD card (LiteOS devices can't)
4. **Standard sensor** — the image sensor has a driver in OpenIPC/Thingino

## Confirmed Compatible

| Camera | SoC | Flash Method | Price | Notes |
|---|---|---|---|---|
| **Wyze Cam v3** | Ingenic T31 | SD card (Thingino) | $15-20 | The gold standard. Huge community. |
| Wyze Cam v2 | Ingenic T20 | SD card (Thingino) | $10-15 | Older but works perfectly |
| Wyze Cam Pan v2 | Ingenic T31 | SD card (Thingino) | $25 | Pan/tilt, same SoC as v3 |
| Wyze Cam Floodlight | Ingenic T31 | SD card (Thingino) | $30 | Outdoor with lights |

## Confirmed Incompatible (DO NOT BUY)

| Camera | SoC | Why | App |
|---|---|---|---|
| **ieGeek ZS-GX5** | Hi3518EV300 | LiteOS (no SD flash), ultrasonically welded, no RTSP | CloudEdge |
| **ieGeek ZS-GQ1** | Hi3518EV300 | Same as above | CloudEdge |
| **Adorbee A3** | Unknown (Anyka?) | CloudEdge, no RTSP, no flash path | CloudEdge |
| **Wyze Cam v4** | Ingenic T40 | Secure boot — requires SoC chip swap | Wyze |
| Any CloudEdge/Meari camera | Various | P2P only, no RTSP, LiteOS, sealed shells | CloudEdge |

## How to Check Before Buying

**The app name tells you everything:**

| App | Verdict | Why |
|---|---|---|
| **CloudEdge / Meari** | AVOID | P2P only, LiteOS, sealed hardware, phones home |
| **Wyze** (v3 or older) | BUY | Thingino proven, SD card flash |
| **Wyze** (v4) | AVOID | Secure boot, chip swap required |
| **Tapo / TP-Link** | MAYBE | Some models flashable, check SoC |
| **iCSee / XMEye** | MAYBE | Often HiSilicon with OpenIPC support |
| **Tuya / Smart Life** | MAYBE | Varies wildly by model |

**FCC ID check:** Every camera sold in the US has an FCC ID on the label. Search it at [fcc.report](https://fcc.report) — the internal photos show the PCB and chip markings. Check the SoC against OpenIPC/Thingino supported lists before buying.

## Why We Don't Support Stock Firmware

We investigated. Here's what we found inside cheap cameras:

- **Ultrasonically welded shut** — can't open without destroying
- **LiteOS instead of Linux** — can't SD card flash
- **No RTSP** — video locked behind proprietary P2P cloud protocol
- **USB is power only** — no data interface
- **Certificate pinning** — can't intercept or audit traffic
- **All video routed through manufacturer's cloud in China**
- **Undocumented Bluetooth radios** — not listed in specs

This isn't lazy engineering. It's deliberate lockdown. The camera is designed to be an unauditable surveillance device that streams to servers you can't inspect. We refuse to put our name on a "compatible" label for hardware that spies on its owner.

**Flash it or don't use it.**

---

## Contributing

Tested a camera? Add it to this list:

1. Buy camera, note the brand/model
2. Find the FCC ID on the label
3. Look up internal photos at [fcc.report](https://fcc.report)
4. Identify the SoC from chip markings
5. Check [OpenIPC](https://openipc.org/cameras/vendors) and [Thingino](https://thingino.com) support
6. If flashable: test it, add to Confirmed Compatible
7. If not: add to Confirmed Incompatible with the reason
8. Submit a PR

The community builds this list. Every camera tested makes the next person's purchase easier.
