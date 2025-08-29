#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mosquitto_opt {
    pub key: *mut ::std::os::raw::c_char,
    pub value: *mut ::std::os::raw::c_char,
}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mosquitto_evt_message {
    pub future: *mut ::std::os::raw::c_void,
    pub client: *mut ::std::os::raw::c_void,
    pub topic: *mut ::std::os::raw::c_char,
    pub payload: *mut ::std::os::raw::c_void,
    pub properties: *mut ::std::os::raw::c_void,
    pub reason_string: *mut ::std::os::raw::c_char,
    pub payloadlen: u32,
    pub qos: ::std::os::raw::c_int,
    pub reason_code: ::std::os::raw::c_int,
    pub retain: bool,
    pub future2: [*mut ::std::os::raw::c_void; 4],
}

#[allow(non_camel_case_types)]
pub enum mosquitto_plugin_id_t {}

extern "C" {
    pub fn mosquitto_callback_register(
        identifier: *mut mosquitto_plugin_id_t,
        event: ::std::os::raw::c_int,
        cb_func: Option<
            extern "C" fn(
                ::std::os::raw::c_int,
                *mut ::std::os::raw::c_void,
                *mut ::std::os::raw::c_void,
            ) -> ::std::os::raw::c_int,
        >,
        event_data: *const ::std::os::raw::c_void,
        userdata: *mut ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;

    pub fn mosquitto_callback_unregister(
        identifier: *mut mosquitto_plugin_id_t,
        event: ::std::os::raw::c_int,
        cb_func: Option<
            extern "C" fn(
                ::std::os::raw::c_int,
                *mut ::std::os::raw::c_void,
                *mut ::std::os::raw::c_void,
            ) -> ::std::os::raw::c_int,
        >,
        event_data: *const ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
