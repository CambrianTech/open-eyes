"""open-eyes Python binding — ctypes wrapper for libopeneyes.

Loads the Rust shared library and exposes the C API as Python functions.
Used by the forge-alloy cv-eval executor to push frames through the pipeline.

Usage:
    from open_eyes import OpenEyesEngine, PixelFormat

    engine = OpenEyesEngine()
    engine.push_frame(camera_id=0, data=frame_bytes, width=640, height=480,
                      fmt=PixelFormat.RGB8)
    motion = engine.motion_magnitude
    engine.close()
"""

from .engine import OpenEyesEngine, PixelFormat, EventType, OEEvent

__all__ = ["OpenEyesEngine", "PixelFormat", "EventType", "OEEvent"]
