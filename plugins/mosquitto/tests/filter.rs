use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
extern crate moqtail_mosquitto;
use moqtail_mosquitto::{
    mosquitto_evt_message, mosquitto_opt, mosquitto_plugin_cleanup, mosquitto_plugin_init,
};

const MOSQ_ERR_PLUGIN_DEFER: c_int = 17;

static mut REGISTERED: Option<(
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
    if let Some(f) = cb_func {
        REGISTERED = Some((f, userdata));
    }
    0
}

#[no_mangle]
unsafe extern "C" fn mosquitto_callback_unregister(
    _identifier: *mut c_void,
    _event: c_int,
    _cb_func: Option<extern "C" fn(c_int, *mut c_void, *mut c_void) -> c_int>,
    _event_data: *const c_void,
) -> c_int {
    REGISTERED = None;
    0
}

#[test]
fn filter_integration() {
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
            0
        );
        let (cb, ctx) = REGISTERED.expect("callback registered");

        let topic1 = CString::new("foo/bar").unwrap();
        let mut msg = mosquitto_evt_message {
            future: std::ptr::null_mut(),
            client: std::ptr::null_mut(),
            topic: topic1.as_ptr() as *mut c_char,
            payload: std::ptr::null_mut(),
            properties: std::ptr::null_mut(),
            reason_string: std::ptr::null_mut(),
            payloadlen: 0,
            qos: 0,
            reason_code: 0,
            retain: false,
            future2: [std::ptr::null_mut(); 4],
        };

        assert_eq!(cb(7, &mut msg as *mut _ as *mut c_void, ctx), 0);

        let topic2 = CString::new("baz/qux").unwrap();
        msg.topic = topic2.as_ptr() as *mut c_char;
        assert_eq!(
            cb(7, &mut msg as *mut _ as *mut c_void, ctx),
            MOSQ_ERR_PLUGIN_DEFER
        );

        mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
        assert!(REGISTERED.is_none());
    }
}

#[test]
fn header_filter() {
    unsafe {
        let key = CString::new("selector").unwrap();
        let val = CString::new("/msg[qos<=1]").unwrap();
        let mut opt = mosquitto_opt {
            key: key.as_ptr() as *mut c_char,
            value: val.as_ptr() as *mut c_char,
        };
        let mut userdata: *mut c_void = std::ptr::null_mut();

        assert_eq!(
            mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1),
            0
        );
        let (cb, ctx) = REGISTERED.expect("callback registered");

        let topic = CString::new("").unwrap();
        let bad_payload = CString::new("not json").unwrap();
        let mut msg = mosquitto_evt_message {
            future: std::ptr::null_mut(),
            client: std::ptr::null_mut(),
            topic: topic.as_ptr() as *mut c_char,
            payload: bad_payload.as_ptr() as *mut c_void,
            properties: std::ptr::null_mut(),
            reason_string: std::ptr::null_mut(),
            payloadlen: bad_payload.as_bytes().len() as u32,
            qos: 0,
            reason_code: 0,
            retain: false,
            future2: [std::ptr::null_mut(); 4],
        };

        assert_eq!(cb(7, &mut msg as *mut _ as *mut c_void, ctx), 0);

        msg.qos = 2;
        assert_eq!(
            cb(7, &mut msg as *mut _ as *mut c_void, ctx),
            MOSQ_ERR_PLUGIN_DEFER
        );

        mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
        assert!(REGISTERED.is_none());
    }
}

#[test]
fn payload_filter() {
    unsafe {
        let key = CString::new("selector").unwrap();
        let val = CString::new("/foo[json$.temp>30]").unwrap();
        let mut opt = mosquitto_opt {
            key: key.as_ptr() as *mut c_char,
            value: val.as_ptr() as *mut c_char,
        };
        let mut userdata: *mut c_void = std::ptr::null_mut();

        assert_eq!(
            mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1),
            0
        );
        let (cb, ctx) = REGISTERED.expect("callback registered");

        let topic = CString::new("foo").unwrap();
        let payload1 = CString::new("{\"temp\":35}").unwrap();
        let mut msg = mosquitto_evt_message {
            future: std::ptr::null_mut(),
            client: std::ptr::null_mut(),
            topic: topic.as_ptr() as *mut c_char,
            payload: payload1.as_ptr() as *mut c_void,
            properties: std::ptr::null_mut(),
            reason_string: std::ptr::null_mut(),
            payloadlen: payload1.as_bytes().len() as u32,
            qos: 0,
            reason_code: 0,
            retain: false,
            future2: [std::ptr::null_mut(); 4],
        };

        assert_eq!(cb(7, &mut msg as *mut _ as *mut c_void, ctx), 0);

        let payload2 = CString::new("{\"temp\":25}").unwrap();
        msg.payload = payload2.as_ptr() as *mut c_void;
        msg.payloadlen = payload2.as_bytes().len() as u32;
        assert_eq!(
            cb(7, &mut msg as *mut _ as *mut c_void, ctx),
            MOSQ_ERR_PLUGIN_DEFER
        );

        mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
        assert!(REGISTERED.is_none());
    }
}
