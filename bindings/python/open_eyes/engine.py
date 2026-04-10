"""Python binding for libopeneyes — ctypes wrapper over the Rust C ABI.

Thin wrapper. All compute is in Rust. Python is the orchestrator for
the forge-alloy cv-eval executor. Frames go in, geometry comes out.
"""

import ctypes
import enum
import os
import platform
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, List, Optional


class PixelFormat(enum.IntEnum):
    BGRA8 = 0
    YUV420 = 1
    RGB8 = 2
    GRAY8 = 3


class EventType(enum.IntEnum):
    MOTION = 0
    CAMERA_DRIFT = 1
    ENTITY_ENTERED = 2
    ENTITY_LEFT = 3
    ZONE_CROSSING = 4
    PLANE_DETECTED = 5
    ERROR = 255


@dataclass
class OEEvent:
    event_type: EventType
    camera_id: int
    entity_id: int
    value: float
    position: tuple
    timestamp: float


# C struct matching OEEvent in types.rs
class _CEvent(ctypes.Structure):
    _fields_ = [
        ("event_type", ctypes.c_uint32),
        ("camera_id", ctypes.c_uint32),
        ("entity_id", ctypes.c_uint64),
        ("value", ctypes.c_float),
        ("position", ctypes.c_float * 3),
        ("timestamp", ctypes.c_double),
    ]


# Callback type for oe_poll_events
_EVENT_CALLBACK = ctypes.CFUNCTYPE(None, ctypes.POINTER(_CEvent))


def _find_library() -> str:
    """Find libopeneyes shared library. Search order:
    1. OPENEYES_LIB_PATH env var
    2. Next to this Python file (bindings/python/open_eyes/)
    3. Workspace target/release/
    4. Workspace target/debug/
    """
    system = platform.system()
    if system == "Darwin":
        lib_name = "libopen_eyes_ffi.dylib"
    elif system == "Linux":
        lib_name = "libopen_eyes_ffi.so"
    elif system == "Windows":
        lib_name = "open_eyes_ffi.dll"
    else:
        lib_name = "libopen_eyes_ffi.so"

    # Env var override
    env_path = os.environ.get("OPENEYES_LIB_PATH")
    if env_path and os.path.exists(env_path):
        return env_path

    # Search relative to this file
    this_dir = Path(__file__).parent
    search_paths = [
        this_dir / lib_name,
        this_dir.parent.parent.parent / "target" / "release" / lib_name,
        this_dir.parent.parent.parent / "target" / "debug" / lib_name,
    ]

    for path in search_paths:
        if path.exists():
            return str(path)

    raise FileNotFoundError(
        f"Cannot find {lib_name}. Build with `cargo build --release -p open-eyes-ffi` "
        f"or set OPENEYES_LIB_PATH. Searched: {[str(p) for p in search_paths]}"
    )


