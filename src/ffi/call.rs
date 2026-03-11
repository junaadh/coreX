use super::{CallError, ForeignCallConv, NativeType, Signature, Value};
use crate::dyld::RawSymbol;
use core::ffi::c_void;
use libffi::middle::{Arg, Cif, CodePtr, Type};

enum ArgStorage {
    I32(i32),
    USize(usize),
    Ptr(*mut c_void),
}

/// Reusable call metadata derived from a foreign function signature.
///
/// `PreparedCall` owns the libffi metadata needed to invoke any symbol that
/// follows the stored signature. It can be reused across many invocations.
///
/// This type does not own a callee symbol. A symbol is provided at call-time.
pub struct PreparedCall {
    signature: Signature,
    call_conv: ForeignCallConv,
    cif: Cif,
}

impl std::fmt::Debug for PreparedCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCall")
            .field("signature", &self.signature)
            .field("call_conv", &self.call_conv)
            .finish_non_exhaustive()
    }
}

impl PreparedCall {
    /// Validates and prepares reusable call metadata from `signature` using
    /// the default foreign call convention.
    ///
    /// # Errors
    /// Returns [`CallError::UnsupportedType`] when the signature contains
    /// unsupported argument types.
    pub fn new(signature: Signature) -> Result<Self, CallError> {
        Self::new_with_call_conv(signature, ForeignCallConv::default_foreign())
    }

    /// Validates and prepares reusable call metadata from `signature` and
    /// explicit foreign calling-convention metadata.
    ///
    /// The current runtime behavior supports C calling convention only.
    ///
    /// # Errors
    /// Returns [`CallError::UnsupportedType`] when the signature contains
    /// unsupported argument types.
    pub fn new_with_call_conv(
        signature: Signature,
        call_conv: ForeignCallConv,
    ) -> Result<Self, CallError> {
        let cif = prepare_cif(&signature)?;
        Ok(Self {
            signature,
            call_conv,
            cif,
        })
    }

    #[must_use]
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    #[must_use]
    /// Returns the explicit resolved foreign calling convention for this
    /// prepared call metadata.
    pub fn call_conv(&self) -> ForeignCallConv {
        self.call_conv
    }

    /// Invokes `symbol` using this prepared call metadata.
    ///
    /// # Errors
    /// Returns:
    /// - [`CallError::NullSymbol`] if the symbol address is null.
    /// - [`CallError::ArityMismatch`] if argument count differs from signature.
    /// - [`CallError::TypeMismatch`] if argument types differ from signature.
    /// - [`CallError::UnsupportedType`] for unsupported signature/value types.
    ///
    /// Pointer-backed arguments remain non-owning. Callers must keep backing
    /// storage alive for the duration of this call.
    pub fn call(
        &self,
        symbol: &RawSymbol,
        args: &[Value],
    ) -> Result<Value, CallError> {
        call_prepared(self, symbol, args)
    }
}

/// Invokes `symbol` using prepared reusable call metadata.
///
/// # Errors
/// Returns:
/// - [`CallError::NullSymbol`] if the symbol address is null.
/// - [`CallError::ArityMismatch`] if argument count differs from signature.
/// - [`CallError::TypeMismatch`] if argument types differ from signature.
/// - [`CallError::UnsupportedType`] for unsupported signature/value types.
///
/// Pointer-backed arguments remain non-owning. Callers must keep backing
/// storage alive for the duration of this call.
pub fn call_prepared(
    prepared: &PreparedCall,
    symbol: &RawSymbol,
    args: &[Value],
) -> Result<Value, CallError> {
    if symbol.is_null() {
        return Err(CallError::NullSymbol);
    }

    preflight(prepared.signature(), args)?;

    let arg_storage = marshal_args(args)?;
    let ffi_args = build_ffi_args(&arg_storage);
    let fn_ptr = CodePtr::from_ptr(symbol.as_ptr());

    Ok(call_with_cif(
        &prepared.cif,
        fn_ptr,
        prepared.signature().ret(),
        &ffi_args,
    ))
}

