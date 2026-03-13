use super::item_table::{GlobalItemTable, ItemKind};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

/// Compatibility alias backed by canonical item kinds.
pub type SymbolKind = ItemKind;

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

/// Builds compatibility scope symbol tables from the canonical item table.
#[must_use]
pub fn scope_symbols_from_global_item_table(
    table: &GlobalItemTable,
) -> BTreeMap<FileId, ScopeSymbols> {
    let mut by_scope = BTreeMap::new();

    for item in table.iter() {
        let scope_symbols = by_scope
            .entry(item.containing_scope_file_id)
            .or_insert_with(|| ScopeSymbols {
                file_id: item.containing_scope_file_id,
                symbols: BTreeMap::new(),
            });

        scope_symbols
            .symbols
            .entry(item.name.clone())
            .or_insert(Symbol {
                name: item.name.clone(),
                kind: item.kind,
                defining_file_id: item.defining_file_id,
            });
    }

    by_scope
}
