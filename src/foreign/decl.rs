use super::{ForeignError, ForeignLibrary};
use crate::dyld::DlError;
use crate::ffi::{ForeignCallConv, Signature};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// Parser-neutral declaration of one foreign library and its function imports.
///
/// `ForeignLibraryDecl` is independent of source-language syntax. It captures
/// a local declaration identity (`library_name`), the runtime library path used
/// for loading (`library_path`), the resolved block-level default foreign
/// calling convention, and the declared functions to lower.
#[derive(Debug, Clone)]
pub struct ForeignLibraryDecl {
    library_name: String,
    library_path: PathBuf,
    default_call_conv: ForeignCallConv,
    functions: Vec<ForeignFunctionDecl>,
}

/// Parser-neutral declaration of one foreign function import.
///
/// `local_name` is the local declaration/lookup name in the lowered runtime
/// library. `symbol_name` is the actual native symbol resolved from the
/// foreign library. `signature` describes the declared call shape.
/// `call_conv` is the explicitly resolved foreign calling convention.
///
/// This declaration does not verify that the declared signature matches the
/// actual native ABI of `symbol_name`.
#[derive(Debug, Clone)]
pub struct ForeignFunctionDecl {
    local_name: String,
    symbol_name: String,
    signature: Signature,
    call_conv: ForeignCallConv,
}

/// Errors produced by structural validation and lowering of foreign declarations.
#[derive(Debug)]
pub enum LoweringError {
    EmptyLibraryName,
    EmptyLocalName,
    EmptySymbolName,
    DuplicateLocalName {
        name: String,
    },
    DuplicateCallConvAttribute {
        context: String,
    },
    LoadLibrary {
        path: PathBuf,
        source: DlError,
    },
    DeclareFunction {
        local_name: String,
        symbol_name: String,
        source: Box<ForeignError>,
    },
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLibraryName => {
                write!(
                    f,
                    "foreign library declaration has an empty library name"
                )
            }
            Self::EmptyLocalName => {
                write!(
                    f,
                    "foreign function declaration has an empty local name"
                )
            }
            Self::EmptySymbolName => {
                write!(
                    f,
                    "foreign function declaration has an empty symbol name"
                )
            }
            Self::DuplicateLocalName { name } => {
                write!(f, "duplicate foreign local function name: {name}")
            }
            Self::DuplicateCallConvAttribute { context } => {
                write!(
                    f,
                    "duplicate foreign call-convention attribute in {context}"
                )
            }
            Self::LoadLibrary { path, source } => {
                write!(
                    f,
                    "failed to load foreign library '{}': {source}",
                    path.display()
                )
            }
            Self::DeclareFunction {
                local_name,
                symbol_name,
                source,
            } => {
                write!(
                    f,
                    "failed to declare foreign function '{local_name}' (symbol '{symbol_name}'): {source}"
                )
            }
        }
    }
}

impl std::error::Error for LoweringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LoadLibrary { source, .. } => Some(source),
            Self::DeclareFunction { source, .. } => Some(source.as_ref()),
            Self::EmptyLibraryName
            | Self::EmptyLocalName
            | Self::EmptySymbolName
            | Self::DuplicateLocalName { .. }
            | Self::DuplicateCallConvAttribute { .. } => None,
        }
    }
}

impl ForeignLibraryDecl {
    /// Creates a parser-neutral foreign library declaration.
    ///
    /// `library_name` is a local declaration identity, and `library_path` is
    /// the path used during lowering to open the runtime library.
    #[must_use]
    pub fn new(
        library_name: impl Into<String>,
        library_path: impl Into<PathBuf>,
        functions: Vec<ForeignFunctionDecl>,
    ) -> Self {
        Self::with_default_call_conv(
            library_name,
            library_path,
            ForeignCallConv::default_foreign(),
            functions,
        )
    }

    /// Creates a parser-neutral foreign library declaration with explicit
    /// default call-convention metadata for contained function declarations.
    #[must_use]
    pub fn with_default_call_conv(
        library_name: impl Into<String>,
        library_path: impl Into<PathBuf>,
        default_call_conv: ForeignCallConv,
        functions: Vec<ForeignFunctionDecl>,
    ) -> Self {
        Self {
            library_name: library_name.into(),
            library_path: library_path.into(),
            default_call_conv,
            functions,
        }
    }

