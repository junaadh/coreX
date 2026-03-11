use super::{ForeignError, ForeignFunction};
use crate::dyld::{DlError, Library};
use crate::ffi::{ForeignCallConv, Signature};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Loaded foreign library with a declaration surface for functions from that library.
///
/// This type retains shared ownership of the underlying loaded library and
/// provides a compact API to declare reusable [`ForeignFunction`] values.
pub struct ForeignLibrary {
    library: Arc<Library>,
    functions: BTreeMap<String, ForeignFunction>,
}

impl ForeignLibrary {
    /// Loads a foreign library from `path` and retains it for future declarations.
    ///
    /// # Errors
    /// Returns [`DlError`] if the library cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DlError> {
        let library = Arc::from(Library::open(path)?);
        Ok(Self {
            library,
            functions: BTreeMap::new(),
        })
    }

    /// Wraps an existing shared loaded library handle.
    #[must_use]
    pub fn from_arc(library: Arc<Library>) -> Self {
        Self {
            library,
            functions: BTreeMap::new(),
        }
    }

    /// Declares a reusable foreign function from this library.
    ///
    /// Declaration performs eager symbol resolution and call-metadata
    /// preparation through [`ForeignFunction::new`].
    ///
    /// Declaring a signature does not verify that the signature matches the
    /// actual native ABI of the target symbol.
    ///
    /// # Errors
    /// Returns [`ForeignError`] when declaration fails.
    pub fn declare(
        &self,
        symbol_name: impl Into<String>,
        signature: Signature,
    ) -> Result<ForeignFunction, ForeignError> {
        self.declare_with_call_conv(
            symbol_name,
            signature,
            ForeignCallConv::default_foreign(),
        )
    }

    /// Declares a reusable foreign function with explicit call-convention
    /// metadata.
    ///
    /// # Errors
    /// Returns [`ForeignError`] when declaration fails.
    pub fn declare_with_call_conv(
        &self,
        symbol_name: impl Into<String>,
        signature: Signature,
        call_conv: ForeignCallConv,
    ) -> Result<ForeignFunction, ForeignError> {
        ForeignFunction::new_with_call_conv(
            self.library.clone(),
            symbol_name,
            signature,
            call_conv,
        )
    }

    /// Declares and stores a reusable foreign function in this library instance.
    ///
    /// Stored declarations are local to this `ForeignLibrary` and can be
    /// retrieved with [`Self::function`].
    ///
    /// Symbol registration rejects duplicates by symbol name.
    ///
    /// # Errors
    /// Returns:
    /// - [`ForeignError::DuplicateDeclaration`] when `symbol_name` is already
    ///   registered in this instance.
    /// - any declaration error returned by [`ForeignFunction::new`].
    pub fn register(
        &mut self,
        symbol_name: impl Into<String>,
        signature: Signature,
    ) -> Result<(), ForeignError> {
        let symbol_name = symbol_name.into();
        self.register_decl_with_call_conv(
            symbol_name.clone(),
            symbol_name,
            signature,
            ForeignCallConv::default_foreign(),
        )
    }

    /// Declares and stores a function with explicit local and native symbol names.
    ///
    /// The function is stored under `local_name` for lookup through
    /// [`Self::function`], and resolves the native `symbol_name` eagerly.
    ///
    /// # Errors
    /// Returns:
    /// - [`ForeignError::DuplicateDeclaration`] when `local_name` is already
    ///   registered in this instance.
    /// - any declaration error returned by [`ForeignFunction::new`].
    pub fn register_decl(
        &mut self,
        local_name: impl Into<String>,
        symbol_name: impl Into<String>,
        signature: Signature,
    ) -> Result<(), ForeignError> {
        self.register_decl_with_call_conv(
            local_name,
            symbol_name,
            signature,
            ForeignCallConv::default_foreign(),
        )
    }

    /// Declares and stores a function with explicit local/native names and
    /// explicit resolved call-convention metadata.
    ///
    /// # Errors
    /// Returns:
    /// - [`ForeignError::DuplicateDeclaration`] when `local_name` is already
    ///   registered in this instance.
    /// - any declaration error returned by [`ForeignFunction::new_with_call_conv`].
    pub fn register_decl_with_call_conv(
        &mut self,
        local_name: impl Into<String>,
        symbol_name: impl Into<String>,
        signature: Signature,
        call_conv: ForeignCallConv,
    ) -> Result<(), ForeignError> {
        let local_name = local_name.into();
        let symbol_name = symbol_name.into();
        let function = ForeignFunction::new_with_call_conv(
            self.library.clone(),
            symbol_name,
            signature,
            call_conv,
        )?;

        match self.functions.entry(local_name.clone()) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(function);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(ForeignError::DuplicateDeclaration { symbol: local_name })
            }
        }
    }

    /// Returns a previously registered foreign function by symbol name.
    #[must_use]
    pub fn function(&self, symbol_name: &str) -> Option<&ForeignFunction> {
        self.functions.get(symbol_name)
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        self.library.as_ref()
    }
}

