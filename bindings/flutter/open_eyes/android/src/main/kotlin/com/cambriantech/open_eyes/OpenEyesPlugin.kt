package com.cambriantech.open_eyes

import android.os.Handler
import android.os.Looper
import com.cambriantech.openeyes.OpenEyesEngine
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * Flutter plugin for open-eyes — bridges MethodChannel/EventChannel to the Rust engine.
 *
 * The hot path (camera frames) NEVER goes through Flutter.
 * Native CameraX → Rust directly via JNI.
 * Flutter only sees geometric results via EventChannel.
 *
 * Works on: Android phones, tablets, Meta Quest headsets.
 */
class OpenEyesPlugin : FlutterPlugin, MethodChannel.MethodCallHandler, EventChannel.StreamHandler {
    private var channel: MethodChannel? = null
    private var eventChannel: EventChannel? = null
    private var eventSink: EventChannel.EventSink? = null
    private var engine: OpenEyesEngine? = null
    private val handler = Handler(Looper.getMainLooper())
    private var pollRunnable: Runnable? = null

    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel = MethodChannel(binding.binaryMessenger, "com.cambriantech.open_eyes/engine").also {
            it.setMethodCallHandler(this)
        }
        eventChannel = EventChannel(binding.binaryMessenger, "com.cambriantech.open_eyes/events").also {
            it.setStreamHandler(this)
        }
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel?.setMethodCallHandler(null)
        eventChannel?.setStreamHandler(null)
        stopEventPolling()
        engine?.close()
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "initialize" -> {
                engine = OpenEyesEngine()
                startEventPolling()
                result.success(null)
            }
            "startCamera" -> {
                // TODO: create CameraX ImageAnalysis, bind to engine
                // val cameraId = call.argument<Int>("cameraId") ?: 0
                result.success(null)
            }
            "stopCamera" -> {
                result.success(null)
            }
            "getMotion" -> {
                result.success((engine?.motionMagnitude ?: 0f).toDouble())
            }
            "getFrameCount" -> {
                result.success(engine?.frameCount?.toInt() ?: 0)
            }
            "setIntrinsics" -> {
                engine?.setIntrinsics(
                    cameraId = call.argument<Int>("cameraId") ?: 0,
                    fx = call.argument<Double>("fx") ?: 0.0,
                    fy = call.argument<Double>("fy") ?: 0.0,
                    cx = call.argument<Double>("cx") ?: 0.0,
                    cy = call.argument<Double>("cy") ?: 0.0,
                    width = call.argument<Int>("width") ?: 0,
                    height = call.argument<Int>("height") ?: 0,
                )
                result.success(null)
            }
            "enableAR" -> {
                // TODO: start ARCore session
                result.success(false)
            }
            "dispose" -> {
                stopEventPolling()
                engine?.close()
                engine = null
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    // Event streaming to Flutter
    override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
        eventSink = events
    }

    override fun onCancel(arguments: Any?) {
        eventSink = null
    }

    private fun startEventPolling() {
        pollRunnable = object : Runnable {
            override fun run() {
                pollEvents()
                handler.postDelayed(this, 33) // ~30Hz
            }
        }
        handler.post(pollRunnable!!)
    }

    private fun stopEventPolling() {
        pollRunnable?.let { handler.removeCallbacks(it) }
        pollRunnable = null
    }

    private fun pollEvents() {
        val sink = eventSink ?: return
        val motion = engine?.motionMagnitude ?: return

        if (motion > 0.01f) {
            sink.success(mapOf(
                "type" to 0, // motion
                "cameraId" to 0,
                "value" to motion.toDouble(),
                "position" to listOf(0.0, 0.0, 0.0),
                "timestamp" to (System.currentTimeMillis() / 1000.0),
            ))
        }
    }
}
