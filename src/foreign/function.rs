use super::ForeignError;
use crate::dyld::{Library, RawSymbol};
use crate::ffi::{
    ForeignCallConv, PreparedCall, Signature, Value, call_prepared,
};
use std::sync::Arc;

/// Declared foreign function with eager symbol resolution and prepared call metadata.
///
/// This type keeps the underlying library alive, resolves the symbol at
/// construction time, and reuses prepared call metadata across invocations.
///
/// Declaring a signature here does not prove it matches the actual native ABI
/// of the resolved symbol. Callers remain responsible for declaration accuracy.
pub struct ForeignFunction {
    library: Arc<Library>,
    symbol_name: String,
    symbol: RawSymbol,
    call_conv: ForeignCallConv,
    prepared: PreparedCall,
}

impl ForeignFunction {
    /// Declares a foreign function by preparing call metadata and eagerly
    /// resolving `symbol_name` with default foreign calling convention metadata.
    ///
    /// # Errors
    /// Returns:
    /// - [`ForeignError::InvalidSignature`] when the declaration signature
    ///   cannot be prepared for invocation.
    /// - [`ForeignError::SymbolResolve`] when symbol resolution fails.
    pub fn new(
        library: Arc<Library>,
        symbol_name: impl Into<String>,
        signature: Signature,
    ) -> Result<Self, ForeignError> {
        Self::new_with_call_conv(
            library,
            symbol_name,
            signature,
            ForeignCallConv::default_foreign(),
        )
    }

    /// Declares a foreign function with explicit resolved calling-convention
    /// metadata.
    ///
    /// # Errors
    /// Returns:
    /// - [`ForeignError::InvalidSignature`] when the declaration signature
    ///   cannot be prepared for invocation.
    /// - [`ForeignError::SymbolResolve`] when symbol resolution fails.
    pub fn new_with_call_conv(
        library: Arc<Library>,
        symbol_name: impl Into<String>,
        signature: Signature,
        call_conv: ForeignCallConv,
    ) -> Result<Self, ForeignError> {
        let symbol_name = symbol_name.into();
        let prepared = PreparedCall::new_with_call_conv(signature, call_conv)
            .map_err(|source| ForeignError::InvalidSignature {
            symbol: symbol_name.clone(),
            message: source.to_string(),
        })?;

        let symbol = library.symbol(&symbol_name).map_err(|source| {
            ForeignError::SymbolResolve {
                symbol: symbol_name.clone(),
                source,
            }
        })?;

        Ok(Self {
            library,
            symbol_name,
            symbol,
            call_conv,
            prepared,
        })
    }

    /// Invokes the declared foreign function using prepared reusable metadata.
    ///
    /// # Errors
    /// Returns [`ForeignError::Invocation`] when argument validation or dynamic
    /// invocation fails in the underlying call runtime.
    pub fn call(&self, args: &[Value]) -> Result<Value, ForeignError> {
        call_prepared(&self.prepared, &self.symbol, args).map_err(|source| {
            ForeignError::Invocation {
                symbol: self.symbol_name.clone(),
                source,
            }
        })
    }

    #[must_use]
    pub fn symbol_name(&self) -> &str {
        &self.symbol_name
    }

    #[must_use]
    pub fn signature(&self) -> &Signature {
        self.prepared.signature()
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        self.library.as_ref()
    }

    #[must_use]
    /// Returns the resolved foreign calling convention metadata used for
    /// prepared invocation of this declaration.
    pub fn call_conv(&self) -> ForeignCallConv {
        self.call_conv
    }
}

