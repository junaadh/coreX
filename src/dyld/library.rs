use super::error::DlError;
use super::raw;
use super::symbol::RawSymbol;
use core::ffi::{c_int, c_void};
use std::ffi::CString;
use std::path::{Path, PathBuf};

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;

pub struct Library {
    handle: *mut c_void,
    path: PathBuf,
}

impl Library {
    /// Opens a dynamic library or framework binary using safe default flags.
    ///
    /// Default flags are `RTLD_NOW | RTLD_LOCAL`.
    ///
    /// # Errors
    /// Returns [`DlError::InteriorNul`] for paths containing interior NUL bytes,
    /// or [`DlError::Open`] when `dlopen` fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DlError> {
        Self::open_with_flags(path, raw::RTLD_NOW | raw::RTLD_LOCAL)
    }

    /// Opens a dynamic library using explicit `dlopen` flags.
    ///
    /// # Errors
    /// Returns [`DlError::InteriorNul`] for paths containing interior NUL bytes,
    /// or [`DlError::Open`] when `dlopen` fails.
    pub fn open_with_flags(
        path: impl AsRef<Path>,
        flags: i32,
    ) -> Result<Self, DlError> {
        let path_ref = path.as_ref();
        let c_path = path_to_cstring(path_ref)?;

        // Safety: `c_path` is a valid NUL-terminated string and `flags` are passed through as-is.
        let handle = unsafe { raw::dlopen(c_path.as_ptr(), flags as c_int) };
        if handle.is_null() {
            // Safety: this is called immediately after the failing `dlopen`.
            let message = unsafe { raw::last_dlerror_message() }
                .unwrap_or_else(|| "unknown dlopen error".to_owned());
            return Err(DlError::Open {
                path: path_ref.to_path_buf(),
                message,
            });
        }

        Ok(Self {
            handle,
            path: path_ref.to_path_buf(),
        })
    }

    /// Resolves a symbol address by name.
    ///
    /// # Errors
    /// Returns [`DlError::InteriorNul`] for symbol names containing interior
    /// NUL bytes, or [`DlError::Symbol`] when `dlsym` reports an error.
    pub fn symbol(&self, name: &str) -> Result<RawSymbol, DlError> {
        let c_name = CString::new(name)
            .map_err(|_| DlError::InteriorNul { what: "symbol" })?;

        // Safety: clears stale thread-local `dlerror` state before `dlsym`.
        unsafe {
            let _ = raw::dlerror();
        }

        // Safety: `self.handle` comes from successful `dlopen`; `c_name` is valid C string.
        let addr = unsafe { raw::dlsym(self.handle, c_name.as_ptr()) };

        // Safety: called immediately after `dlsym` to detect loader-reported failure.
        if let Some(message) = unsafe { raw::last_dlerror_message() } {
            return Err(DlError::Symbol {
                symbol: name.to_owned(),
                message,
            });
        }

        Ok(RawSymbol::from_ptr(addr))
    }

    #[must_use]
    pub fn as_ptr(&self) -> *mut c_void {
        self.handle
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("path", &self.path)
            .field("handle", &self.handle)
            .finish()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }

        let handle = self.handle;
        self.handle = core::ptr::null_mut();

        // Safety: `handle` came from `dlopen` and is consumed exactly once by this drop path.
        let close_result = unsafe { raw::dlclose(handle) };
        if close_result != 0 {
            // Safety: read immediately after `dlclose` failure; intentionally ignored in drop.
            let close_message = unsafe { raw::last_dlerror_message() }
                .unwrap_or_else(|| "unknown dlclose error".to_owned());
            let _ = DlError::Close {
                message: close_message,
            };
        }
    }
}

