use core::ffi::c_void;

#[derive(Clone, Copy)]
pub struct RawSymbol {
    addr: *mut c_void,
}

impl RawSymbol {
    pub(crate) fn from_ptr(addr: *mut c_void) -> Self {
        Self { addr }
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.addr
    }

    pub fn is_null(&self) -> bool {
        self.addr.is_null()
    }
}

impl std::fmt::Debug for RawSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawSymbol")
            .field("addr", &self.addr)
            .finish()
    }
}
