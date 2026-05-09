//! Mosquitto plugin entry points.
//!
//! The plugin parses `plugin_opt_selector` entries from the broker
//! configuration, compiles them using `moqtail-core`, and registers a publish
//! callback.  Incoming messages are filtered before they reach the broker
//! clients.

use moqtail_core::{compile, Matcher, Message};
use serde_json::Value as JsonValue;
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::CStr,
    os::raw::{c_int, c_void},
    slice,
};

// Bindings generated in build.rs
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

const MOSQ_EVT_MESSAGE: c_int = 7;
const MOSQ_ERR_SUCCESS: c_int = 0;
const MOSQ_ERR_PLUGIN_DEFER: c_int = 17;

// Use generated types `mosquitto_evt_message` and `mosquitto_opt`

pub struct PluginContext {
    matchers: Vec<Matcher>,
}

extern "C" fn on_message(_: c_int, event_data: *mut c_void, userdata: *mut c_void) -> c_int {
    if userdata.is_null() || event_data.is_null() {
        return MOSQ_ERR_PLUGIN_DEFER;
    }

    unsafe {
        let ctx = &*(userdata as *mut PluginContext);
        let msg = &*(event_data as *mut mosquitto_evt_message);
        if msg.topic.is_null() {
            return MOSQ_ERR_SUCCESS;
        }
        let topic = match CStr::from_ptr(msg.topic).to_str() {
            Ok(t) => t,
            Err(_) => return MOSQ_ERR_PLUGIN_DEFER,
        };

        let mut headers = HashMap::new();
        headers.insert(Cow::Borrowed("qos"), Cow::Owned(msg.qos.to_string()));
        headers.insert(
            Cow::Borrowed("retained"),
            Cow::Borrowed(if msg.retain { "true" } else { "false" }),
        );

        let payload = if !msg.payload.is_null() && msg.payloadlen > 0 {
            let bytes = slice::from_raw_parts(msg.payload as *const u8, msg.payloadlen as usize);
            match serde_json::from_slice::<JsonValue>(bytes) {
                Ok(j) => Some(j),
                Err(e) => {
                    eprintln!("[MoQTail] payload JSON parse error: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let m = Message {
            topic,
            headers,
            payload,
        };
        for matcher in &ctx.matchers {
            if matcher.matches(&m) {
                return MOSQ_ERR_SUCCESS;
            }
        }
    }
    MOSQ_ERR_PLUGIN_DEFER
}

// Generated bindings provide `mosquitto_opt`

/// Called when the plugin is loaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_init(
    identifier: *mut c_void,
    userdata: *mut *mut c_void,
    options: *mut mosquitto_opt,
    option_count: c_int,
) -> c_int {
    if userdata.is_null() || option_count < 0 {
        return MOSQ_ERR_PLUGIN_DEFER;
    }

    let slice = if option_count == 0 {
        &[]
    } else {
        if options.is_null() {
            return MOSQ_ERR_PLUGIN_DEFER;
        }
        std::slice::from_raw_parts(options, option_count as usize)
    };
    let mut matchers = Vec::new();

    for opt in slice.iter() {
        if opt.key.is_null() || opt.value.is_null() {
            continue;
        }
        let key = CStr::from_ptr(opt.key).to_string_lossy();
        if key == "selector" || key == "plugin_opt_selector" {
            let val = CStr::from_ptr(opt.value).to_string_lossy();
            match compile(&val) {
                Ok(sel) => matchers.push(Matcher::new(sel)),
                Err(e) => eprintln!("[MoQTail] selector error: {}", e),
            }
        }
    }

    let ctx = Box::new(PluginContext { matchers });
    let ctx_ptr = Box::into_raw(ctx) as *mut c_void;

    let rc = mosquitto_callback_register(
        identifier as *mut mosquitto_plugin_id_t,
        MOSQ_EVT_MESSAGE,
        Some(on_message),
        std::ptr::null(),
        ctx_ptr,
    );
    if rc != MOSQ_ERR_SUCCESS {
        drop(Box::from_raw(ctx_ptr as *mut PluginContext));
        return rc;
    }

    *userdata = ctx_ptr;
    MOSQ_ERR_SUCCESS
}

/// Called when the plugin is unloaded.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_cleanup(
    identifier: *mut c_void,
    userdata: *mut c_void,
    _options: *mut mosquitto_opt,
    _option_count: c_int,
) -> c_int {
    let _ = mosquitto_callback_unregister(
        identifier as *mut mosquitto_plugin_id_t,
        MOSQ_EVT_MESSAGE,
        Some(on_message),
        std::ptr::null(),
    );
    if !userdata.is_null() {
        drop(Box::from_raw(userdata as *mut PluginContext));
    }
    MOSQ_ERR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::raw::c_char;

    #[no_mangle]
    pub static mut REGISTERED: Option<(
        extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int,
        *mut c_void,
    )> = None;
    pub static mut REGISTER_RC: c_int = MOSQ_ERR_SUCCESS;

    #[no_mangle]
    unsafe extern "C" fn mosquitto_callback_register(
        _identifier: *mut c_void,
        _event: c_int,
        cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        _event_data: *const c_void,
        userdata: *mut c_void,
    ) -> c_int {
        let rc = REGISTER_RC;
        if rc == MOSQ_ERR_SUCCESS {
            if let Some(f) = cb_func {
                REGISTERED = Some((f, userdata));
            }
        }
        rc
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
    fn on_message_rejects_null_callback_data() {
        let topic = CString::new("foo/bar").unwrap();
        let mut msg = mosquitto_evt_message {
            future: std::ptr::null_mut(),
            client: std::ptr::null_mut(),
            topic: topic.as_ptr() as *mut c_char,
            payload: std::ptr::null_mut(),
            properties: std::ptr::null_mut(),
            reason_string: std::ptr::null_mut(),
            payloadlen: 0,
            qos: 0,
            reason_code: 0,
            retain: false,
            future2: [std::ptr::null_mut(); 4],
        };
        let mut ctx = PluginContext {
            matchers: Vec::new(),
        };

        assert_eq!(
            on_message(
                MOSQ_EVT_MESSAGE,
                std::ptr::null_mut(),
                &mut ctx as *mut _ as *mut c_void
            ),
            MOSQ_ERR_PLUGIN_DEFER
        );
        assert_eq!(
            on_message(
                MOSQ_EVT_MESSAGE,
                &mut msg as *mut _ as *mut c_void,
                std::ptr::null_mut(),
            ),
            MOSQ_ERR_PLUGIN_DEFER
        );
    }

    #[test]
    fn init_accepts_zero_options_and_rejects_invalid_inputs() {
        unsafe {
            REGISTERED = None;
            REGISTER_RC = MOSQ_ERR_SUCCESS;
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, std::ptr::null_mut(), 0,),
                MOSQ_ERR_SUCCESS
            );
            assert!(!userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_some());
            mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, std::ptr::null_mut(), 1),
                MOSQ_ERR_PLUGIN_DEFER
            );
            assert_eq!(
                mosquitto_plugin_init(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                ),
                MOSQ_ERR_PLUGIN_DEFER
            );
            assert_eq!(
                mosquitto_plugin_init(
                    std::ptr::null_mut(),
                    &mut userdata,
                    std::ptr::null_mut(),
                    -1,
                ),
                MOSQ_ERR_PLUGIN_DEFER
            );
        }
    }

    #[test]
    fn init_does_not_write_userdata_when_registration_fails() {
        unsafe {
            REGISTERED = None;
            REGISTER_RC = MOSQ_ERR_PLUGIN_DEFER;
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, std::ptr::null_mut(), 0),
                MOSQ_ERR_PLUGIN_DEFER
            );
            assert!(userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_none());

            REGISTER_RC = MOSQ_ERR_SUCCESS;
        }
    }

    #[test]
    fn filter_logic() {
        unsafe {
            REGISTER_RC = MOSQ_ERR_SUCCESS;
            let key = CString::new("selector").unwrap();
            let val = CString::new("/foo/+").unwrap();
            let mut opt = mosquitto_opt {
                key: key.as_ptr() as *mut c_char,
                value: val.as_ptr() as *mut c_char,
            };
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1),
                MOSQ_ERR_SUCCESS
            );
            let (cb, ctx) = REGISTERED.expect("callback registered");

            let topic1 = CString::new("foo/bar").unwrap();
            let payload1 = CString::new("{\"temp\":1}").unwrap();
            let mut msg = mosquitto_evt_message {
                future: std::ptr::null_mut(),
                client: std::ptr::null_mut(),
                topic: topic1.as_ptr() as *mut c_char,
                payload: payload1.as_ptr() as *mut c_void,
                properties: std::ptr::null_mut(),
                reason_string: std::ptr::null_mut(),
                payloadlen: payload1.as_bytes().len() as u32,
                qos: 0,
                reason_code: 0,
                retain: false,
                future2: [std::ptr::null_mut(); 4],
            };

            assert_eq!(
                cb(MOSQ_EVT_MESSAGE, &mut msg as *mut _ as *mut c_void, ctx),
                MOSQ_ERR_SUCCESS
            );

            let topic2 = CString::new("baz/qux").unwrap();
            msg.topic = topic2.as_ptr() as *mut c_char;
            assert_eq!(
                cb(MOSQ_EVT_MESSAGE, &mut msg as *mut _ as *mut c_void, ctx),
                MOSQ_ERR_PLUGIN_DEFER
            );

            let topic3 = CString::new("foo/baz").unwrap();
            let bad_payload = CString::new("{temp:}").unwrap();
            msg.topic = topic3.as_ptr() as *mut c_char;
            msg.payload = bad_payload.as_ptr() as *mut c_void;
            msg.payloadlen = bad_payload.as_bytes().len() as u32;
            assert_eq!(
                cb(MOSQ_EVT_MESSAGE, &mut msg as *mut _ as *mut c_void, ctx),
                MOSQ_ERR_SUCCESS
            );

            mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
            let registered = REGISTERED;
            assert!(registered.is_none());
        }
    }
}
