use crate::frontend::hir::{
    HirFile, HirItemId, HirItemKind, HirModule, HirTypeId,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// File-scoped reference to one lowered HIR item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirItemRef {
    pub file_id: FileId,
    pub item_id: HirItemId,
}

impl HirItemRef {
    #[must_use]
    pub const fn new(file_id: FileId, item_id: HirItemId) -> Self {
        Self { file_id, item_id }
    }
}

/// Canonical top-level HIR item kind tracked by the resolver item table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirCollectedItemKind {
    Function,
    Struct,
    Enum,
    Protocol,
    Impl,
    Extern,
}

/// Collected global HIR item entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirCollectedItem {
    pub item_ref: HirItemRef,
    pub kind: HirCollectedItemKind,
    pub name: String,
    pub file_id: FileId,
}

/// HIR item table build failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirItemTableError {
    MissingModule {
        file_id: FileId,
    },
    MissingItem {
        item_ref: HirItemRef,
    },
    DuplicateName {
        name: String,
        first: HirItemRef,
        duplicate: HirItemRef,
    },
}

impl Display for HirItemTableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModule { file_id } => {
                write!(f, "missing HIR module for file id {}", file_id.raw())
            }
            Self::MissingItem { item_ref } => write!(
                f,
                "missing HIR item {} in file id {}",
                item_ref.item_id.raw(),
                item_ref.file_id.raw()
            ),
            Self::DuplicateName {
                name,
                first,
                duplicate,
            } => write!(
                f,
                "duplicate HIR item name '{}' between file {} item {} and file {} item {}",
                name,
                first.file_id.raw(),
                first.item_id.raw(),
                duplicate.file_id.raw(),
                duplicate.item_id.raw()
            ),
        }
    }
}

impl std::error::Error for HirItemTableError {}

/// Deterministic global HIR item table.
///
/// This table only tracks root-level named items and intentionally does not
/// resolve paths or bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirItemTable {
    items_by_ref: BTreeMap<HirItemRef, HirCollectedItem>,
    by_name: BTreeMap<String, HirItemRef>,
    by_file_id: BTreeMap<FileId, Vec<HirItemRef>>,
}

impl HirItemTable {
    /// Collects top-level HIR items from lowered files/modules.
    ///
    /// Collected kinds: function, struct, enum, protocol, impl, extern.
    ///
    /// # Errors
    ///
    /// Returns [`HirItemTableError::DuplicateName`] when two collected items
    /// share the same logical file scope + local name.
    pub fn collect(
        hir_files: &[HirFile],
        hir_modules: &BTreeMap<FileId, HirModule>,
    ) -> Result<Self, HirItemTableError> {
        let mut items_by_ref = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        let mut by_file_id: BTreeMap<FileId, Vec<HirItemRef>> = BTreeMap::new();
        let mut by_file_and_name = BTreeMap::new();

        for hir_file in hir_files {
            let module = hir_modules.get(&hir_file.file_id).ok_or(
                HirItemTableError::MissingModule {
                    file_id: hir_file.file_id,
                },
            )?;

            for item_id in &hir_file.root_items {
                let item_ref = HirItemRef::new(hir_file.file_id, *item_id);
                let item = module
                    .items
                    .get(item_id)
                    .ok_or(HirItemTableError::MissingItem { item_ref })?;

                let Some((kind, name)) =
                    collected_item_kind_and_name(&item.kind, module)
                else {
                    continue;
                };

                let file_and_name = (hir_file.file_id, name.clone());
                if let Some(first) =
                    by_file_and_name.get(&file_and_name).copied()
                {
                    return Err(HirItemTableError::DuplicateName {
                        name,
                        first,
                        duplicate: item_ref,
                    });
                }

                by_file_and_name.insert(file_and_name, item_ref);
                // Preserve deterministic first-definition lookup by bare name.
                by_name.entry(name.clone()).or_insert(item_ref);
                by_file_id
                    .entry(hir_file.file_id)
                    .or_default()
                    .push(item_ref);
                items_by_ref.insert(
                    item_ref,
                    HirCollectedItem {
                        item_ref,
                        kind,
                        name,
                        file_id: hir_file.file_id,
                    },
                );
            }
        }

        Ok(Self {
            items_by_ref,
            by_name,
            by_file_id,
        })
    }