impl std::fmt::Debug for ForeignFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeignFunction")
            .field("symbol_name", &self.symbol_name)
            .field("signature", self.prepared.signature())
            .field("call_conv", &self.call_conv)
            .field("symbol", &self.symbol)
            .field("library_path", &self.library.path())
            .finish()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::ffi::{CallError, ForeignCallConv, NativeType};
    use std::ffi::CString;

    const LIBSYSTEM_PATH: &str = "/usr/lib/libSystem.B.dylib";

    #[test]
    fn declare_and_call_getpid() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let getpid = ForeignFunction::new(
            lib,
            "getpid",
            Signature::new(vec![], NativeType::I32),
        )
        .expect("declare getpid");
        assert_eq!(getpid.call_conv(), ForeignCallConv::C);

        let result = getpid.call(&[]).expect("invoke getpid");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn declare_and_call_strlen() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let strlen = ForeignFunction::new(
            lib,
            "strlen",
            Signature::new(vec![NativeType::Ptr], NativeType::USize),
        )
        .expect("declare strlen");
        let input = CString::new("hello").expect("literal contains no NUL");

        let result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("invoke strlen");
        match result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
    }

    #[test]
    fn declare_and_call_puts_multiple_times() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let puts = ForeignFunction::new(
            lib,
            "puts",
            Signature::new(vec![NativeType::Ptr], NativeType::I32),
        )
        .expect("declare puts");
        let first =
            CString::new("first puts call").expect("literal contains no NUL");
        let second =
            CString::new("second puts call").expect("literal contains no NUL");

        let first_rc = puts
            .call(&[Value::from_c_string(&first)])
            .expect("first puts call");
        let second_rc = puts
            .call(&[Value::from_c_string(&second)])
            .expect("second puts call");

        match first_rc {
            Value::I32(rc) => assert!(rc >= 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
        match second_rc {
            Value::I32(rc) => assert!(rc >= 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn declare_bad_symbol_returns_symbol_resolve_error() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let result = ForeignFunction::new(
            lib,
            "__definitely_not_a_real_symbol__",
            Signature::new(vec![], NativeType::I32),
        );

        let err = result.expect_err("declaration should fail");
        match err {
            ForeignError::SymbolResolve { symbol, .. } => {
                assert_eq!(symbol, "__definitely_not_a_real_symbol__");
            }
            other @ (ForeignError::Invocation { .. }
            | ForeignError::InvalidSignature { .. }
            | ForeignError::DuplicateDeclaration { .. }) => {
                panic!("expected SymbolResolve, got {other:?}");
            }
        }
    }

    #[test]
    fn invocation_type_mismatch_is_wrapped_with_symbol_context() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let strlen = ForeignFunction::new(
            lib,
            "strlen",
            Signature::new(vec![NativeType::Ptr], NativeType::USize),
        )
        .expect("declare strlen");

        let err = strlen
            .call(&[Value::I32(123)])
            .expect_err("mismatch should fail");
        match err {
            ForeignError::Invocation { symbol, source } => {
                assert_eq!(symbol, "strlen");
                assert!(matches!(source, CallError::TypeMismatch { .. }));
            }
            other @ (ForeignError::SymbolResolve { .. }
            | ForeignError::InvalidSignature { .. }
            | ForeignError::DuplicateDeclaration { .. }) => {
                panic!("expected Invocation, got {other:?}");
            }
        }
    }

    #[test]
    fn declaration_rejects_void_parameter_signature() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let result = ForeignFunction::new(
            lib,
            "getpid",
            Signature::new(vec![NativeType::Void], NativeType::I32),
        );

        let err =
            result.expect_err("invalid signature should fail at declaration");
        match err {
            ForeignError::InvalidSignature { symbol, message } => {
                assert_eq!(symbol, "getpid");
                assert!(message.contains("Void"));
            }
            other @ (ForeignError::SymbolResolve { .. }
            | ForeignError::Invocation { .. }
            | ForeignError::DuplicateDeclaration { .. }) => {
                panic!("expected InvalidSignature, got {other:?}");
            }
        }
    }

    #[test]
    fn value_from_c_str_can_be_used_with_strlen() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let strlen = ForeignFunction::new(
            lib,
            "strlen",
            Signature::new(vec![NativeType::Ptr], NativeType::USize),
        )
        .expect("declare strlen");
        let input = CString::new("hello").expect("literal contains no NUL");

        let result = strlen
            .call(&[Value::from_c_str(input.as_c_str())])
            .expect("invoke strlen");
        match result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
    }

    #[test]
    fn c_string_backing_storage_lives_across_call() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let strlen = ForeignFunction::new(
            lib,
            "strlen",
            Signature::new(vec![NativeType::Ptr], NativeType::USize),
        )
        .expect("declare strlen");
        let input = CString::new("hello").expect("literal contains no NUL");
        let arg = Value::from_c_string(&input);

        let result = strlen.call(&[arg]).expect("invoke strlen");
        match result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
    }

    #[test]
    fn declaration_keeps_library_alive() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let getpid = ForeignFunction::new(
            lib.clone(),
            "getpid",
            Signature::new(vec![], NativeType::I32),
        )
        .expect("declare getpid");

        drop(lib);

        let result = getpid
            .call(&[])
            .expect("invoke after dropping original Arc");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn foreign_function_stores_prepared_metadata_structurally() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let foreign = ForeignFunction::new(
            lib,
            "getpid",
            Signature::new(vec![], NativeType::I32),
        )
        .expect("declare getpid");

        assert_eq!(foreign.prepared.signature(), foreign.signature());
        assert_eq!(foreign.call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn runtime_foreign_function_carries_explicit_call_conv() {
        let lib: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let foreign = ForeignFunction::new_with_call_conv(
            lib,
            "getpid",
            Signature::new(vec![], NativeType::I32),
            ForeignCallConv::C,
        )
        .expect("declare getpid");

        assert_eq!(foreign.call_conv(), ForeignCallConv::C);
    }
}
