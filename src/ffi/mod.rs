//! Runtime-facing dynamic native call substrate.
//!
//! This module introduces the first runtime-defined call path:
//! - [`NativeType`] describes native argument/return types at runtime.
//! - [`Value`] carries runtime values for those types.
//! - [`Signature`] declares parameter and return layout.
//! - [`ForeignCallConv`] describes resolved foreign calling convention metadata.
//! - [`PreparedCall`] stores reusable libffi metadata for a fixed signature and call convention.
//! - [`call_symbol`] performs preflight validation and dynamic invocation.
//! - [`call_prepared`] reuses prepared call metadata for repeated invocations.
//!
//! ## Why Typed Casts Alone Are Not Enough
//! Typed cast helpers remove repetitive call-site `transmute`, but still
//! require compile-time Rust function-pointer types at each call site. That
//! does not support runtime-defined signatures.
//!
//! ## Why This Module Introduces `NativeType` / `Value` / `Signature`
//! A runtime needs to describe function signatures and values dynamically.
//! These three types are the minimum substrate required to validate, marshal,
//! call, and decode native functions without per-call Rust type definitions.
//!
//! ## Why the Supported Type Set Is Intentionally Tiny
//! Only `Void`, `I32`, `USize`, and `Ptr` are included because they are enough
//! for current smoke targets (`getpid`, `strlen`, `puts`) and keep the unsafe
//! marshalling surface narrow.
//!
//! ## Why libffi Is Used Here
//! `libffi` handles platform ABI call mechanics for runtime-described
//! signatures, so this module can focus on value/type validation and marshalling
//! instead of hand-rolling ABI dispatch logic.
//!
//! ## Deferred
//! callbacks, variadics, structs-by-value, Objective-C, and richer
//! string/object bridging.

mod call;
mod call_conv;
mod error;
mod signature;
mod types;
mod value;

pub use call::{PreparedCall, call_prepared, call_symbol};
pub use call_conv::ForeignCallConv;
pub use error::CallError;
pub use signature::Signature;
pub use types::NativeType;
pub use value::Value;
