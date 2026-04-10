/// open_eyes — Flutter plugin for the open-eyes 3D scene understanding engine.
///
/// This is the UI LAYER. It does NOT process pixels. It does NOT run ML models.
/// It does NOT do computer vision. ALL of that is in Rust.
///
/// This plugin:
/// 1. Starts native camera capture (which feeds frames directly to Rust)
/// 2. Receives geometric results from Rust (motion magnitude, events, scene state)
/// 3. Provides Dart Streams for the Flutter UI to consume
///
/// The hot path (camera frame → Rust) never touches Dart.
/// Dart only sees the geometric OUTPUT.
library open_eyes;

import 'dart:async';
import 'package:flutter/services.dart';

/// Event types from the pipeline.
enum OEEventType {
  motion,
  cameraDrift,
  entityEntered,
  entityLeft,
  zoneCrossing,
  planeDetected,
  error,
}

/// A pipeline event — geometry, not pixels.
class OEEvent {
  final OEEventType type;
  final int cameraId;
  final int entityId;
  final double value;
  final List<double> position;
  final double timestamp;

  OEEvent({
    required this.type,
    required this.cameraId,
    this.entityId = 0,
    this.value = 0.0,
    this.position = const [0.0, 0.0, 0.0],
    this.timestamp = 0.0,
  });

  factory OEEvent.fromMap(Map<dynamic, dynamic> map) {
    return OEEvent(
      type: OEEventType.values[map['type'] as int],
      cameraId: map['cameraId'] as int,
      entityId: map['entityId'] as int? ?? 0,
      value: (map['value'] as num?)?.toDouble() ?? 0.0,
      position: (map['position'] as List?)?.cast<double>() ?? [0.0, 0.0, 0.0],
      timestamp: (map['timestamp'] as num?)?.toDouble() ?? 0.0,
    );
  }
}

/// The open-eyes engine — 3D scene understanding from camera feeds.
///
/// Usage:
/// ```dart
/// final engine = OpenEyes();
/// await engine.initialize();
/// await engine.startCamera();
///
/// // Listen to geometric events (not pixels!)
/// engine.events.listen((event) {
///   if (event.type == OEEventType.motion) {
///     print('Motion: ${event.value}');
///   }
/// });
///
/// // Read motion magnitude directly
/// final motion = await engine.motionMagnitude;
/// ```
class OpenEyes {
  static const MethodChannel _channel = MethodChannel('com.cambriantech.open_eyes/engine');
  static const EventChannel _eventChannel = EventChannel('com.cambriantech.open_eyes/events');

  StreamSubscription? _eventSubscription;
  final StreamController<OEEvent> _eventController = StreamController<OEEvent>.broadcast();

  /// Stream of pipeline events (motion, drift, entities, planes).
  /// All geometry — no pixels cross this boundary.
  Stream<OEEvent> get events => _eventController.stream;

  /// Initialize the engine. Must be called before startCamera.
  Future<void> initialize() async {
    await _channel.invokeMethod('initialize');

    // Subscribe to native event stream
    _eventSubscription = _eventChannel
        .receiveBroadcastStream()
        .listen((dynamic event) {
      if (event is Map) {
        _eventController.add(OEEvent.fromMap(event));
      }
    });
  }

  /// Start camera capture. Frames go directly from native camera API to Rust.
  /// Dart never sees the pixel data.
  ///
  /// [cameraId] — which camera to start (0 = back, 1 = front, 2+ = external)
  /// [resolution] — target resolution ('720p', '1080p', '4k')
  Future<void> startCamera({int cameraId = 0, String resolution = '720p'}) async {
    await _channel.invokeMethod('startCamera', {
      'cameraId': cameraId,
      'resolution': resolution,
    });
  }

  /// Stop camera capture.
  Future<void> stopCamera({int cameraId = 0}) async {
    await _channel.invokeMethod('stopCamera', {'cameraId': cameraId});
  }

  /// Get current motion magnitude (0.0 = static).
  /// Single float — cheapest possible query.
  Future<double> get motionMagnitude async {
    final result = await _channel.invokeMethod<double>('getMotion');
    return result ?? 0.0;
  }

  /// Get total frames processed.
  Future<int> get frameCount async {
    final result = await _channel.invokeMethod<int>('getFrameCount');
    return result ?? 0;
  }

  /// Set camera intrinsics (from ARKit/ARCore calibration data).
  Future<void> setIntrinsics({
    required int cameraId,
    required double fx,
    required double fy,
    required double cx,
    required double cy,
    required int width,
    required int height,
  }) async {
    await _channel.invokeMethod('setIntrinsics', {
      'cameraId': cameraId,
      'fx': fx, 'fy': fy, 'cx': cx, 'cy': cy,
      'width': width, 'height': height,
    });
  }

  /// Enable AR mode — starts ARKit/ARCore session and feeds pose data to engine.
  /// Returns false if AR is not available on this device.
  Future<bool> enableAR() async {
    final result = await _channel.invokeMethod<bool>('enableAR');
    return result ?? false;
  }

  /// Dispose the engine and release all resources.
  Future<void> dispose() async {
    _eventSubscription?.cancel();
    _eventController.close();
    await _channel.invokeMethod('dispose');
  }
}
