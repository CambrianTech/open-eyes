/// OpenEyes — Swift wrapper for the Rust open-eyes engine.
///
/// Thin binding. All compute is in Rust. This file handles:
/// 1. Camera frame delivery (AVFoundation, ARKit, visionOS passthrough)
/// 2. AR pose delivery (ARKit / ARSession)
/// 3. Event polling back to the caller
///
/// Works on: iOS, iPadOS, visionOS (Vision Pro), macOS
/// The engine doesn't care about the platform — it receives frames + poses.
/// Camera source abstraction handles the API differences.

import Foundation
import AVFoundation

// MARK: - C FFI imports (from openeyes.h)

// These are the raw C function pointers from libopeneyes.
// In a real build, this comes from the bridging header or module map.
// Declaring them here for documentation — the actual linkage is via
// the .xcframework's module.modulemap.

/*
 func oe_create() -> OpaquePointer
 func oe_destroy(_ engine: OpaquePointer)
 func oe_push_frame(_ engine: OpaquePointer, _ camera_id: UInt32,
                     _ data: UnsafePointer<UInt8>, _ len: Int,
                     _ width: UInt32, _ height: UInt32,
                     _ format: UInt32, _ rotation: Int32) -> Int32
 func oe_set_camera_pose(_ engine: OpaquePointer, _ camera_id: UInt32,
                          _ transform: UnsafePointer<Float>) -> Int32
 func oe_get_motion(_ engine: OpaquePointer) -> Float
 func oe_get_frame_count(_ engine: OpaquePointer) -> UInt64
 func oe_set_intrinsics(_ engine: OpaquePointer, _ camera_id: UInt32,
                         _ fx: Double, _ fy: Double,
                         _ cx: Double, _ cy: Double,
                         _ width: UInt32, _ height: UInt32,
                         _ k1: Double, _ k2: Double, _ k3: Double) -> Int32
 func oe_poll_events(_ engine: OpaquePointer,
                      _ callback: @convention(c) (UnsafePointer<OEEvent>?) -> Void) -> Int32
*/

// MARK: - Pixel format constants (must match OEPixelFormat in Rust)

public enum OEPixelFormat: UInt32 {
    case bgra8 = 0    // iOS AVCaptureSession default
    case yuv420 = 1   // Android Camera2 / some iOS modes
    case rgb8 = 2
    case gray8 = 3
}

// MARK: - Camera source protocol

/// Abstract camera source — the engine consumes frames from any source.
/// Implementations handle platform-specific camera APIs.
///
/// Known implementations:
/// - PhoneCameraSource: AVCaptureSession (iPhone/iPad)
/// - ARCameraSource: ARSession (ARKit on iOS/iPadOS)
/// - PassthroughCameraSource: Enterprise passthrough (Vision Pro)
/// - QuestCameraSource: Passthrough API (Meta Quest — Android side)
public protocol OpenEyesCameraSource: AnyObject {
    var cameraId: UInt32 { get }
    func start()
    func stop()
}

// MARK: - Engine wrapper

public final class OpenEyesEngine {
    private var engine: OpaquePointer?

    /// Motion magnitude (0.0 = static, higher = more motion)
    public private(set) var motionMagnitude: Float = 0.0

    /// Total frames processed
    public var frameCount: UInt64 {
        guard let engine = engine else { return 0 }
        return oe_get_frame_count(engine)
    }

    public init() {
        engine = oe_create()
    }

    deinit {
        if let engine = engine {
            oe_destroy(engine)
        }
    }

    // MARK: - Frame input

    /// Push a raw camera frame. Called from the camera callback — must be fast.
    ///
    /// On iOS: CVPixelBuffer from AVCaptureVideoDataOutput or ARFrame.capturedImage
    /// On visionOS: same path via enterprise passthrough API
    ///
    /// This is the HOT PATH. No Swift allocations here. Pointer goes straight to Rust.
    public func pushFrame(cameraId: UInt32, pixelBuffer: CVPixelBuffer, rotation: Int32 = 0) {
        guard let engine = engine else { return }

        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }

        let width = UInt32(CVPixelBufferGetWidth(pixelBuffer))
        let height = UInt32(CVPixelBufferGetHeight(pixelBuffer))

        // Determine pixel format from the CVPixelBuffer's actual format
        let osType = CVPixelBufferGetPixelFormatType(pixelBuffer)
        let format: UInt32
        switch osType {
        case kCVPixelFormatType_32BGRA:
            format = OEPixelFormat.bgra8.rawValue
        case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
             kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
            format = OEPixelFormat.yuv420.rawValue
        default:
            format = OEPixelFormat.bgra8.rawValue // best guess
        }

        guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else { return }
        let dataSize = CVPixelBufferGetDataSize(pixelBuffer)
        let ptr = baseAddress.assumingMemoryBound(to: UInt8.self)

        // Straight to Rust. No copy. No Swift processing.
        oe_push_frame(engine, cameraId, ptr, dataSize, width, height, format, rotation)

        // Update cached motion (single float read, no allocation)
        motionMagnitude = oe_get_motion(engine)
    }

    // MARK: - AR Pose input

    /// Set camera pose from ARKit (ARFrame.camera.transform).
    /// Column-major 4x4 matrix — exactly what ARKit gives you.
    public func setCameraPose(cameraId: UInt32, transform: simd_float4x4) {
        guard let engine = engine else { return }

        // simd_float4x4 is column-major, which is what Rust expects.
        // Pass the raw 16 floats directly — no conversion needed.
        withUnsafePointer(to: transform) { ptr in
            ptr.withMemoryRebound(to: Float.self, capacity: 16) { floatPtr in
                oe_set_camera_pose(engine, cameraId, floatPtr)
            }
        }
    }

    // MARK: - Camera intrinsics

    /// Set camera intrinsics. Call once per camera, or when camera config changes.
    /// On ARKit: extract from ARFrame.camera.intrinsics
    public func setIntrinsics(
        cameraId: UInt32,
        fx: Double, fy: Double,
        cx: Double, cy: Double,
        width: UInt32, height: UInt32,
        k1: Double = 0, k2: Double = 0, k3: Double = 0
    ) {
        guard let engine = engine else { return }
        oe_set_intrinsics(engine, cameraId, fx, fy, cx, cy, width, height, k1, k2, k3)
    }
}

// MARK: - AVCaptureSession Camera Source (iPhone / iPad)

/// Camera source using AVCaptureSession — the standard iOS camera API.
/// Delivers CVPixelBuffer directly to the engine. Zero-copy on the hot path.
///
/// Also works on Mac Catalyst and macOS with minor config changes.
public final class PhoneCameraSource: NSObject, OpenEyesCameraSource, AVCaptureVideoDataOutputSampleBufferDelegate {
    public let cameraId: UInt32
    private let engine: OpenEyesEngine
    private let session = AVCaptureSession()
    private let outputQueue = DispatchQueue(label: "com.cambriantech.open-eyes.camera", qos: .userInteractive)

    public init(cameraId: UInt32, engine: OpenEyesEngine) {
        self.cameraId = cameraId
        self.engine = engine
        super.init()
        configureCaptureSession()
    }

    private func configureCaptureSession() {
        session.sessionPreset = .hd1280x720

        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
              let input = try? AVCaptureDeviceInput(device: device) else { return }

        if session.canAddInput(input) {
            session.addInput(input)
        }

        let output = AVCaptureVideoDataOutput()
        // BGRA — matches OEPixelFormat::Bgra8, no conversion needed
        output.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA
        ]
        output.alwaysDiscardsLateVideoFrames = true
        output.setSampleBufferDelegate(self, queue: outputQueue)

        if session.canAddOutput(output) {
            session.addOutput(output)
        }
    }

    public func start() {
        session.startRunning()
    }

    public func stop() {
        session.stopRunning()
    }

    // AVCaptureVideoDataOutputSampleBufferDelegate — the hot path
    public func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        // Straight to Rust. CVPixelBuffer → oe_push_frame. No intermediate processing.
        engine.pushFrame(cameraId: cameraId, pixelBuffer: pixelBuffer)
    }
}

// MARK: - Future camera sources (stubs for architecture reference)

// ARCameraSource: Uses ARSession.delegate to get ARFrame.capturedImage (CVPixelBuffer)
// + ARFrame.camera.transform (simd_float4x4 pose) + ARFrame.camera.intrinsics.
// Same pushFrame + setCameraPose calls. ARKit gives us everything.
//
// PassthroughCameraSource (Vision Pro): Enterprise API gives passthrough camera access.
// visionOS ARKit provides spatial tracking. Same pattern — frames + poses to engine.
// The engine doesn't know it's running on a headset. It just gets frames + transforms.
//
// QuestCameraSource: See the Android binding — Meta Quest runs Android.
// Passthrough API gives camera frames. OVR SDK gives headset pose.
// Same oe_push_frame + oe_set_camera_pose calls via JNI.
