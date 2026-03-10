use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;

pub const RTLD_LAZY: i32 = 0x1;
pub const RTLD_NOW: i32 = 0x2;
pub const RTLD_LOCAL: i32 = 0x4;
pub const RTLD_GLOBAL: i32 = 0x8;

unsafe extern "C" {
    pub(crate) fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    pub(crate) fn dlsym(
        handle: *mut c_void,
        symbol: *const c_char,
    ) -> *mut c_void;
    pub(crate) fn dlclose(handle: *mut c_void) -> c_int;
    pub(crate) fn dlerror() -> *const c_char;
}

/// Returns the most recent `dlerror()` message, if one exists.
///
/// # Safety
/// Must be called immediately after a dynamic-loader operation where
/// `dlerror()` state is relevant to avoid reporting stale errors.
pub(crate) unsafe fn last_dlerror_message() -> Option<String> {
    // Safety: caller guarantees `dlerror()` is being read in a valid loader context.
    let err_ptr = unsafe { dlerror() };
    if err_ptr.is_null() {
        return None;
    }

    // Safety: `dlerror()` returns a NUL-terminated C string when non-null.
    let c_message = unsafe { CStr::from_ptr(err_ptr) };
    Some(c_message.to_string_lossy().into_owned())
}
