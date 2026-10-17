//! Mosquitto plugin entry points.
//!
//! The plugin parses `plugin_opt_selector` entries from the broker
//! configuration, compiles them using `moqtail-core`, and registers a publish
//! callback. Every incoming PUBLISH is checked against the selectors; one that
//! matches none of them is denied before it is routed to any subscriber.

use moqtail_core::{ast::Field, compile, payload, Matcher, Message};
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::CStr,
    os::raw::{c_int, c_void},
    slice,
    sync::atomic::{AtomicBool, Ordering},
};

// Bindings generated in build.rs
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

const MOSQ_EVT_MESSAGE: c_int = 7;
const MOSQ_ERR_SUCCESS: c_int = 0;
const MOSQ_ERR_INVAL: c_int = 3;
const MOSQ_ERR_PLUGIN_DEFER: c_int = 17;
const MOSQ_ERR_ACL_DENIED: c_int = 12;

pub struct PluginContext {
    identifier: *mut mosquitto_plugin_id_t,
    matchers: Vec<Matcher>,
    /// Per matcher, whether its selector reads the payload.
    reads_payload: Vec<bool>,
    /// Whether any selector reads a header; when none does the header map is
    /// left empty.
    needs_headers: bool,
    /// Set after the first payload that could not be decoded has been
    /// reported, so a stream of opaque payloads cannot flood the log.
    decode_error_reported: AtomicBool,
}

impl PluginContext {
    fn new(identifier: *mut mosquitto_plugin_id_t, matchers: Vec<Matcher>) -> Self {
        let fields = || matchers.iter().flat_map(|m| m.selector().fields());
        let needs_headers = fields().any(|f| matches!(f, Field::Header(_)));
        let reads_payload: Vec<bool> = matchers
            .iter()
            .map(|m| m.selector().needs_payload())
            .collect();
        Self {
            identifier,
            matchers,
            reads_payload,
            needs_headers,
            decode_error_reported: AtomicBool::new(false),
        }
    }
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
        // Mosquitto validates topics as UTF-8 before this point, so this only
        // guards against a broker that does not. Deny rather than defer: a
        // non-success code other than ACL_DENIED disconnects the publisher.
        let Ok(topic) = CStr::from_ptr(msg.topic).to_str() else {
            return MOSQ_ERR_ACL_DENIED;
        };

        let mut headers = HashMap::new();
        if ctx.needs_headers {
            let qos = match msg.qos {
                0 => Cow::Borrowed("0"),
                1 => Cow::Borrowed("1"),
                2 => Cow::Borrowed("2"),
                other => Cow::Owned(other.to_string()),
            };
            headers.insert(Cow::Borrowed("qos"), qos);
            headers.insert(
                Cow::Borrowed("retained"),
                Cow::Borrowed(if msg.retain { "true" } else { "false" }),
            );
        }

        let mut m = Message {
            topic,
            headers,
            payload: None,
        };
        // First pass without the payload. A match here is final, since a
        // payload can only make more predicates true. Meanwhile note whether
        // any selector could still match once the payload is known; if none
        // can, the payload is never decoded.
        let mut payload_could_help = false;
        for (matcher, &reads_payload) in ctx.matchers.iter().zip(&ctx.reads_payload) {
            if matcher.matches(&m) {
                return MOSQ_ERR_SUCCESS;
            }
            payload_could_help |= reads_payload && matcher.may_match(&m);
        }
        if !payload_could_help || msg.payload.is_null() || msg.payloadlen == 0 {
            return MOSQ_ERR_ACL_DENIED;
        }

