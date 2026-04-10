package com.cambriantech.openeyes

import android.media.Image
import java.nio.ByteBuffer

/**
 * OpenEyes — Kotlin wrapper for the Rust open-eyes engine.
 *
 * Thin binding. All compute is in Rust. This file handles:
 * 1. Camera frame delivery (CameraX, Camera2, Meta Quest Passthrough)
 * 2. AR pose delivery (ARCore, OVR SDK)
 * 3. Event polling back to the caller
 *
 * Works on: Android phones/tablets, Meta Quest (2/3/Pro), Android Auto cameras
 * The engine doesn't care about the platform — it receives frames + poses.
 */
class OpenEyesEngine : AutoCloseable {

    // Native engine pointer (opaque, managed by Rust)
    private var enginePtr: Long = 0L

    /** Motion magnitude (0.0 = static, higher = more motion) */
    var motionMagnitude: Float = 0f
        private set

    /** Total frames processed */
    val frameCount: Long get() = if (enginePtr != 0L) nativeGetFrameCount(enginePtr) else 0L

    init {
        System.loadLibrary("openeyes")
        enginePtr = nativeCreate()
    }

    override fun close() {
        if (enginePtr != 0L) {
            nativeDestroy(enginePtr)
            enginePtr = 0L
        }
    }

    // MARK: - Frame input

    /**
     * Push a raw camera frame from CameraX ImageAnalysis or Camera2 ImageReader.
     *
     * HOT PATH. The YUV planes go straight to Rust via JNI. No Java-side pixel processing.
     *
     * On Meta Quest: same path. Passthrough API delivers frames as Image objects
     * with the same YUV_420_888 format.
     */
    fun pushFrame(cameraId: Int, image: Image, rotation: Int = 0) {
        if (enginePtr == 0L) return

        val width = image.width
        val height = image.height

        when (image.format) {
            android.graphics.ImageFormat.YUV_420_888 -> {
                // YUV420 — the standard Android camera output.
                // Pack Y + UV planes into a single buffer for the FFI.
                val yPlane = image.planes[0].buffer
                val uPlane = image.planes[1].buffer
                val vPlane = image.planes[2].buffer

                val ySize = yPlane.remaining()
                val uSize = uPlane.remaining()
                val vSize = vPlane.remaining()

                // Allocate once, reuse via ThreadLocal in production
                val packed = ByteBuffer.allocateDirect(ySize + uSize + vSize)
                packed.put(yPlane)
                packed.put(uPlane)
                packed.put(vPlane)
                packed.rewind()

                nativePushFrame(
                    enginePtr, cameraId,
                    packed, packed.remaining(),
                    width, height,
                    PIXEL_FORMAT_YUV420, rotation
                )
            }
            android.graphics.ImageFormat.FLEX_RGBA_8888 -> {
                val buffer = image.planes[0].buffer
                nativePushFrame(
                    enginePtr, cameraId,
                    buffer, buffer.remaining(),
                    width, height,
                    PIXEL_FORMAT_RGB8, rotation
                )
            }
        }

        motionMagnitude = nativeGetMotion(enginePtr)
    }

    /**
     * Push a raw byte buffer directly. For non-Image sources (screenshots, RTSP decoded frames).
     */
    fun pushFrameBuffer(
        cameraId: Int,
        buffer: ByteBuffer,
        width: Int, height: Int,
        format: Int,
        rotation: Int = 0
    ) {
        if (enginePtr == 0L) return
        nativePushFrame(enginePtr, cameraId, buffer, buffer.remaining(), width, height, format, rotation)
        motionMagnitude = nativeGetMotion(enginePtr)
    }

    // MARK: - AR Pose input

    /**
     * Set camera pose from ARCore (Pose / Camera.getDisplayOrientedPose()).
     * Column-major 4x4 float matrix — 16 floats.
     *
     * On Meta Quest: OVR SDK provides headset pose as a 4x4 matrix.
     * Same call, same format.
     */
    fun setCameraPose(cameraId: Int, transform: FloatArray) {
        if (enginePtr == 0L || transform.size != 16) return
        nativeSetCameraPose(enginePtr, cameraId, transform)
    }

    // MARK: - Camera intrinsics

    /**
     * Set camera intrinsics. Call once per camera.
     * On ARCore: extract from CameraIntrinsics.getFocalLength() / getPrincipalPoint()
     */
    fun setIntrinsics(
        cameraId: Int,
        fx: Double, fy: Double,
        cx: Double, cy: Double,
        width: Int, height: Int,
        k1: Double = 0.0, k2: Double = 0.0, k3: Double = 0.0
    ) {
        if (enginePtr == 0L) return
        nativeSetIntrinsics(enginePtr, cameraId, fx, fy, cx, cy, width, height, k1, k2, k3)
    }

    // MARK: - JNI native methods

    private external fun nativeCreate(): Long
    private external fun nativeDestroy(engine: Long)
    private external fun nativePushFrame(
        engine: Long, cameraId: Int,
        data: ByteBuffer, len: Int,
        width: Int, height: Int,
        format: Int, rotation: Int
    )
    private external fun nativeSetCameraPose(engine: Long, cameraId: Int, transform: FloatArray)
    private external fun nativeGetMotion(engine: Long): Float
    private external fun nativeGetFrameCount(engine: Long): Long
    private external fun nativeSetIntrinsics(
        engine: Long, cameraId: Int,
        fx: Double, fy: Double,
        cx: Double, cy: Double,
        width: Int, height: Int,
        k1: Double, k2: Double, k3: Double
    )

    companion object {
        const val PIXEL_FORMAT_BGRA8 = 0
        const val PIXEL_FORMAT_YUV420 = 1
        const val PIXEL_FORMAT_RGB8 = 2
        const val PIXEL_FORMAT_GRAY8 = 3
    }
}

/**
 * Abstract camera source — the engine consumes frames from any source.
 *
 * Known implementations:
 * - PhoneCameraSource: CameraX ImageAnalysis (phones/tablets)
 * - ARCameraSource: ARCore Session (AR-capable devices)
 * - QuestCameraSource: Passthrough API (Meta Quest headsets)
 * - RtspCameraSource: FFmpeg-decoded RTSP stream (IP cameras)
 */
interface OpenEyesCameraSource {
    val cameraId: Int
    fun start()
    fun stop()
}

// MARK: - CameraX camera source

// In a real implementation this would be:
//
// class PhoneCameraSource(
//     override val cameraId: Int,
//     private val engine: OpenEyesEngine,
//     private val lifecycleOwner: LifecycleOwner
// ) : OpenEyesCameraSource {
//
//     private val cameraProviderFuture = ProcessCameraProvider.getInstance(context)
//
//     override fun start() {
//         val imageAnalysis = ImageAnalysis.Builder()
//             .setTargetResolution(Size(1280, 720))
//             .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
//             .build()
//
//         imageAnalysis.setAnalyzer(executor) { imageProxy ->
//             // HOT PATH: image → Rust, no Java processing
//             imageProxy.image?.let { engine.pushFrame(cameraId, it) }
//             imageProxy.close()
//         }
//
//         cameraProvider.bindToLifecycle(lifecycleOwner, cameraSelector, imageAnalysis, preview)
//     }
// }
//
// QuestCameraSource follows the same pattern via OVR Passthrough API.
// The frame delivery contract is identical: Image → pushFrame → Rust.
