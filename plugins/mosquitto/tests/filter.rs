use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
extern crate moqtail_mosquitto;
use moqtail_mosquitto::{mosquitto_opt, mosquitto_evt_message, mosquitto_plugin_init, mosquitto_plugin_cleanup};


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
        let val = CString::new("/foo/+" ).unwrap();
        let mut opt = mosquitto_opt { key: key.as_ptr() as *mut c_char, value: val.as_ptr() as *mut c_char };
        let mut userdata: *mut c_void = std::ptr::null_mut();

        assert_eq!(mosquitto_plugin_init(std::ptr::null_mut(), &mut userdata, &mut opt, 1), 0);
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
        assert_eq!(cb(7, &mut msg as *mut _ as *mut c_void, ctx), 1);

        mosquitto_plugin_cleanup(std::ptr::null_mut(), userdata, std::ptr::null_mut(), 0);
        assert!(REGISTERED.is_none());
    }
}