        let bytes = slice::from_raw_parts(msg.payload as *const u8, msg.payloadlen as usize);
        match payload::from_bytes(bytes) {
            Ok(value) => m.payload = Some(value),
            Err(e) => {
                // A payload that is neither JSON nor CBOR is ordinary traffic,
                // not a fault: payload predicates just don't match it. Say so
                // once so a misconfiguration is visible.
                if !ctx.decode_error_reported.swap(true, Ordering::Relaxed) {
                    eprintln!(
                        "[MoQTail] payload on '{topic}' is neither JSON nor CBOR ({e}); \
                         payload predicates will not match such messages. \
                         Further decode failures are not reported."
                    );
                }
                return MOSQ_ERR_ACL_DENIED;
            }
        }
        let second_pass = ctx.matchers.iter().zip(&ctx.reads_payload);
        if second_pass
            .into_iter()
            .any(|(matcher, &reads)| reads && matcher.matches(&m))
        {
            return MOSQ_ERR_SUCCESS;
        }
    }
    MOSQ_ERR_ACL_DENIED
}

// Generated bindings provide `mosquitto_opt`

/// Tells Mosquitto 2.x which plugin interface this library implements.
/// Without this export the broker refuses to load the library.
#[no_mangle]
pub extern "C" fn mosquitto_plugin_version(_count: c_int, _versions: *const c_int) -> c_int {
    5
}

/// Called when the plugin is loaded.
///
/// # Safety
///
/// Mosquitto must pass a valid `userdata` out-pointer and, when `option_count`
/// is greater than zero, `options` must point to `option_count` initialised
/// [`mosquitto_opt`] values whose `key`/`value` members are either null or
/// NUL-terminated C strings. On success the plugin writes an owned context
/// pointer to `*userdata` which must be handed back to
/// [`mosquitto_plugin_cleanup`] exactly once.
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
    let mut failed = false;

    for opt in slice.iter() {
        if opt.key.is_null() || opt.value.is_null() {
            continue;
        }
        let key = CStr::from_ptr(opt.key).to_string_lossy();
        if key == "selector" || key == "plugin_opt_selector" {
            let val = CStr::from_ptr(opt.value).to_string_lossy();
            match compile(val.trim()) {
                Ok(sel) => matchers.push(Matcher::new(sel)),
                Err(e) => {
                    eprintln!("[MoQTail] selector {:?} does not compile: {e}", val.trim());
                    failed = true;
                }
            }
        }
    }

    // The selectors are an allowlist, so loading with one missing would
    // silently drop the traffic it was meant to admit, and loading with none
    // would drop everything. Refuse to start instead.
    if failed {
        eprintln!("[MoQTail] refusing to load: fix the selector(s) above");
        return MOSQ_ERR_INVAL;
    }
    if matchers.is_empty() {
        eprintln!("[MoQTail] refusing to load: no plugin_opt_selector configured");
        return MOSQ_ERR_INVAL;
    }

    let ctx = Box::new(PluginContext::new(
        identifier as *mut mosquitto_plugin_id_t,
        matchers,
    ));
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
        if !userdata.is_null() {
            *userdata = std::ptr::null_mut();
        }
        return rc;
    }

    if !userdata.is_null() {
        *userdata = ctx_ptr;
    }

    rc
}

