use core::ffi::c_void;

/// Raw resolved symbol address returned from `dlsym`.
///
/// This type is non-owning. It does not keep the backing library loaded.
#[derive(Clone, Copy)]
pub struct RawSymbol {
    addr: *mut c_void,
}

impl RawSymbol {
    pub(super) fn from_ptr(addr: *mut c_void) -> Self {
        Self { addr }
    }

    #[cfg(test)]
    pub(crate) fn null_for_test() -> Self {
        Self {
            addr: core::ptr::null_mut(),
        }
    }

    #[must_use]
    pub fn as_ptr(&self) -> *mut c_void {
        self.addr
    }

    #[must_use]
    pub fn is_null(&self) -> bool {
        self.addr.is_null()
    }

    /// Casts this raw symbol address into a caller-specified function pointer type.
    ///
    /// # Safety
    /// The caller must ensure all of the following:
    /// - `T` exactly matches the native symbol ABI and signature.
    /// - The symbol actually refers to a callable function with that type.
    /// - The returned function pointer is not used after the parent
    ///   [`crate::dyld::Library`] has been dropped/unloaded.
    ///
    /// Violating these requirements is undefined behavior.
    #[must_use]
    pub unsafe fn cast<T>(&self) -> T
    where
        T: Copy,
    {
        debug_assert_eq!(
            core::mem::size_of::<T>(),
            core::mem::size_of::<*mut c_void>()
        );
        // Safety: caller upholds type/ABI/lifetime requirements documented above.
        unsafe { core::mem::transmute_copy(&self.addr) }
    }
}

impl std::fmt::Debug for RawSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawSymbol")
            .field("addr", &self.addr)
            .finish()
    }
}