/// Convenience wrapper that prepares call metadata from `sig` for this call.
///
/// Prefer [`PreparedCall`] + [`call_prepared`] for repeated invocations with
/// the same signature.
///
/// # Errors
/// Returns:
/// - [`CallError::NullSymbol`] if the symbol address is null.
/// - [`CallError::ArityMismatch`] if argument count differs from signature.
/// - [`CallError::TypeMismatch`] if argument types differ from signature.
/// - [`CallError::UnsupportedType`] for unsupported signature/value types.
pub fn call_symbol(
    symbol: &RawSymbol,
    sig: &Signature,
    args: &[Value],
) -> Result<Value, CallError> {
    let prepared = PreparedCall::new_with_call_conv(
        sig.clone(),
        ForeignCallConv::default_foreign(),
    )?;
    call_prepared(&prepared, symbol, args)
}

fn prepare_cif(signature: &Signature) -> Result<Cif, CallError> {
    let ffi_param_types = signature
        .params()
        .iter()
        .copied()
        .map(ffi_type_for_param)
        .collect::<Result<Vec<_>, _>>()?;
    let ffi_ret_type = ffi_type_for(signature.ret());
    Ok(Cif::new(ffi_param_types, ffi_ret_type))
}

fn preflight(sig: &Signature, args: &[Value]) -> Result<(), CallError> {
    if sig.params().len() != args.len() {
        return Err(CallError::ArityMismatch {
            expected: sig.params().len(),
            actual: args.len(),
        });
    }

    for (index, expected) in sig.params().iter().copied().enumerate() {
        if expected == NativeType::Void {
            return Err(CallError::UnsupportedType { ty: expected });
        }

        let actual = &args[index];
        if !value_matches(expected, actual) {
            return Err(CallError::TypeMismatch {
                index,
                expected,
                actual: actual.type_name(),
            });
        }
    }

    Ok(())
}

fn value_matches(expected: NativeType, value: &Value) -> bool {
    matches!(
        (expected, value),
        (NativeType::I32, Value::I32(_))
            | (NativeType::USize, Value::USize(_))
            | (NativeType::Ptr, Value::Ptr(_))
    )
}

fn marshal_args(args: &[Value]) -> Result<Vec<ArgStorage>, CallError> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        let slot = match arg {
            Value::I32(v) => ArgStorage::I32(*v),
            Value::USize(v) => ArgStorage::USize(*v),
            Value::Ptr(v) => ArgStorage::Ptr(*v),
            Value::Void => {
                return Err(CallError::UnsupportedType {
                    ty: NativeType::Void,
                });
            }
        };
        out.push(slot);
    }

    Ok(out)
}

fn build_ffi_args(storage: &[ArgStorage]) -> Vec<Arg<'_>> {
    storage
        .iter()
        .map(|slot| match slot {
            ArgStorage::I32(v) => Arg::new(v),
            ArgStorage::USize(v) => Arg::new(v),
            ArgStorage::Ptr(v) => Arg::new(v),
        })
        .collect()
}

fn ffi_type_for_param(ty: NativeType) -> Result<Type, CallError> {
    if ty == NativeType::Void {
        return Err(CallError::UnsupportedType { ty });
    }
    Ok(ffi_type_for(ty))
}

fn ffi_type_for(ty: NativeType) -> Type {
    match ty {
        NativeType::Void => Type::void(),
        NativeType::I32 => Type::i32(),
        NativeType::USize => Type::usize(),
        NativeType::Ptr => Type::pointer(),
    }
}

