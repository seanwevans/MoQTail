//! Mosquitto plugin entry points.
//!
//! The plugin parses `plugin_opt_selector` entries from the broker
//! configuration, compiles them using `moqtail-core`, and registers a publish
//! callback.  Incoming messages are filtered before they reach the broker
//! clients.

use moqtail_core::{ast::{Axis, Segment, Selector, Step}, compile};
use std::{ffi::CStr, os::raw::{c_char, c_int, c_void}};

const MOSQ_EVT_MESSAGE: c_int = 7;
const MOSQ_ERR_SUCCESS: c_int = 0;

#[repr(C)]
pub struct MosquittoEvtMessage {
    _future: *mut c_void,
    _client: *mut c_void,
    topic: *mut c_char,
    _payload: *mut c_void,
    _properties: *mut c_void,
    _reason_string: *mut c_char,
    _payloadlen: u32,
    _qos: u8,
    _reason_code: u8,
    _retain: bool,
}

extern "C" {
    #[cfg_attr(not(test), link_name = "mosquitto_callback_register")]
    fn mosquitto_callback_register(
        identifier: *mut c_void,
        event: c_int,
        cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        event_data: *const c_void,
        userdata: *mut c_void,
    ) -> c_int;

    #[cfg_attr(not(test), link_name = "mosquitto_callback_unregister")]
    fn mosquitto_callback_unregister(
        identifier: *mut c_void,
        event: c_int,
        cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        event_data: *const c_void,
    ) -> c_int;
}

pub struct PluginContext {
    selectors: Vec<Selector>,
}

fn match_segment(seg: &Segment, value: &str) -> bool {
    match seg {
        Segment::Literal(lit) => lit == value,
        Segment::Plus => true,
        Segment::Hash => true,
    }
}

fn match_steps(steps: &[Step], segments: &[&str]) -> bool {
    if steps.is_empty() {
        return segments.is_empty();
    }

    let step = &steps[0];

    if matches!(step.segment, Segment::Hash) {
        return true;
    }

    match step.axis {
        Axis::Child => {
            if segments.is_empty() {
                return false;
            }
            if match_segment(&step.segment, segments[0]) {
                match_steps(&steps[1..], &segments[1..])
            } else {
                false
            }
        }
        Axis::Descendant => {
            for i in 0..segments.len() {
                if match_segment(&step.segment, segments[i])
                    && match_steps(&steps[1..], &segments[i + 1..])
                {
                    return true;
                }
            }
            false
        }
    }
}

fn matches_selector(sel: &Selector, topic: &str) -> bool {
    let parts: Vec<&str> = if topic.is_empty() {
        Vec::new()
    } else {
        topic.split('/').collect()
    };
    match_steps(&sel.0, &parts)
}

extern "C" fn on_message(_: c_int, event_data: *mut c_void, userdata: *mut c_void) -> c_int {
    unsafe {
        let ctx = &*(userdata as *mut PluginContext);
        let msg = &*(event_data as *mut MosquittoEvtMessage);
        if msg.topic.is_null() {
            return MOSQ_ERR_SUCCESS;
        }
        let topic = match CStr::from_ptr(msg.topic).to_str() {
            Ok(t) => t,
            Err(_) => return 1,
        };
        for sel in &ctx.selectors {
            if matches_selector(sel, topic) {
                return MOSQ_ERR_SUCCESS;
            }
        }
    }
    1
}

#[repr(C)]
pub struct MosquittoOpt {
    pub key: *const c_char,
    pub value: *const c_char,
}

/// Called when the plugin is loaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_init(
    identifier: *mut c_void,
    userdata: *mut *mut c_void,
    options: *mut MosquittoOpt,
    option_count: c_int,
) -> c_int {
    let slice = std::slice::from_raw_parts(options, option_count as usize);
    let mut selectors = Vec::new();

    for opt in slice.iter() {
        if opt.key.is_null() || opt.value.is_null() {
            continue;
        }
        let key = CStr::from_ptr(opt.key).to_string_lossy();
        if key == "selector" {
            let val = CStr::from_ptr(opt.value).to_string_lossy();
            match compile(&val) {
                Ok(sel) => selectors.push(sel),
                Err(e) => eprintln!("[MoQTail] selector error: {}", e),
            }
        }
    }

    let ctx = Box::new(PluginContext { selectors });
    let ctx_ptr = Box::into_raw(ctx) as *mut c_void;
    *userdata = ctx_ptr;

    mosquitto_callback_register(
        identifier,
        MOSQ_EVT_MESSAGE,
        Some(on_message),
        std::ptr::null(),
        ctx_ptr,
    )
}

/// Called when the plugin is unloaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_cleanup(
    identifier: *mut c_void,
    userdata: *mut c_void,
    _options: *mut MosquittoOpt,
    _option_count: c_int,
) -> c_int {
    let _ = mosquitto_callback_unregister(identifier, MOSQ_EVT_MESSAGE, Some(on_message), std::ptr::null());
    if !userdata.is_null() {
        drop(Box::from_raw(userdata as *mut PluginContext));
    }
    MOSQ_ERR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    static mut REGISTERED: Option<(extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int, *mut c_void)> = None;

    #[no_mangle]
    unsafe extern "C" fn mosquitto_callback_register(
        _identifier: *mut c_void,
        _event: c_int,
        cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        _event_data: *const c_void,
        userdata: *mut c_void,
    ) -> c_int {
        if let Some(f) = cb_func {
            REGISTERED = Some((f, userdata));
        }
        MOSQ_ERR_SUCCESS
    }

    #[no_mangle]
    unsafe extern "C" fn mosquitto_callback_unregister(
        _identifier: *mut c_void,
        _event: c_int,
        _cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        _event_data: *const c_void,
    ) -> c_int {
        REGISTERED = None;
        MOSQ_ERR_SUCCESS
    }

    #[test]
    fn filter_logic() {
        unsafe {
            let key = CString::new("selector").unwrap();
            let val = CString::new("/foo/+" ).unwrap();
            let mut opt = MosquittoOpt { key: key.as_ptr(), value: val.as_ptr() };
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1), MOSQ_ERR_SUCCESS);
            let (cb, ctx) = REGISTERED.expect("callback registered");

            let topic1 = CString::new("foo/bar").unwrap();
            let mut msg = MosquittoEvtMessage {
                _future: std::ptr::null_mut(),
                _client: std::ptr::null_mut(),
                topic: topic1.as_ptr() as *mut c_char,
                _payload: std::ptr::null_mut(),
                _properties: std::ptr::null_mut(),
                _reason_string: std::ptr::null_mut(),
                _payloadlen: 0,
                _qos: 0,
                _reason_code: 0,
                _retain: false,
            };

            assert_eq!(cb(MOSQ_EVT_MESSAGE, &mut msg as *mut _ as *mut c_void, ctx), MOSQ_ERR_SUCCESS);

            let topic2 = CString::new("baz/qux").unwrap();
            msg.topic = topic2.as_ptr() as *mut c_char;
            assert_eq!(cb(MOSQ_EVT_MESSAGE, &mut msg as *mut _ as *mut c_void, ctx), 1);

            mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
            assert!(REGISTERED.is_none());
        }
    }
}