impl std::fmt::Debug for ForeignLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeignLibrary")
            .field("library_path", &self.library.path())
            .field("registered_functions", &self.functions.len())
            .finish()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::ffi::{ForeignCallConv, NativeType, Value};
    use std::ffi::CString;

    const LIBSYSTEM_PATH: &str = "/usr/lib/libSystem.B.dylib";

    #[test]
    fn open_foreign_library_and_declare_getpid() {
        let lib = ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        let getpid = lib
            .declare("getpid", Signature::new(vec![], NativeType::I32))
            .expect("declare getpid");
        assert_eq!(getpid.call_conv(), ForeignCallConv::C);

        let result = getpid.call(&[]).expect("call getpid");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn declare_multiple_functions_from_same_library() {
        let lib = ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        let strlen = lib
            .declare(
                "strlen",
                Signature::new(vec![NativeType::Ptr], NativeType::USize),
            )
            .expect("declare strlen");
        let puts = lib
            .declare(
                "puts",
                Signature::new(vec![NativeType::Ptr], NativeType::I32),
            )
            .expect("declare puts");
        let getpid = lib
            .declare("getpid", Signature::new(vec![], NativeType::I32))
            .expect("declare getpid");
        let input = CString::new("hello").expect("literal contains no NUL");

        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let puts_result = puts
            .call(&[Value::from_c_string(&input)])
            .expect("call puts");
        let pid_result = getpid.call(&[]).expect("call getpid");

        match strlen_result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match puts_result {
            Value::I32(rc) => assert!(rc >= 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn foreign_library_from_arc_preserves_existing_library_handle() {
        let arc: Arc<Library> =
            Arc::from(Library::open(LIBSYSTEM_PATH).expect("open libSystem"));
        let lib = ForeignLibrary::from_arc(arc);
        let getpid = lib
            .declare("getpid", Signature::new(vec![], NativeType::I32))
            .expect("declare getpid");

        let result = getpid.call(&[]).expect("call getpid");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn declare_bad_symbol_propagates_foreign_error() {
        let lib = ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        let err = lib
            .declare(
                "__definitely_not_a_real_symbol__",
                Signature::new(vec![], NativeType::I32),
            )
            .expect_err("bad symbol should fail");
        assert!(matches!(err, ForeignError::SymbolResolve { .. }));
    }

    #[test]
    fn declaration_from_library_keeps_symbol_usable() {
        let lib = ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        let getpid = lib
            .declare("getpid", Signature::new(vec![], NativeType::I32))
            .expect("declare getpid");
        drop(lib);

        let result = getpid.call(&[]).expect("call after dropping library");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn register_and_lookup_function() {
        let mut lib =
            ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        lib.register("getpid", Signature::new(vec![], NativeType::I32))
            .expect("register getpid");

        let getpid = lib.function("getpid").expect("lookup getpid");
        let result = getpid.call(&[]).expect("call getpid");
        match result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn register_duplicate_symbol_rejected() {
        let mut lib =
            ForeignLibrary::open(LIBSYSTEM_PATH).expect("open libSystem");
        lib.register("getpid", Signature::new(vec![], NativeType::I32))
            .expect("first register should succeed");

        let err = lib
            .register("getpid", Signature::new(vec![], NativeType::I32))
            .expect_err("duplicate register should fail");
        assert!(matches!(err, ForeignError::DuplicateDeclaration { .. }));
    }
}
