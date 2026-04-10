# Continuum Grid Integration — How open-eyes Plugs In

**Status**: Design + implementation plan. Ready to build.

open-eyes is a **grid application**. It uses the same infrastructure as every other continuum application: Commands for control, Events for signals, Foreman for orchestration, Docker for deployment.

---

## The Pattern

Every grid application follows the same shape:

```
┌─────────────────────────────────────────────────────┐
│  Continuum Grid                                     │
│                                                     │
│  Commands.execute('app/action', params) → result    │
│  Events.emit('app:domain:event', payload)           │
│  Foreman orchestrates nodes + resources             │
│  Docker deploys containers                          │
│                                                     │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐      │
│  │ open-eyes │  │  Hermes   │  │ OpenClaw  │  ... │
│  │ cameras   │  │  agents   │  │  search   │      │
│  └───────────┘  └───────────┘  └───────────┘      │
│                                                     │
│  Same Commands. Same Events. Same Docker.           │
│  Different container contents.                      │
└─────────────────────────────────────────────────────┘
```

open-eyes is the first non-AI-chat grid application. If this integration works cleanly, every future application (Hermes, OpenClaw, custom agents, whatever) follows the same template.

---

## What Already Works in Continuum

| System | Status | What open-eyes needs from it |
|---|---|---|
| **Commands** | 339 auto-discovered | Register `open-eyes/*` commands |
| **Events** | Working pub/sub with patterns | Emit `camera:*` events, personas subscribe |
| **Grid routing** | Working via GridInterceptor | Commands route to camera node from any node |
| **Docker** | Multi-service orchestration | Add open-eyes container to compose |
| **Daemons** | 20 implemented | Create OpenEyesCameraDaemon |
| **Personas** | Working autonomous loop | SecurityAnalyst persona watches camera events |
| **Factory** | Working forge pipeline | Forge detection models via sentinel-ai |
| **Tailscale** | Working grid transport | Camera nodes join the mesh |

---

## Integration Points

### 1. Commands (what users and personas can DO)

```
open-eyes/camera/list          List all cameras with status + power state
open-eyes/camera/register      Register a new camera (from setup scan)
open-eyes/camera/stream        Start/stop RTSP stream for a camera
open-eyes/camera/status        Detailed status (connected, fps, battery, temp)

open-eyes/scene/view           Get current scene as geometry (planes, entities, rooms)
open-eyes/scene/coverage       Get coverage map (camera FOVs, dead zones)
open-eyes/scene/history        Query scene state at a past timestamp

open-eyes/entity/list          List tracked entities (people, vehicles, animals)
open-eyes/entity/track         Get trail for a specific entity
open-eyes/entity/threats       List entities with threat_level > threshold

open-eyes/alert/subscribe      Subscribe to alert events
open-eyes/alert/history        Query past alerts
open-eyes/alert/acknowledge    Acknowledge an alert (human reviewed)

open-eyes/triage/config        Get/set triage pipeline parameters
open-eyes/triage/tier          Get/set triage tier for a camera
open-eyes/power/status         Power state for all cameras
```

Each command gets a CommandGenerator spec → auto-generated server/browser/shared, README, help text. Discoverable by personas and jtag CLI.

### 2. Events (what the system SIGNALS)

```typescript
// Camera events (emitted by open-eyes nodes)
'camera:motion:detected'       // { cameraId, magnitude, direction, quadrant }
'camera:drift:detected'        // { cameraId, shiftPixels, direction }
'camera:entity:entered'        // { cameraId, entityId, class, position }
'camera:entity:left'           // { cameraId, entityId, lastPosition }
'camera:zone:crossing'         // { entityId, fromZone, toZone, timestamp }
'camera:audio:detected'        // { cameraId, class, confidence } (glass_break, scream, etc.)
'camera:threat:assessed'       // { entityId, level, reason, cameras }

// System events (emitted by Foreman/grid)
'camera:connected'             // { cameraId, model, firmware }
'camera:disconnected'          // { cameraId, lastSeen }
'camera:battery:low'           // { cameraId, level, hoursRemaining }
'camera:battery:critical'      // { cameraId, level }
'camera:solar:blocked'         // { cameraId, expectedCharge, actualCharge }
'camera:coverage:gap'          // { zone, reason, compensatingCameras }
'camera:thermal:throttle'      // { cameraId, temperature, newTier }

// Scene events
'scene:entity:tracked'         // { entityId, position3d, velocity, cameras }
'scene:plane:detected'         // { planeId, normal, label, confidence }
'scene:updated'                // { timestamp, entitiesCount, camerasOnline }
```