    #[must_use]
    pub fn library_name(&self) -> &str {
        &self.library_name
    }

    #[must_use]
    pub fn library_path(&self) -> &Path {
        &self.library_path
    }

    #[must_use]
    /// Returns the resolved default foreign call convention for this library
    /// declaration.
    pub fn default_call_conv(&self) -> ForeignCallConv {
        self.default_call_conv
    }

    #[must_use]
    pub fn functions(&self) -> &[ForeignFunctionDecl] {
        &self.functions
    }

    pub fn add_function(&mut self, function: ForeignFunctionDecl) {
        self.functions.push(function);
    }

    /// Validates declaration shape without touching the operating system loader.
    ///
    /// Validation checks only structural constraints such as non-empty names
    /// and duplicate local function declarations.
    ///
    /// # Errors
    /// Returns [`LoweringError`] when structural validation fails.
    pub fn validate(&self) -> Result<(), LoweringError> {
        validate_foreign_library_decl(self)
    }

    /// Lowers this declaration into a live runtime [`ForeignLibrary`].
    ///
    /// Lowering validates the declaration, opens the runtime library, and
    /// registers each function declaration by local name while resolving each
    /// native symbol by symbol name.
    ///
    /// # Errors
    /// Returns [`LoweringError`] on structural validation failures, library
    /// load failures, or per-function declaration failures.
    pub fn lower(&self) -> Result<ForeignLibrary, LoweringError> {
        lower_foreign_library_decl(self)
    }
}

impl ForeignFunctionDecl {
    /// Creates a foreign function declaration with explicit local and native names.
    ///
    /// `local_name` is the local declaration/lookup name in the lowered runtime
    /// library, while `symbol_name` is the native symbol resolved from the
    /// foreign library.
    #[must_use]
    pub fn new(
        local_name: impl Into<String>,
        symbol_name: impl Into<String>,
        signature: Signature,
    ) -> Self {
        Self::with_call_conv(
            local_name,
            symbol_name,
            signature,
            ForeignCallConv::default_foreign(),
        )
    }

    /// Creates a foreign function declaration with explicit resolved calling
    /// convention metadata.
    #[must_use]
    pub fn with_call_conv(
        local_name: impl Into<String>,
        symbol_name: impl Into<String>,
        signature: Signature,
        call_conv: ForeignCallConv,
    ) -> Self {
        Self {
            local_name: local_name.into(),
            symbol_name: symbol_name.into(),
            signature,
            call_conv,
        }
    }

    /// Creates a foreign function declaration where local and native names are identical.
    #[must_use]
    pub fn identical_name(
        name: impl Into<String>,
        signature: Signature,
    ) -> Self {
        let name = name.into();
        Self::new(name.clone(), name, signature)
    }

    #[must_use]
    pub fn local_name(&self) -> &str {
        &self.local_name
    }

    #[must_use]
    pub fn symbol_name(&self) -> &str {
        &self.symbol_name
    }

    #[must_use]
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    #[must_use]
    /// Returns the resolved foreign calling convention for this function
    /// declaration.
    pub fn call_conv(&self) -> ForeignCallConv {
        self.call_conv
    }
}

/// Validates a foreign library declaration structurally.
///
/// Validation is purely structural and does not open libraries or resolve
/// symbols.
///
/// # Errors
/// Returns [`LoweringError`] when structural validation fails.
pub fn validate_foreign_library_decl(
    decl: &ForeignLibraryDecl,
) -> Result<(), LoweringError> {
    if decl.library_name.trim().is_empty() {
        return Err(LoweringError::EmptyLibraryName);
    }

    let mut names = BTreeSet::new();
    for function in &decl.functions {
        if function.local_name.trim().is_empty() {
            return Err(LoweringError::EmptyLocalName);
        }
        if function.symbol_name.trim().is_empty() {
            return Err(LoweringError::EmptySymbolName);
        }
        if !names.insert(function.local_name.clone()) {
            return Err(LoweringError::DuplicateLocalName {
                name: function.local_name.clone(),
            });
        }
    }

    Ok(())
}