class OpenEyesEngine:
    """Python wrapper for the open-eyes Rust engine.

    Usage:
        engine = OpenEyesEngine()
        for frame in video_frames:
            engine.push_frame(0, frame.data, frame.width, frame.height, PixelFormat.RGB8)
            if engine.motion_magnitude > 0.05:
                print(f"Motion detected: {engine.motion_magnitude}")
        events = engine.poll_events()
        engine.close()
    """

    def __init__(self, lib_path: Optional[str] = None):
        path = lib_path or _find_library()
        self._lib = ctypes.cdll.LoadLibrary(path)
        self._setup_bindings()
        self._ptr = self._lib.oe_create()
        if not self._ptr:
            raise RuntimeError("oe_create returned null")
        self._closed = False

    def _setup_bindings(self):
        lib = self._lib

        # oe_create() -> *mut OEEngine
        lib.oe_create.restype = ctypes.c_void_p
        lib.oe_create.argtypes = []

        # oe_destroy(*mut OEEngine)
        lib.oe_destroy.restype = None
        lib.oe_destroy.argtypes = [ctypes.c_void_p]

        # oe_push_frame(*mut OEEngine, camera_id, data, len, w, h, format, rotation) -> i32
        lib.oe_push_frame.restype = ctypes.c_int32
        lib.oe_push_frame.argtypes = [
            ctypes.c_void_p,    # engine
            ctypes.c_uint32,    # camera_id
            ctypes.c_char_p,    # data
            ctypes.c_size_t,    # len
            ctypes.c_uint32,    # width
            ctypes.c_uint32,    # height
            ctypes.c_uint32,    # format (OEPixelFormat)
            ctypes.c_int32,     # rotation
        ]

        # oe_set_camera_pose(*mut OEEngine, camera_id, *const f32) -> i32
        lib.oe_set_camera_pose.restype = ctypes.c_int32
        lib.oe_set_camera_pose.argtypes = [
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.POINTER(ctypes.c_float),
        ]

        # oe_get_motion(*const OEEngine) -> f32
        lib.oe_get_motion.restype = ctypes.c_float
        lib.oe_get_motion.argtypes = [ctypes.c_void_p]

        # oe_get_frame_count(*const OEEngine) -> u64
        lib.oe_get_frame_count.restype = ctypes.c_uint64
        lib.oe_get_frame_count.argtypes = [ctypes.c_void_p]

        # oe_poll_events(*const OEEngine, callback) -> i32
        lib.oe_poll_events.restype = ctypes.c_int32
        lib.oe_poll_events.argtypes = [ctypes.c_void_p, _EVENT_CALLBACK]

        # oe_set_intrinsics(*mut OEEngine, camera_id, fx, fy, cx, cy, w, h, k1, k2, k3) -> i32
        lib.oe_set_intrinsics.restype = ctypes.c_int32
        lib.oe_set_intrinsics.argtypes = [
            ctypes.c_void_p,
            ctypes.c_uint32,
            ctypes.c_double, ctypes.c_double,  # fx, fy
            ctypes.c_double, ctypes.c_double,  # cx, cy
            ctypes.c_uint32, ctypes.c_uint32,  # width, height
            ctypes.c_double, ctypes.c_double, ctypes.c_double,  # k1, k2, k3
        ]

    def push_frame(
        self,
        camera_id: int,
        data: bytes,
        width: int,
        height: int,
        fmt: PixelFormat = PixelFormat.RGB8,
        rotation: int = 0,
    ) -> None:
        """Push a raw frame into the pipeline. This is the hot path."""
        if self._closed:
            raise RuntimeError("Engine is closed")
        result = self._lib.oe_push_frame(
            self._ptr, camera_id,
            data, len(data),
            width, height,
            int(fmt), rotation,
        )
        if result != 0:
            raise RuntimeError(f"oe_push_frame returned {result}")

    def push_numpy_frame(
        self,
        camera_id: int,
        array,  # numpy.ndarray (H, W, 3) uint8
        fmt: PixelFormat = PixelFormat.RGB8,
        rotation: int = 0,
    ) -> None:
        """Push a numpy array (H, W, C) as a frame. Avoids copy if contiguous."""
        if not array.flags["C_CONTIGUOUS"]:
            array = array.copy(order="C")
        h, w = array.shape[:2]
        self._lib.oe_push_frame(
            self._ptr, camera_id,
            array.ctypes.data_as(ctypes.c_char_p),
            array.nbytes,
            w, h, int(fmt), rotation,
        )

    def set_camera_pose(self, camera_id: int, transform: list) -> None:
        """Set camera pose (4x4 column-major float matrix)."""
        if len(transform) != 16:
            raise ValueError("Transform must be 16 floats (4x4 column-major)")
        arr = (ctypes.c_float * 16)(*transform)
        self._lib.oe_set_camera_pose(self._ptr, camera_id, arr)

    def set_intrinsics(
        self, camera_id: int,
        fx: float, fy: float,
        cx: float, cy: float,
        width: int, height: int,
        k1: float = 0.0, k2: float = 0.0, k3: float = 0.0,
    ) -> None:
        self._lib.oe_set_intrinsics(
            self._ptr, camera_id,
            fx, fy, cx, cy, width, height, k1, k2, k3,
        )

    @property
    def motion_magnitude(self) -> float:
        """Current motion magnitude (0.0 = static)."""
        if self._closed:
            return 0.0
        return self._lib.oe_get_motion(self._ptr)

    @property
    def frame_count(self) -> int:
        """Total frames processed."""
        if self._closed:
            return 0
        return self._lib.oe_get_frame_count(self._ptr)

    def poll_events(self) -> List[OEEvent]:
        """Drain all pending events from the engine."""
        if self._closed:
            return []
        events = []

        @_EVENT_CALLBACK
        def _callback(event_ptr):
            e = event_ptr.contents
            events.append(OEEvent(
                event_type=EventType(e.event_type),
                camera_id=e.camera_id,
                entity_id=e.entity_id,
                value=e.value,
                position=(e.position[0], e.position[1], e.position[2]),
                timestamp=e.timestamp,
            ))

        self._lib.oe_poll_events(self._ptr, _callback)
        return events

    def close(self):
        """Destroy the engine. Safe to call multiple times."""
        if not self._closed and self._ptr:
            self._lib.oe_destroy(self._ptr)
            self._ptr = None
            self._closed = True

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

    def __del__(self):
        self.close()