    #[must_use]
    pub fn get(&self, item_ref: HirItemRef) -> Option<&HirCollectedItem> {
        self.items_by_ref.get(&item_ref)
    }

    #[must_use]
    pub fn item_ref_by_name(&self, name: &str) -> Option<HirItemRef> {
        self.by_name.get(name).copied()
    }

    #[must_use]
    pub fn item_refs_in_file(&self, file_id: FileId) -> &[HirItemRef] {
        self.by_file_id
            .get(&file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &HirCollectedItem> {
        self.items_by_ref.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items_by_ref.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items_by_ref.is_empty()
    }
}

/// Builds a [`HirItemTable`] from lowered project HIR files/modules.
///
/// # Errors
///
/// Propagates any collection failure from [`HirItemTable::collect`].
pub fn build_hir_item_table(
    hir_files: &[HirFile],
    hir_modules: &BTreeMap<FileId, HirModule>,
) -> Result<HirItemTable, HirItemTableError> {
    HirItemTable::collect(hir_files, hir_modules)
}

fn collected_item_kind_and_name(
    item_kind: &HirItemKind,
    module: &HirModule,
) -> Option<(HirCollectedItemKind, String)> {
    match item_kind {
        HirItemKind::Function(function) => {
            Some((HirCollectedItemKind::Function, function.name.clone()))
        }
        HirItemKind::Struct(struct_decl) => {
            Some((HirCollectedItemKind::Struct, struct_decl.name.clone()))
        }
        HirItemKind::Enum(enum_decl) => {
            Some((HirCollectedItemKind::Enum, enum_decl.name.clone()))
        }
        HirItemKind::Protocol(protocol_decl) => {
            Some((HirCollectedItemKind::Protocol, protocol_decl.name.clone()))
        }
        HirItemKind::Impl(impl_decl) => Some((
            HirCollectedItemKind::Impl,
            impl_display_name(impl_decl, module),
        )),
        HirItemKind::Extern(extern_block) => Some((
            HirCollectedItemKind::Extern,
            format!("extern {}", extern_block.library_name),
        )),
        HirItemKind::Use(_) => None,
    }
}

fn impl_display_name(
    impl_decl: &crate::frontend::hir::HirImpl,
    module: &HirModule,
) -> String {
    let target = render_hir_type(module, impl_decl.target);
    match impl_decl.conformance {
        Some(conformance) => format!(
            "impl {} as {}",
            target,
            render_hir_type(module, conformance)
        ),
        None => format!("impl {target}"),
    }
}

fn render_hir_type(module: &HirModule, type_id: HirTypeId) -> String {
    let Some(ty) = module.types.get(&type_id) else {
        return "<missing-type>".to_string();
    };

    match &ty.kind {
        crate::frontend::hir::HirTypeKind::Path(path) => {
            if path.segments.is_empty() {
                "<empty-path>".to_string()
            } else {
                path.segments.join("::")
            }
        }
        crate::frontend::hir::HirTypeKind::Reference { mutable, inner } => {
            let inner = render_hir_type(module, *inner);
            if *mutable {
                format!("&mut {inner}")
            } else {
                format!("&{inner}")
            }
        }
        crate::frontend::hir::HirTypeKind::Pointer { mutable, inner } => {
            let inner = render_hir_type(module, *inner);
            if *mutable {
                format!("*mut {inner}")
            } else {
                format!("*const {inner}")
            }
        }
        crate::frontend::hir::HirTypeKind::Optional { inner } => {
            format!("{}?", render_hir_type(module, *inner))
        }
        crate::frontend::hir::HirTypeKind::Result { ok, err } => format!(
            "Result<{}, {}>",
            render_hir_type(module, *ok),
            render_hir_type(module, *err)
        ),
        crate::frontend::hir::HirTypeKind::GenericApplication {
            base,
            args,
        } => {
            let base_name = render_hir_type(module, *base);
            let rendered_args = args
                .iter()
                .map(|arg| render_hir_type(module, *arg))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{base_name}<{rendered_args}>")
        }
        crate::frontend::hir::HirTypeKind::SelfType => "Self".to_string(),
    }
}