fn call_with_cif(
    cif: &Cif,
    fn_ptr: CodePtr,
    ret_ty: NativeType,
    args: &[Arg<'_>],
) -> Value {
    match ret_ty {
        NativeType::Void => {
            // Safety: `cif`, `args`, and `fn_ptr` are prepared from validated signature/value data.
            unsafe { cif.call::<()>(fn_ptr, args) };
            Value::Void
        }
        NativeType::I32 => {
            // Safety: return type and arguments were validated before invocation.
            let out = unsafe { cif.call::<i32>(fn_ptr, args) };
            Value::I32(out)
        }
        NativeType::USize => {
            // Safety: return type and arguments were validated before invocation.
            let out = unsafe { cif.call::<usize>(fn_ptr, args) };
            Value::USize(out)
        }
        NativeType::Ptr => {
            // Safety: return type and arguments were validated before invocation.
            let out = unsafe { cif.call::<*mut c_void>(fn_ptr, args) };
            Value::Ptr(out)
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::dyld::Library;
    use crate::ffi::ForeignCallConv;
    use std::ffi::CString;

    const LIBSYSTEM_PATH: &str = "/usr/lib/libSystem.B.dylib";

    #[test]
    fn prepared_call_new_accepts_simple_signature() {
        let sig = Signature::new(vec![], NativeType::I32);
        let prepared =
            PreparedCall::new(sig.clone()).expect("prepare should succeed");
        assert_eq!(prepared.signature(), &sig);
        assert_eq!(prepared.call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn prepared_call_carries_explicit_call_conv() {
        let prepared = PreparedCall::new_with_call_conv(
            Signature::new(vec![], NativeType::I32),
            ForeignCallConv::C,
        )
        .expect("prepare should succeed");
        assert_eq!(prepared.call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn prepared_call_rejects_void_parameter_signature() {
        let sig = Signature::new(vec![NativeType::Void], NativeType::I32);
        let err = PreparedCall::new(sig).expect_err("void parameter must fail");
        assert!(matches!(
            err,
            CallError::UnsupportedType {
                ty: NativeType::Void
            }
        ));
    }

    #[test]
    fn prepared_call_can_invoke_getpid() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let symbol = lib.symbol("getpid").expect("getpid should resolve");
        let prepared =
            PreparedCall::new(Signature::new(vec![], NativeType::I32))
                .expect("prepare getpid");

        let result = call_prepared(&prepared, &symbol, &[])
            .expect("call should succeed");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn prepared_call_can_invoke_strlen() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let symbol = lib.symbol("strlen").expect("strlen should resolve");
        let prepared = PreparedCall::new(Signature::new(
            vec![NativeType::Ptr],
            NativeType::USize,
        ))
        .expect("prepare strlen");
        let input = CString::new("hello").expect("literal contains no NUL");

        let result =
            call_prepared(&prepared, &symbol, &[Value::from_c_string(&input)])
                .expect("call should succeed");
        match result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
    }

    #[test]
    fn prepared_call_reuse_multiple_invocations() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let symbol = lib.symbol("strlen").expect("strlen should resolve");
        let prepared = PreparedCall::new(Signature::new(
            vec![NativeType::Ptr],
            NativeType::USize,
        ))
        .expect("prepare strlen");
        let first = CString::new("hello").expect("literal contains no NUL");
        let second = CString::new("goodbye").expect("literal contains no NUL");

        let first_result = prepared
            .call(&symbol, &[Value::from_c_string(&first)])
            .expect("first call should succeed");
        let second_result = prepared
            .call(&symbol, &[Value::from_c_string(&second)])
            .expect("second call should succeed");

        match first_result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match second_result {
            Value::USize(len) => assert_eq!(len, 7),
            other => panic!("expected Value::USize, got {other:?}"),
        }
    }

    #[test]
    fn prepared_call_null_symbol_is_rejected() {
        let null_symbol = RawSymbol::null_for_test();
        let prepared =
            PreparedCall::new(Signature::new(vec![], NativeType::I32))
                .expect("prepare");
        let err = call_prepared(&prepared, &null_symbol, &[])
            .expect_err("null symbol should fail");
        assert!(matches!(err, CallError::NullSymbol));
    }

    #[test]
    fn prepared_call_argument_type_mismatch_still_fails() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let symbol = lib.symbol("strlen").expect("strlen should resolve");
        let prepared = PreparedCall::new(Signature::new(
            vec![NativeType::Ptr],
            NativeType::USize,
        ))
        .expect("prepare strlen");

        let err = call_prepared(&prepared, &symbol, &[Value::I32(123)])
            .expect_err("type mismatch expected");
        assert!(matches!(
            err,
            CallError::TypeMismatch {
                index: 0,
                expected: NativeType::Ptr,
                actual: "I32"
            }
        ));
    }

    #[test]
    fn call_symbol_still_works_as_compatibility_wrapper() {
        let lib = Library::open(LIBSYSTEM_PATH).expect("libSystem should open");
        let symbol = lib.symbol("getpid").expect("getpid should resolve");
        let sig = Signature::new(vec![], NativeType::I32);

        let result =
            call_symbol(&symbol, &sig, &[]).expect("call should succeed");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }
}
