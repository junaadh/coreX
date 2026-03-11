use super::types::NativeType;
use core::ffi::c_void;
use std::ffi::{CStr, CString};

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Void,
    I32(i32),
    USize(usize),
    Ptr(*mut c_void),
}

impl Value {
    /// Constructs an opaque pointer value from a mutable pointer.
    ///
    /// The returned value does not take ownership of the pointee. The caller
    /// must ensure the pointed-to data remains valid for the duration of any
    /// foreign call that uses this value.
    #[must_use]
    pub fn from_ptr(ptr: *mut c_void) -> Self {
        Self::Ptr(ptr)
    }

    /// Constructs an opaque pointer value from a const pointer.
    ///
    /// This uses the runtime's opaque pointer ABI representation. If this value
    /// is stored internally as a mutable opaque pointer, that is only a
    /// representation detail and does not grant mutability of the original
    /// pointee.
    ///
    /// The returned value does not take ownership of the pointee. The caller
    /// must ensure the pointed-to data remains valid for the duration of any
    /// foreign call that uses this value.
    #[must_use]
    pub fn from_const_ptr(ptr: *const c_void) -> Self {
        Self::Ptr(ptr.cast_mut())
    }

    /// Constructs an opaque pointer value from a borrowed [`CString`].
    ///
    /// This does not copy the bytes and does not take ownership of the string
    /// storage. The caller must ensure the [`CString`] remains alive for the
    /// duration of any foreign call that uses this value.
    #[must_use]
    pub fn from_c_string(s: &CString) -> Self {
        Self::from_const_ptr(s.as_ptr().cast())
    }

    /// Constructs an opaque pointer value from a borrowed [`CStr`].
    ///
    /// This does not copy the bytes and does not take ownership of the backing
    /// storage. The caller must ensure the backing storage remains alive for
    /// the duration of any foreign call that uses this value.
    #[must_use]
    pub fn from_c_str(s: &CStr) -> Self {
        Self::from_const_ptr(s.as_ptr().cast())
    }

    #[must_use]
    pub fn native_type(&self) -> NativeType {
        match self {
            Self::Void => NativeType::Void,
            Self::I32(_) => NativeType::I32,
            Self::USize(_) => NativeType::USize,
            Self::Ptr(_) => NativeType::Ptr,
        }
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Void => "Void",
            Self::I32(_) => "I32",
            Self::USize(_) => "USize",
            Self::Ptr(_) => "Ptr",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_from_const_ptr_round_trips_to_ptr_variant() {
        let x: u8 = 7;
        let raw: *const c_void = core::ptr::addr_of!(x).cast();
        let value = Value::from_const_ptr(raw);

        match value {
            Value::Ptr(stored) => assert_eq!(stored.cast_const(), raw),
            other => panic!("expected Value::Ptr, got {other:?}"),
        }
    }

    #[test]
    fn value_from_ptr_round_trips_to_ptr_variant() {
        let mut x: u8 = 9;
        let raw: *mut c_void = core::ptr::addr_of_mut!(x).cast();
        let value = Value::from_ptr(raw);

        match value {
            Value::Ptr(stored) => assert_eq!(stored, raw),
            other => panic!("expected Value::Ptr, got {other:?}"),
        }
    }
}