/// Called when the plugin is unloaded.
///
/// # Safety
///
/// `userdata` must be either null or the pointer previously produced by
/// [`mosquitto_plugin_init`], not yet passed to this function. The pointer is
/// consumed and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn mosquitto_plugin_cleanup(
    userdata: *mut c_void,
    _options: *mut mosquitto_opt,
    _option_count: c_int,
) -> c_int {
    if !userdata.is_null() {
        let ctx = Box::from_raw(userdata as *mut PluginContext);
        let _ = mosquitto_callback_unregister(
            ctx.identifier,
            MOSQ_EVT_MESSAGE,
            Some(on_message),
            std::ptr::null(),
        );
    }
    MOSQ_ERR_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static REGISTER_RESULT: AtomicI32 = AtomicI32::new(MOSQ_ERR_SUCCESS);

    #[no_mangle]
    pub static mut REGISTERED: Option<(
        extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int,
        *mut c_void,
    )> = None;

    #[no_mangle]
    unsafe extern "C" fn mosquitto_callback_register(
        _identifier: *mut c_void,
        _event: c_int,
        cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
        _event_data: *const c_void,
        userdata: *mut c_void,
    ) -> c_int {
        let rc = REGISTER_RESULT.load(Ordering::SeqCst);
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
    fn init_clears_userdata_when_callback_registration_fails() {
        let _guard = TEST_LOCK.lock().unwrap();
        REGISTER_RESULT.store(MOSQ_ERR_PLUGIN_DEFER, Ordering::SeqCst);
        unsafe {
            REGISTERED = None;
            let key = CString::new("selector").unwrap();
            let val = CString::new("/foo/+").unwrap();
            let mut opt = mosquitto_opt {
                key: key.as_ptr() as *mut c_char,
                value: val.as_ptr() as *mut c_char,
            };
            let mut userdata = std::ptr::dangling_mut::<c_void>();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1),
                MOSQ_ERR_PLUGIN_DEFER
            );
            assert!(userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_none());
        }
        REGISTER_RESULT.store(MOSQ_ERR_SUCCESS, Ordering::SeqCst);
    }

    #[test]
    fn on_message_rejects_null_callback_data() {
        let _guard = TEST_LOCK.lock().unwrap();
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
        let mut ctx = PluginContext::new(std::ptr::null_mut(), Vec::new());

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
    fn init_refuses_zero_selectors_and_invalid_inputs() {
        let _guard = TEST_LOCK.lock().unwrap();
        REGISTER_RESULT.store(MOSQ_ERR_SUCCESS, Ordering::SeqCst);
        unsafe {
            REGISTERED = None;
            let mut userdata: *mut c_void = std::ptr::null_mut();

            // With no selector the allowlist would deny every message.
            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, std::ptr::null_mut(), 0,),
                MOSQ_ERR_INVAL
            );
            assert!(userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_none());

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
        let _guard = TEST_LOCK.lock().unwrap();
        REGISTER_RESULT.store(MOSQ_ERR_PLUGIN_DEFER, Ordering::SeqCst);
        unsafe {
            REGISTERED = None;
            let key = CString::new("selector").unwrap();
            let val = CString::new("/foo").unwrap();
            let mut opt = mosquitto_opt {
                key: key.as_ptr() as *mut c_char,
                value: val.as_ptr() as *mut c_char,
            };
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1),
                MOSQ_ERR_PLUGIN_DEFER
            );
            assert!(userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_none());
        }
        REGISTER_RESULT.store(MOSQ_ERR_SUCCESS, Ordering::SeqCst);
    }

    #[test]
    fn init_refuses_a_selector_that_does_not_compile() {
        let _guard = TEST_LOCK.lock().unwrap();
        REGISTER_RESULT.store(MOSQ_ERR_SUCCESS, Ordering::SeqCst);
        unsafe {
            REGISTERED = None;
            let key = CString::new("selector").unwrap();
            let good = CString::new("/foo").unwrap();
            let bad = CString::new("foo[").unwrap();
            let mut opts = [
                mosquitto_opt {
                    key: key.as_ptr() as *mut c_char,
                    value: good.as_ptr() as *mut c_char,
                },
                mosquitto_opt {
                    key: key.as_ptr() as *mut c_char,
                    value: bad.as_ptr() as *mut c_char,
                },
            ];
            let mut userdata: *mut c_void = std::ptr::null_mut();

            assert_eq!(
                mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, opts.as_mut_ptr(), 2),
                MOSQ_ERR_INVAL
            );
            assert!(userdata.is_null());
            let registered = REGISTERED;
            assert!(registered.is_none());
        }
    }

    #[test]
    fn payload_is_not_decoded_when_no_selector_reads_it() {
        let ctx = PluginContext::new(
            std::ptr::null_mut(),
            vec![Matcher::new(compile("/foo/+").unwrap())],
        );
        assert_eq!(ctx.reads_payload, [false]);
        assert!(!ctx.needs_headers);

        let ctx = PluginContext::new(
            std::ptr::null_mut(),
            vec![
                Matcher::new(compile("/msg[qos=1]/foo").unwrap()),
                Matcher::new(compile("//foo[json$.v>1]").unwrap()),
            ],
        );
        assert_eq!(ctx.reads_payload, [false, true]);
        assert!(ctx.needs_headers);
    }

    #[test]
    fn cbor_payloads_match_json_predicates() {
        let mut ctx = PluginContext::new(
            std::ptr::null_mut(),
            vec![Matcher::new(compile("//sensor[json$.temp>30]").unwrap())],
        );
        let topic = CString::new("site/sensor").unwrap();
        let mut send = |payload: &[u8]| {
            let mut msg = mosquitto_evt_message {
                future: std::ptr::null_mut(),
                client: std::ptr::null_mut(),
                topic: topic.as_ptr() as *mut c_char,
                payload: payload.as_ptr() as *mut c_void,
                properties: std::ptr::null_mut(),
                reason_string: std::ptr::null_mut(),
                payloadlen: payload.len() as u32,
                qos: 0,
                reason_code: 0,
                retain: false,
                future2: [std::ptr::null_mut(); 4],
            };
            on_message(
                MOSQ_EVT_MESSAGE,
                &mut msg as *mut _ as *mut c_void,
                &mut ctx as *mut _ as *mut c_void,
            )
        };
        let cbor = |temp: i64| {
            use ciborium::value::Value;
            let mut bytes = Vec::new();
            ciborium::into_writer(
                &Value::Map(vec![(
                    Value::Text("temp".into()),
                    Value::Integer(temp.into()),
                )]),
                &mut bytes,
            )
            .unwrap();
            bytes
        };

        assert_eq!(send(&cbor(35)), MOSQ_ERR_SUCCESS);
        assert_eq!(send(&cbor(25)), MOSQ_ERR_ACL_DENIED);
        assert_eq!(send(br#"{"temp":35}"#), MOSQ_ERR_SUCCESS);
        // Opaque bytes are denied, not a reason to disconnect anyone.
        assert_eq!(send(&[0xff, 0x00, 0x13]), MOSQ_ERR_ACL_DENIED);
    }

    #[test]
    fn payload_is_only_decoded_when_the_topic_could_match() {
        let mut ctx = PluginContext::new(
            std::ptr::null_mut(),
            vec![Matcher::new(compile("//sensor[json$.temp>30]").unwrap())],
        );
        let opaque = [0xffu8, 0x00, 0x13];
        let mut send = |topic: &str| {
            let topic = CString::new(topic).unwrap();
            let mut msg = mosquitto_evt_message {
                future: std::ptr::null_mut(),
                client: std::ptr::null_mut(),
                topic: topic.as_ptr() as *mut c_char,
                payload: opaque.as_ptr() as *mut c_void,
                properties: std::ptr::null_mut(),
                reason_string: std::ptr::null_mut(),
                payloadlen: opaque.len() as u32,
                qos: 0,
                reason_code: 0,
                retain: false,
                future2: [std::ptr::null_mut(); 4],
            };
            let rc = on_message(
                MOSQ_EVT_MESSAGE,
                &mut msg as *mut _ as *mut c_void,
                &mut ctx as *mut _ as *mut c_void,
            );
            (rc, ctx.decode_error_reported.load(Ordering::Relaxed))
        };

        // The topic rules the selector out, so the payload is never looked at.
        assert_eq!(send("site/actuator"), (MOSQ_ERR_ACL_DENIED, false));
        // Here it could match, so the payload is decoded (and fails).
        assert_eq!(send("site/sensor"), (MOSQ_ERR_ACL_DENIED, true));
    }

    #[test]
    fn filter_logic() {
        let _guard = TEST_LOCK.lock().unwrap();
        REGISTER_RESULT.store(MOSQ_ERR_SUCCESS, Ordering::SeqCst);
        unsafe {
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
                MOSQ_ERR_ACL_DENIED
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

            mosquitto_plugin_cleanup(userdata, std::ptr::null_mut(), 0);
            let registered = REGISTERED;
            assert!(registered.is_none());
        }
    }
}