/// Lowers a foreign library declaration into a live runtime [`ForeignLibrary`].
///
/// Lowering validates declaration shape, opens the declared library path, and
/// registers each function declaration by local name.
///
/// # Errors
/// Returns [`LoweringError`] on structural validation failures, library load
/// failures, or per-function runtime declaration failures.
pub fn lower_foreign_library_decl(
    decl: &ForeignLibraryDecl,
) -> Result<ForeignLibrary, LoweringError> {
    validate_foreign_library_decl(decl)?;

    let mut runtime =
        ForeignLibrary::open(&decl.library_path).map_err(|source| {
            LoweringError::LoadLibrary {
                path: decl.library_path.clone(),
                source,
            }
        })?;

    for function in &decl.functions {
        runtime
            .register_decl_with_call_conv(
                function.local_name.clone(),
                function.symbol_name.clone(),
                function.signature.clone(),
                function.call_conv(),
            )
            .map_err(|source| LoweringError::DeclareFunction {
                local_name: function.local_name.clone(),
                symbol_name: function.symbol_name.clone(),
                source: Box::new(source),
            })?;
    }

    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::NativeType;

    #[test]
    fn function_decl_identical_name_uses_same_local_and_symbol() {
        let sig = Signature::new(vec![NativeType::Ptr], NativeType::USize);
        let decl = ForeignFunctionDecl::identical_name("strlen", sig);
        assert_eq!(decl.local_name(), "strlen");
        assert_eq!(decl.symbol_name(), "strlen");
    }

    #[test]
    fn function_decl_new_preserves_distinct_local_and_symbol_names() {
        let sig = Signature::new(vec![], NativeType::I32);
        let decl = ForeignFunctionDecl::new("pid", "getpid", sig);
        assert_eq!(decl.local_name(), "pid");
        assert_eq!(decl.symbol_name(), "getpid");
        assert_eq!(decl.call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn library_decl_holds_explicit_name_path_and_functions() {
        let sig = Signature::new(vec![], NativeType::I32);
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            "/usr/lib/libSystem.B.dylib",
            vec![ForeignFunctionDecl::identical_name("getpid", sig)],
        );
        assert_eq!(decl.library_name(), "libSystem");
        assert_eq!(decl.default_call_conv(), ForeignCallConv::C);
        assert_eq!(
            decl.library_path(),
            Path::new("/usr/lib/libSystem.B.dylib")
        );
        assert_eq!(decl.functions().len(), 1);
    }

    #[test]
    fn normalized_ir_carries_explicit_call_conv() {
        let decl = ForeignFunctionDecl::with_call_conv(
            "pid",
            "getpid",
            Signature::new(vec![], NativeType::I32),
            ForeignCallConv::C,
        );
        assert_eq!(decl.call_conv(), ForeignCallConv::C);
    }

    #[test]
    fn validate_rejects_empty_library_name() {
        let decl = ForeignLibraryDecl::new(
            "",
            "/usr/lib/libSystem.B.dylib",
            vec![ForeignFunctionDecl::identical_name(
                "getpid",
                Signature::new(vec![], NativeType::I32),
            )],
        );
        assert!(matches!(
            validate_foreign_library_decl(&decl),
            Err(LoweringError::EmptyLibraryName)
        ));
    }

    #[test]
    fn validate_rejects_empty_local_name() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            "/usr/lib/libSystem.B.dylib",
            vec![ForeignFunctionDecl::new(
                "",
                "getpid",
                Signature::new(vec![], NativeType::I32),
            )],
        );
        assert!(matches!(
            validate_foreign_library_decl(&decl),
            Err(LoweringError::EmptyLocalName)
        ));
    }

    #[test]
    fn validate_rejects_empty_symbol_name() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            "/usr/lib/libSystem.B.dylib",
            vec![ForeignFunctionDecl::new(
                "pid",
                "",
                Signature::new(vec![], NativeType::I32),
            )],
        );
        assert!(matches!(
            validate_foreign_library_decl(&decl),
            Err(LoweringError::EmptySymbolName)
        ));
    }

    #[test]
    fn validate_rejects_duplicate_local_names() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            "/usr/lib/libSystem.B.dylib",
            vec![
                ForeignFunctionDecl::new(
                    "pid",
                    "getpid",
                    Signature::new(vec![], NativeType::I32),
                ),
                ForeignFunctionDecl::new(
                    "pid",
                    "getpid",
                    Signature::new(vec![], NativeType::I32),
                ),
            ],
        );
        assert!(matches!(
            validate_foreign_library_decl(&decl),
            Err(LoweringError::DuplicateLocalName { .. })
        ));
    }

    #[test]
    fn validate_allows_duplicate_symbol_names_with_distinct_local_names() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            "/usr/lib/libSystem.B.dylib",
            vec![
                ForeignFunctionDecl::new(
                    "pid1",
                    "getpid",
                    Signature::new(vec![], NativeType::I32),
                ),
                ForeignFunctionDecl::new(
                    "pid2",
                    "getpid",
                    Signature::new(vec![], NativeType::I32),
                ),
            ],
        );
        assert!(validate_foreign_library_decl(&decl).is_ok());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use crate::ffi::{NativeType, Value};
    use std::ffi::CString;

    const LIBSYSTEM_PATH: &str = "/usr/lib/libSystem.B.dylib";

    #[test]
    fn lower_decl_opens_library_and_registers_by_local_name() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            LIBSYSTEM_PATH,
            vec![
                ForeignFunctionDecl::new(
                    "strlen",
                    "strlen",
                    Signature::new(vec![NativeType::Ptr], NativeType::USize),
                ),
                ForeignFunctionDecl::new(
                    "pid",
                    "getpid",
                    Signature::new(vec![], NativeType::I32),
                ),
            ],
        );
        let runtime =
            lower_foreign_library_decl(&decl).expect("lower declaration");
        let strlen = runtime.function("strlen").expect("lookup strlen");
        let pid = runtime.function("pid").expect("lookup pid");
        let input = CString::new("hello").expect("literal contains no NUL");

        let strlen_result = strlen
            .call(&[Value::from_c_string(&input)])
            .expect("call strlen");
        let pid_result = pid.call(&[]).expect("call pid");

        match strlen_result {
            Value::USize(len) => assert_eq!(len, 5),
            other => panic!("expected Value::USize, got {other:?}"),
        }
        match pid_result {
            Value::I32(pid) => assert!(pid > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }

    #[test]
    fn lowering_wraps_bad_symbol_errors_with_local_and_symbol_context() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            LIBSYSTEM_PATH,
            vec![ForeignFunctionDecl::new(
                "bad",
                "definitely_not_a_real_symbol",
                Signature::new(vec![], NativeType::I32),
            )],
        );
        let err = lower_foreign_library_decl(&decl)
            .expect_err("lowering should fail");

        match err {
            LoweringError::DeclareFunction {
                local_name,
                symbol_name,
                ..
            } => {
                assert_eq!(local_name, "bad");
                assert_eq!(symbol_name, "definitely_not_a_real_symbol");
            }
            other => panic!("expected DeclareFunction, got {other:?}"),
        }
    }

    #[test]
    fn lowering_allows_aliasing_without_lookup_by_native_symbol_name() {
        let decl = ForeignLibraryDecl::new(
            "libSystem",
            LIBSYSTEM_PATH,
            vec![ForeignFunctionDecl::new(
                "pid",
                "getpid",
                Signature::new(vec![], NativeType::I32),
            )],
        );
        let runtime =
            lower_foreign_library_decl(&decl).expect("lower declaration");

        assert!(runtime.function("pid").is_some());
        assert!(runtime.function("getpid").is_none());

        let pid = runtime.function("pid").expect("lookup pid");
        let result = pid.call(&[]).expect("call pid");
        match result {
            Value::I32(v) => assert!(v > 0),
            other => panic!("expected Value::I32, got {other:?}"),
        }
    }
}
