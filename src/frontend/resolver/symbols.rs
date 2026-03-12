use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// Top-level declaration kinds collected into scope symbol tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Scope,
    Function,
    Struct,
    Enum,
    Protocol,
}

/// Collected top-level declaration symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub defining_file_id: FileId,
}

/// Symbol table for one resolved scope file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSymbols {
    pub file_id: FileId,
    pub symbols: BTreeMap<String, Symbol>,
}

impl ScopeSymbols {
    /// Returns a symbol by local name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    /// Returns the number of collected symbols.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true when no symbols are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}