Personas subscribe to these via the existing Events system:
```typescript
Events.subscribe('camera:entity:entered', async (event) => {
    // SecurityAnalyst persona decides: known person or stranger?
    const threat = await assessThreat(event);
    if (threat.level > 0.7) {
        await Events.emit('camera:threat:assessed', threat);
        await Commands.execute('collaboration/chat/send', {
            room: 'security',
            message: `Unknown person detected at ${event.position}. Threat level: ${threat.level}`
        });
    }
});
```

### 3. Docker Container

```yaml
# Added to continuum/docker-compose.yml
open-eyes:
    build: ../open-eyes/docker
    volumes:
      - ipc-sockets:/root/.continuum/sockets        # IPC to continuum-core
      - open-eyes-data:/data/open-eyes               # Scene state, recordings
      - /dev/video*:/dev/video*                       # USB cameras (optional)
    environment:
      - OPENEYES_CONFIG=/data/open-eyes/config.json
      - RUST_LOG=info
    depends_on:
      continuum-core:
        condition: service_healthy
    profiles: ["cameras"]
    deploy:
      resources:
        reservations:
          devices:
            - capabilities: [gpu]                     # Optional: detection models
```

Start with cameras: `docker compose --profile cameras up`

The open-eyes container:
1. Loads `libopeneyes.so` (the Rust FFI binary)
2. Connects to continuum-core via IPC socket
3. Registers `open-eyes/*` commands
4. Subscribes to grid events (node health, Foreman commands)
5. Starts camera discovery (mDNS/SSDP for local cameras, Tailscale for remote)
6. Begins triage on discovered cameras
7. Emits events to the grid as cameras detect motion/entities/threats

### 4. Rust IPC Bridge

open-eyes talks to continuum-core via the same Unix socket IPC that all Rust workers use:

```rust
// In open-eyes-grid crate
use continuum_core_ipc::{IpcClient, IpcMessage};

struct GridBridge {
    ipc: IpcClient,
}

impl GridBridge {
    async fn emit_event(&self, event_type: &str, payload: serde_json::Value) {
        self.ipc.send(IpcMessage::Event {
            topic: event_type.to_string(),
            payload,
        }).await;
    }

    async fn register_command(&self, name: &str) {
        self.ipc.send(IpcMessage::RegisterCommand {
            name: name.to_string(),
        }).await;
    }

    async fn handle_command(&self, name: &str, params: serde_json::Value) -> serde_json::Value {
        // Route to appropriate open-eyes function
        match name {
            "open-eyes/camera/list" => self.list_cameras().await,
            "open-eyes/scene/view" => self.get_scene().await,
            // ...
        }
    }
}
```

### 5. SecurityAnalyst Persona

A PersonaUser that specializes in watching camera events:

```typescript
// In continuum persona config
{
    "name": "Sentinel",
    "role": "Security Analyst",
    "systemPrompt": "You monitor the open-eyes camera network...",
    "subscriptions": [
        "camera:entity:entered",
        "camera:threat:assessed",
        "camera:audio:detected",
        "camera:coverage:gap"
    ],
    "tools": [
        "open-eyes/camera/list",
        "open-eyes/scene/view",
        "open-eyes/entity/track",
        "open-eyes/alert/history"
    ]
}
```

The persona:
- Receives camera events via its inbox
- Assesses context (time of day, zone, entity history)
- Decides alert level (log quietly vs notify user)
- Responds in the security chat room
- Can query scene state and entity tracks to build context

---

## Implementation Order

### Step 1: Event Bridge (smallest useful integration)
Wire open-eyes event emission → continuum Events system. Camera events appear in the grid. Personas can subscribe. No commands yet, just signals flowing.

### Step 2: Basic Commands
`open-eyes/camera/list` and `open-eyes/scene/view` via CommandGenerator. Personas and jtag can query camera state.

### Step 3: Docker Container
Containerize open-eyes, add to docker-compose.yml, IPC socket shared with continuum-core.

### Step 4: Full Command Set
All 15+ commands registered, discoverable, documented.

### Step 5: SecurityAnalyst Persona
Persona config, event subscriptions, threat assessment logic, chat room integration.

### Step 6: Foreman Camera Management
Load balancing, power-aware scheduling, coverage gap detection, triage tier management.

---

## What This Proves

If open-eyes integrates cleanly:
- The grid architecture works for ANY application, not just AI chat
- Commands + Events are truly universal primitives
- Docker containerization makes deployment trivial
- Personas can reason about physical-world sensor data
- The same forge pipeline that compacts LLMs compacts detection models

This is the test. If it works, Hermes (agent orchestration), OpenClaw (web search), voice assistants, home automation — they all follow the same template. open-eyes is the proof that continuum is a platform, not just a chatbot.
