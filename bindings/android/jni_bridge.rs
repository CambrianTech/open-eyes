//! JNI bridge — connects Kotlin `external` methods to Rust C functions.
//!
//! This file is compiled into libopeneyes.so alongside the FFI crate.
//! The JNI methods call straight through to the C API — zero overhead wrapper.
//!
//! For Meta Quest: same .so, same JNI methods. The Quest is Android.

#![allow(non_snake_case)]

use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jlong, jint, jfloat, jdouble, jfloatArray};
use std::os::raw::c_void;

// Import the C API functions from open-eyes-ffi
extern "C" {
    fn oe_create() -> *mut c_void;
    fn oe_destroy(engine: *mut c_void);
    fn oe_push_frame(
        engine: *mut c_void, camera_id: u32,
        data: *const u8, len: usize,
        width: u32, height: u32,
        format: u32, rotation: i32,
    ) -> i32;
    fn oe_set_camera_pose(engine: *mut c_void, camera_id: u32, transform: *const f32) -> i32;
    fn oe_get_motion(engine: *const c_void) -> f32;
    fn oe_get_frame_count(engine: *const c_void) -> u64;
    fn oe_set_intrinsics(
        engine: *mut c_void, camera_id: u32,
        fx: f64, fy: f64, cx: f64, cy: f64,
        width: u32, height: u32,
        k1: f64, k2: f64, k3: f64,
    ) -> i32;
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeCreate(
    _env: JNIEnv, _class: JClass,
) -> jlong {
    unsafe { oe_create() as jlong }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeDestroy(
    _env: JNIEnv, _class: JClass, engine: jlong,
) {
    unsafe { oe_destroy(engine as *mut c_void) }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativePushFrame(
    env: JNIEnv, _class: JClass,
    engine: jlong, camera_id: jint,
    data: jni::objects::JByteBuffer, len: jint,
    width: jint, height: jint,
    format: jint, rotation: jint,
) {
    let ptr = match env.get_direct_buffer_address(&data) {
        Ok(p) => p,
        Err(_) => return,
    };

    unsafe {
        oe_push_frame(
            engine as *mut c_void,
            camera_id as u32,
            ptr.as_ptr(),
            len as usize,
            width as u32, height as u32,
            format as u32, rotation as i32,
        );
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeSetCameraPose(
    env: JNIEnv, _class: JClass,
    engine: jlong, camera_id: jint, transform: jfloatArray,
) {
    let mut buf = [0f32; 16];
    if env.get_float_array_region(transform, 0, &mut buf).is_err() {
        return;
    }
    unsafe {
        oe_set_camera_pose(engine as *mut c_void, camera_id as u32, buf.as_ptr());
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeGetMotion(
    _env: JNIEnv, _class: JClass, engine: jlong,
) -> jfloat {
    unsafe { oe_get_motion(engine as *const c_void) }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeGetFrameCount(
    _env: JNIEnv, _class: JClass, engine: jlong,
) -> jlong {
    unsafe { oe_get_frame_count(engine as *const c_void) as jlong }
}

#[no_mangle]
pub extern "system" fn Java_com_cambriantech_openeyes_OpenEyesEngine_nativeSetIntrinsics(
    _env: JNIEnv, _class: JClass,
    engine: jlong, camera_id: jint,
    fx: jdouble, fy: jdouble, cx: jdouble, cy: jdouble,
    width: jint, height: jint,
    k1: jdouble, k2: jdouble, k3: jdouble,
) {
    unsafe {
        oe_set_intrinsics(
            engine as *mut c_void, camera_id as u32,
            fx, fy, cx, cy,
            width as u32, height as u32,
            k1, k2, k3,
        );
    }
}
