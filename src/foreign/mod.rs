//! Reusable foreign function declarations built on top of the dynamic call layer.
//!
//! This module provides a stable declaration object that packages:
//! - strong library ownership
//! - eager symbol resolution
//! - prepared reusable call metadata for a stored runtime signature
//! - reusable invocation with symbol-aware error context
//!
//! It does not add parser-level syntax, language declarations, or additional ABI
//! features beyond the existing dynamic call substrate.

mod bindgen;
mod decl;
mod error;
mod function;
mod library;
mod manifest;
mod parse;

pub use crate::ffi::ForeignCallConv;
pub use bindgen::{
    BindgenError, BindgenOptions, BindgenOutput, generate_foreign_bindings,
};
pub use decl::{
    ForeignFunctionDecl, ForeignLibraryDecl, LoweringError,
    lower_foreign_library_decl, validate_foreign_library_decl,
};
pub use error::ForeignError;
pub use function::ForeignFunction;
pub use library::ForeignLibrary;
pub use manifest::{
    FileLoweringError, ForeignLibraryManifest, LibraryPaths, ManifestError,
    ManifestLoweringError, ResolveError, TargetOs,
    lower_parsed_foreign_file_with_manifest,
    lower_parsed_foreign_library_decl_with_manifest,
};
pub use parse::{
    Attribute, CallConv, ParamName, ParseError, ParsedForeignFile,
    ParsedForeignFunctionDecl, ParsedForeignLibraryDecl, ParsedForeignParam,
    SourceForeignType, lower_parsed_foreign_library_decl, parse_foreign_file,
    parse_foreign_library_decl,
};
