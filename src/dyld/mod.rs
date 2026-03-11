//! macOS dynamic loader module.
//!
//! ## Scope
//! This module provides only dynamic loading primitives for macOS:
//! opening libraries/framework binaries by explicit path, resolving
//! exported symbols by name, and releasing handles on drop.
//!
//! ## Public API
//! - [`Library`] owns a single `dlopen` handle.
//! - [`RawSymbol`] is an untyped, non-owning symbol address.
//! - [`DlError`] reports loader failures as structured Rust errors.
//! - `RTLD_*` constants are re-exported for `open_with_flags`.
//!
//! ## Ownership Model
//! `Library` is the sole owner of one loader handle. Symbols returned by
//! [`Library::symbol`] never own the handle and are only valid while the
//! corresponding library remains loaded.
//!
//! ## Safety Invariants
//! 1. `Library` owns exactly one `dlopen` handle.
//! 2. A `Library` handle is passed to `dlclose` exactly once during drop.
//! 3. `Library` is only constructed when `dlopen` returns non-null.
//! 4. `symbol()` never transfers library-handle ownership.
//! 5. `RawSymbol` is non-owning and tied to library lifetime by convention.
//! 6. `dlerror()` is read immediately after failing loader operations.
//! 7. Expected runtime failures return `Result` errors, not panics.
//! 8. Unsafe code is isolated to a small core boundary.
//!
//! ## Why Default Flags Are `RTLD_NOW | RTLD_LOCAL`
//! `RTLD_NOW` surfaces unresolved bindings at load-time, and `RTLD_LOCAL`
//! avoids exporting symbols into global lookup scope.
//!
//! ## Why `RawSymbol` Is Untyped
//! This module is loader-only. No function invocation, pointer transmutation,
//! or typed symbol API is included.
//!
//! ## Deferred Non-Goals
//! Function calling, `libffi`, Objective-C bridging, parser/VM work,
//! cross-platform support, search heuristics, and plugin hot reload.
//!
//! ## Test Strategy
//! macOS-only unit tests cover known-good library/framework opens, known
//! symbol resolution, failure paths (bad path/symbol), interior-NUL handling,
//! and repeated-open/drop behavior.
//!
//! ## TODO / FIXME
//! - TODO: if typed symbol support is added later, preserve this safe loader API
//!   and keep transmutation isolated from the loader core.
//! - TODO: if close-error observability is needed later, add opt-in diagnostics
//!   without changing `Drop` panic-free behavior.
//! - FIXME: keep future path-resolution/search behavior out of this module unless
//!   explicitly required by runtime design.

mod error;
mod library;
pub(crate) mod raw;
mod symbol;

pub use error::DlError;
pub use library::Library;
pub use raw::{RTLD_GLOBAL, RTLD_LAZY, RTLD_LOCAL, RTLD_NOW};
pub use symbol::RawSymbol;
