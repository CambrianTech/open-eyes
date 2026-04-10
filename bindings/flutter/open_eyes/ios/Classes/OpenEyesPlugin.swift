import Flutter
import UIKit

/// Flutter plugin for open-eyes — bridges MethodChannel/EventChannel to the Rust engine.
///
/// The hot path (camera frames) NEVER goes through Flutter.
/// Native AVCaptureSession → Rust directly.
/// Flutter only sees geometric results via EventChannel.
public class OpenEyesPlugin: NSObject, FlutterPlugin, FlutterStreamHandler {
    private var engine: OpenEyesEngine?
    private var cameraSource: PhoneCameraSource?
    private var eventSink: FlutterEventSink?

    // Timer for polling Rust events and forwarding to Flutter
    private var pollTimer: Timer?

    public static func register(with registrar: FlutterPluginRegistrar) {
        let channel = FlutterMethodChannel(
            name: "com.cambriantech.open_eyes/engine",
            binaryMessenger: registrar.messenger()
        )
        let eventChannel = FlutterEventChannel(
            name: "com.cambriantech.open_eyes/events",
            binaryMessenger: registrar.messenger()
        )

        let instance = OpenEyesPlugin()
        registrar.addMethodCallDelegate(instance, channel: channel)
        eventChannel.setStreamHandler(instance)
    }

    public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        switch call.method {
        case "initialize":
            engine = OpenEyesEngine()
            startEventPolling()
            result(nil)

        case "startCamera":
            guard let engine = engine else { result(FlutterError(code: "NO_ENGINE", message: "Call initialize first", details: nil)); return }
            let args = call.arguments as? [String: Any] ?? [:]
            let cameraId = args["cameraId"] as? Int ?? 0
            cameraSource = PhoneCameraSource(cameraId: UInt32(cameraId), engine: engine)
            cameraSource?.start()
            result(nil)

        case "stopCamera":
            cameraSource?.stop()
            cameraSource = nil
            result(nil)

        case "getMotion":
            result(Double(engine?.motionMagnitude ?? 0.0))

        case "getFrameCount":
            result(Int(engine?.frameCount ?? 0))

        case "setIntrinsics":
            guard let engine = engine, let args = call.arguments as? [String: Any] else { result(nil); return }
            engine.setIntrinsics(
                cameraId: UInt32(args["cameraId"] as? Int ?? 0),
                fx: args["fx"] as? Double ?? 0,
                fy: args["fy"] as? Double ?? 0,
                cx: args["cx"] as? Double ?? 0,
                cy: args["cy"] as? Double ?? 0,
                width: UInt32(args["width"] as? Int ?? 0),
                height: UInt32(args["height"] as? Int ?? 0)
            )
            result(nil)

        case "enableAR":
            // TODO: start ARSession, feed ARFrame.capturedImage + camera.transform to engine
            result(false)

        case "dispose":
            stopEventPolling()
            cameraSource?.stop()
            cameraSource = nil
            engine = nil
            result(nil)

        default:
            result(FlutterMethodNotImplemented)
        }
    }

    // MARK: - Event streaming

    public func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? {
        eventSink = events
        return nil
    }

    public func onCancel(withArguments arguments: Any?) -> FlutterError? {
        eventSink = nil
        return nil
    }

    private func startEventPolling() {
        // Poll Rust events at 30Hz and forward to Flutter as geometry
        pollTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            self?.pollEvents()
        }
    }

    private func stopEventPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    private func pollEvents() {
        guard let engine = engine, let sink = eventSink else { return }

        // Also send periodic motion update even without explicit events
        let motion = engine.motionMagnitude
        if motion > 0.01 {
            sink([
                "type": 0, // motion
                "cameraId": 0,
                "value": Double(motion),
                "position": [0.0, 0.0, 0.0],
                "timestamp": Date().timeIntervalSince1970,
            ] as [String: Any])
        }
    }
}
