use super::signatures::TypedFunctionSignature;
use std::collections::BTreeMap;

/// Lookup-only semantic context for call/type queries outside the current
/// target-local semantic tables.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExternalSemanticLookup {
    named_roots: BTreeMap<String, ExternalNamedRoot>,
    extern_libraries: BTreeMap<String, ExternalExternLibrary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExternalNamedRoot {
    functions_by_path: BTreeMap<Vec<String>, TypedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ExternalExternLibrary {
    functions_by_name: BTreeMap<String, TypedFunctionSignature>,
}

impl ExternalSemanticLookup {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_named_root_function(
        &mut self,
        root_name: String,
        path: Vec<String>,
        signature: TypedFunctionSignature,
    ) {
        self.named_roots
            .entry(root_name)
            .or_default()
            .functions_by_path
            .insert(path, signature);
    }

    #[must_use]
    pub fn function_for_named_root_path(
        &self,
        root_name: &str,
        path: &[String],
    ) -> Option<&TypedFunctionSignature> {
        self.named_roots.get(root_name)?.functions_by_path.get(path)
    }

    pub fn insert_extern_function(
        &mut self,
        library_name: String,
        function_name: String,
        signature: TypedFunctionSignature,
    ) {
        self.extern_libraries
            .entry(library_name)
            .or_default()
            .functions_by_name
            .insert(function_name, signature);
    }

    #[must_use]
    pub fn extern_function_signature(
        &self,
        library_name: &str,
        function_name: &str,
    ) -> Option<&TypedFunctionSignature> {
        self.extern_libraries
            .get(library_name)?
            .functions_by_name
            .get(function_name)
    }

    #[must_use]
    pub fn is_extern_library(&self, library_name: &str) -> bool {
        self.extern_libraries.contains_key(library_name)
    }

    #[must_use]
    pub fn extern_namespace_for_function(
        &self,
        function_name: &str,
    ) -> Option<&str> {
        self.extern_libraries
            .iter()
            .find_map(|(library, functions)| {
                functions
                    .functions_by_name
                    .contains_key(function_name)
                    .then_some(library.as_str())
            })
    }
}
