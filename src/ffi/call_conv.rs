/// Resolved foreign calling convention used by lowered declarations.
///
/// The runtime currently supports only C calling convention metadata, but this
/// explicit enum keeps call metadata honest and forward-compatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignCallConv {
    C,
}

impl ForeignCallConv {
    /// Returns the default foreign calling convention.
    #[must_use]
    pub const fn default_foreign() -> Self {
        Self::C
    }
}
