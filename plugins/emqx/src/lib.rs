//! Minimal EMQX plugin using the extension API.

use moqtail_core::{compile, Matcher, Message};
use std::{
    collections::HashMap,
    ffi::CStr,
    os::raw::{c_char, c_int, c_void},
};

/// Representation of the publish message that EMQX passes to hooks.
#[repr(C)]
pub struct EmqxMessage {
    pub topic: *const c_char,
    // Additional fields are ignored by this example
}

type HookFn = extern "C" fn(*mut EmqxMessage, *mut c_void) -> c_int;

extern "C" {
    fn emqx_extension_register_hook(name: *const c_char, cb: Option<HookFn>, data: *mut c_void) -> c_int;
    fn emqx_extension_unregister_hook(name: *const c_char, cb: Option<HookFn>, data: *mut c_void) -> c_int;
}

const MESSAGE_HOOK: &[u8] = b"message_publish\0";

pub struct PluginContext {
    matchers: Vec<Matcher>,
}

extern "C" fn on_message(msg: *mut EmqxMessage, userdata: *mut c_void) -> c_int {
    unsafe {
        let ctx = &*(userdata as *mut PluginContext);
        if msg.is_null() || (*msg).topic.is_null() {
            return 0;
        }
        let topic = match CStr::from_ptr((*msg).topic).to_str() {
            Ok(t) => t,
            Err(_) => return 1,
        };
        let m = Message {
            topic,
            headers: HashMap::new(),
            payload: None,
        };
        for matcher in &ctx.matchers {
            if matcher.matches(&m) {
                return 0;
            }
        }
    }
    1
}

/// Called by EMQX when the plugin is loaded.
#[no_mangle]
pub unsafe extern "C" fn moqtail_init(selectors: *const *const c_char, count: usize) -> *mut c_void {
    let slice = std::slice::from_raw_parts(selectors, count);
    let mut matchers = Vec::new();
    for &ptr in slice {
        if ptr.is_null() {
            continue;
        }
        let sel = match CStr::from_ptr(ptr).to_str() {
            Ok(s) => s,
            Err(_) => continue,
        };
        match compile(sel) {
            Ok(s) => matchers.push(Matcher::new(s)),
            Err(e) => eprintln!("[MoQTail] selector error: {}", e),
        }
    }

    let ctx = Box::new(PluginContext { matchers });
    let ctx_ptr = Box::into_raw(ctx) as *mut c_void;
    emqx_extension_register_hook(MESSAGE_HOOK.as_ptr() as *const c_char, Some(on_message), ctx_ptr);
    ctx_ptr
}

/// Called when EMQX unloads the plugin.
#[no_mangle]
pub unsafe extern "C" fn moqtail_deinit(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    emqx_extension_unregister_hook(MESSAGE_HOOK.as_ptr() as *const c_char, Some(on_message), ctx);
    drop(Box::from_raw(ctx as *mut PluginContext));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[no_mangle]
    pub static mut REGISTERED: Option<(HookFn, *mut c_void)> = None;

    #[no_mangle]
    pub unsafe extern "C" fn emqx_extension_register_hook(
        _name: *const c_char,
        cb: Option<HookFn>,
        data: *mut c_void,
    ) -> c_int {
        if let Some(f) = cb {
            REGISTERED = Some((f, data));
        }
        0
    }

    #[no_mangle]
    pub unsafe extern "C" fn emqx_extension_unregister_hook(
        _name: *const c_char,
        _cb: Option<HookFn>,
        _data: *mut c_void,
    ) -> c_int {
        REGISTERED = None;
        0
    }

    #[test]
    fn filter_logic() {
        unsafe {
            let sel = CString::new("/foo/+" ).unwrap();
            let arr = [sel.as_ptr()];
            let ctx = moqtail_init(arr.as_ptr(), arr.len());
            let (cb, data) = REGISTERED.expect("hook registered");

            let topic1 = CString::new("foo/bar").unwrap();
            let mut msg = EmqxMessage { topic: topic1.as_ptr() };
            assert_eq!(cb(&mut msg as *mut _, data), 0);

            let topic2 = CString::new("baz/qux").unwrap();
            msg.topic = topic2.as_ptr();
            assert_eq!(cb(&mut msg as *mut _, data), 1);

            moqtail_deinit(ctx);
            assert!(REGISTERED.is_none());
        }
    }
}
