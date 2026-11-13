//! C ABI. See `include/brick.h` for the contract.

use crate::engine::Engine;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type BrickEventCallback = extern "C" fn(json: *const c_char, user_data: *mut c_void);

/// What `brick_open` hands back. `closed` is held while a callback runs, so
/// once `brick_close` returns the host will never be called again.
pub struct Handle {
    engine: Engine,
    closed: Arc<Mutex<bool>>,
}

struct UserData(*mut c_void);
// The host promises its callback may be invoked from any thread.
unsafe impl Send for UserData {}
unsafe impl Sync for UserData {}

fn to_c(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `data_dir` must be a valid NUL-terminated UTF-8 path. `callback` must be
/// safe to call from any thread for as long as the engine is open.
#[no_mangle]
pub unsafe extern "C" fn brick_open(
    data_dir: *const c_char,
    callback: BrickEventCallback,
    user_data: *mut c_void,
    error_out: *mut *mut c_char,
) -> *mut Handle {
    let fail = |msg: String| {
        if !error_out.is_null() {
            *error_out = to_c(msg);
        }
        std::ptr::null_mut()
    };
    if data_dir.is_null() {
        return fail("data_dir is null".into());
    }
    let Ok(dir) = CStr::from_ptr(data_dir).to_str() else {
        return fail("data_dir is not valid UTF-8".into());
    };
    let user = UserData(user_data);
    let closed = Arc::new(Mutex::new(false));
    let gate = closed.clone();
    let emit = Arc::new(move |json: String| {
        let user = &user;
        let closed = gate.lock().unwrap_or_else(|e| e.into_inner());
        if *closed {
            return;
        }
        if let Ok(c) = CString::new(json) {
            callback(c.as_ptr(), user.0);
        }
    });
    match std::panic::catch_unwind(|| Engine::open(Path::new(dir), emit)) {
        Ok(Ok(engine)) => Box::into_raw(Box::new(Handle { engine, closed })),
        Ok(Err(e)) => fail(format!("{e:#}")),
        Err(_) => fail("internal error while opening the mail store".into()),
    }
}

/// # Safety
/// `engine` must come from `brick_open` and not have been closed. `request`
/// must be a valid NUL-terminated string. Free the result with `brick_string_free`.
#[no_mangle]
pub unsafe extern "C" fn brick_call(engine: *mut Handle, request: *const c_char) -> *mut c_char {
    if engine.is_null() || request.is_null() {
        return to_c(r#"{"error":"null argument"}"#.into());
    }
    let engine = &(*engine).engine;
    let request = CStr::from_ptr(request).to_string_lossy();
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.call(&request)))
        .unwrap_or_else(|_| r#"{"error":"internal error"}"#.into());
    to_c(out)
}

/// # Safety
/// `s` must be a string returned by this library, or null.
#[no_mangle]
pub unsafe extern "C" fn brick_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// # Safety
/// `engine` must come from `brick_open`, or be null. It must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn brick_close(engine: *mut Handle) {
    if !engine.is_null() {
        let handle = Box::from_raw(engine);
        *handle.closed.lock().unwrap_or_else(|e| e.into_inner()) = true;
        drop(handle);
    }
}