fn path_to_cstring(path: &Path) -> Result<CString, DlError> {
    #[cfg(target_family = "unix")]
    {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DlError::InteriorNul { what: "path" })
    }

    #[cfg(not(target_family = "unix"))]
    {
        CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| DlError::InteriorNul { what: "path" })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::ffi::{CString, OsString, c_char};
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    const LIBSYSTEM_PATH: &str = "/usr/lib/libSystem.B.dylib";
    const FOUNDATION_PATH: &str =
        "/System/Library/Frameworks/Foundation.framework/Foundation";
    const BAD_LIBRARY_PATH: &str =
        "/definitely/not/a/real/library/path/libnope.dylib";

    type PutsFn = unsafe extern "C" fn(*const c_char) -> i32;
    type StrlenFn = unsafe extern "C" fn(*const c_char) -> usize;
    type GetPidFn = unsafe extern "C" fn() -> i32;

    #[test]
    fn open_known_system_library() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        assert!(!lib.as_ptr().is_null());
    }

    #[test]
    fn resolve_known_symbols() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");

        let puts = lib.symbol("puts").expect("puts should resolve");
        let strlen = lib.symbol("strlen").expect("strlen should resolve");
        let getpid = lib.symbol("getpid").expect("getpid should resolve");

        assert!(!puts.is_null());
        assert!(!strlen.is_null());
        assert!(!getpid.is_null());
    }

    #[test]
    fn open_bad_path_returns_open_error() {
        let result = Library::open(BAD_LIBRARY_PATH);
        assert!(matches!(result, Err(DlError::Open { .. })));
    }

    #[test]
    fn bad_symbol_returns_symbol_error() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let result = lib.symbol("definitely_not_a_real_symbol");
        assert!(matches!(result, Err(DlError::Symbol { .. })));
    }

    #[test]
    fn interior_nul_in_path_is_rejected() {
        let path_with_nul = PathBuf::from(OsString::from_vec(
            b"/usr/lib/libSystem.B.dylib\0tail".to_vec(),
        ));
        let result = Library::open(path_with_nul);
        assert!(matches!(result, Err(DlError::InteriorNul { what: "path" })));
    }

    #[test]
    fn interior_nul_in_symbol_is_rejected() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let result = lib.symbol("str\0len");
        assert!(matches!(
            result,
            Err(DlError::InteriorNul { what: "symbol" })
        ));
    }

    #[test]
    fn repeated_open_succeeds_and_drop_is_safe() {
        let lib1 =
            Library::open(LIBSYSTEM_PATH).expect("first open should succeed");
        let lib2 =
            Library::open(LIBSYSTEM_PATH).expect("second open should succeed");

        assert!(!lib1.as_ptr().is_null());
        assert!(!lib2.as_ptr().is_null());

        drop(lib1);
        drop(lib2);
    }

    #[test]
    fn open_framework_binary_path() {
        let framework = Library::open(FOUNDATION_PATH)
            .expect("Foundation framework should open");
        assert!(!framework.as_ptr().is_null());
    }

    #[test]
    fn call_puts_smoke_test() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let puts = lib.symbol("puts").expect("puts should resolve");

        // Safety: test uses the known libc signature for `puts` and keeps `lib` alive.
        let puts_fn: PutsFn = unsafe { puts.cast() };
        let msg = CString::new("dyld smoke test: puts")
            .expect("CString literal has no NUL");

        // Safety: `msg` is a valid C string pointer for the duration of the call.
        let rc = unsafe { puts_fn(msg.as_ptr()) };
        assert!(rc >= 0);
    }

    #[test]
    fn call_strlen_smoke_test() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let strlen = lib.symbol("strlen").expect("strlen should resolve");

        // Safety: test uses the known libc signature for `strlen` and keeps `lib` alive.
        let strlen_fn: StrlenFn = unsafe { strlen.cast() };
        let input = CString::new("hello").expect("CString literal has no NUL");

        // Safety: `input` is a valid C string pointer for the duration of the call.
        let len = unsafe { strlen_fn(input.as_ptr()) };
        assert_eq!(len, 5);
    }

    #[test]
    fn call_getpid_smoke_test() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let getpid = lib.symbol("getpid").expect("getpid should resolve");

        // Safety: test uses the known libc signature for `getpid` and keeps `lib` alive.
        let getpid_fn: GetPidFn = unsafe { getpid.cast() };

        // Safety: `getpid` takes no arguments and is safe to invoke in-process.
        let pid = unsafe { getpid_fn() };
        assert!(pid > 0);
    }
}
