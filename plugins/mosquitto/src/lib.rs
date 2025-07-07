//! Mosquitto plugin entry points.
//!
//! This minimal implementation demonstrates how a Mosquitto plugin written in
//! Rust can expose the C symbols expected by the broker. The functions here
//! simply log messages using the `moqtail-core` crate.

use moqtail_core::hello;
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
pub struct MosquittoOpt {
    pub key: *const c_char,
    pub value: *const c_char,
}

/// Called when the plugin is loaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_init(
    _identifier: *mut c_void,
    _userdata: *mut *mut c_void,
    _options: *mut MosquittoOpt,
    _option_count: c_int,
) -> c_int {
    println!("[MoQTail] init: {}", hello());
    0
}

/// Called when the plugin is unloaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_cleanup(
    _userdata: *mut c_void,
    _options: *mut MosquittoOpt,
    _option_count: c_int,
) -> c_int {
    println!("[MoQTail] cleanup");
    0
}
