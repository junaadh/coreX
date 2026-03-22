use super::Type;
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ItemId, ItemKind,
};
use crate::frontend::source::FileId;
use crate::midend::type_check::signatures::{
    TypedEnumSignatureData, TypedFunctionSignature, TypedImplSignature,
    TypedProtocolSignatureData, TypedSignatureTable, TypedStructSignatureData,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypedItemKind {
    Function,
    Struct,
    Enum,
    Protocol,
}

impl TypedItemKind {
    #[must_use]
    pub const fn from_item_kind(kind: ItemKind) -> Option<Self> {
        match kind {
            ItemKind::Function => Some(Self::Function),
            ItemKind::Struct => Some(Self::Struct),
            ItemKind::Enum => Some(Self::Enum),
            ItemKind::Protocol => Some(Self::Protocol),
            ItemKind::Scope => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedItemData {
    Function(TypedFunctionSignature),
    Struct(TypedStructSignatureData),
    Enum(TypedEnumSignatureData),
    Protocol(TypedProtocolSignatureData),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedImplAttachment {
    pub owner: DeclarationOwner,
    pub containing_scope_file_id: FileId,
    pub target_item_id: Option<ItemId>,
    pub conformance_item_id: Option<ItemId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedItemTableIssueKind {
    MissingSignatureForGlobalItem {
        item_kind: TypedItemKind,
    },
    SignatureWithoutGlobalItem {
        signature_kind: TypedItemKind,
    },
    SignatureKindMismatch {
        global_kind: ItemKind,
        signature_kind: TypedItemKind,
    },
    DuplicateImplOwner {
        owner: DeclarationOwner,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedItemTableIssue {
    pub associated_item_id: Option<ItemId>,
    pub kind: TypedItemTableIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedItemTable {
    by_item_id: BTreeMap<ItemId, TypedItemData>,
    by_kind: BTreeMap<TypedItemKind, Vec<ItemId>>,
    by_scope_file_id: BTreeMap<FileId, Vec<ItemId>>,
    impl_signatures_by_owner: BTreeMap<DeclarationOwner, TypedImplSignature>,
    impl_attachments_by_owner: BTreeMap<DeclarationOwner, TypedImplAttachment>,
    impl_owners_by_target_item_id: BTreeMap<ItemId, Vec<DeclarationOwner>>,
    pub issues: Vec<TypedItemTableIssue>,
}

impl TypedItemTable {
    #[must_use]
    pub fn get(&self, item_id: ItemId) -> Option<&TypedItemData> {
        self.by_item_id.get(&item_id)
    }

    #[must_use]
    pub fn function(&self, item_id: ItemId) -> Option<&TypedFunctionSignature> {
        match self.by_item_id.get(&item_id) {
            Some(TypedItemData::Function(signature)) => Some(signature),
            _ => None,
        }
    }

    #[must_use]
    pub fn struct_data(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedStructSignatureData> {
        match self.by_item_id.get(&item_id) {
            Some(TypedItemData::Struct(signature)) => Some(signature),
            _ => None,
        }
    }

    #[must_use]
    pub fn enum_data(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedEnumSignatureData> {
        match self.by_item_id.get(&item_id) {
            Some(TypedItemData::Enum(signature)) => Some(signature),
            _ => None,
        }
    }

    #[must_use]
    pub fn protocol(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedProtocolSignatureData> {
        match self.by_item_id.get(&item_id) {
            Some(TypedItemData::Protocol(signature)) => Some(signature),
            _ => None,
        }
    }

    #[must_use]
    pub fn ids_for_kind(&self, kind: TypedItemKind) -> &[ItemId] {
        self.by_kind.get(&kind).map(Vec::as_slice).unwrap_or(&[])
    }

    #[must_use]
    pub fn ids_in_scope(&self, scope_file_id: FileId) -> &[ItemId] {
        self.by_scope_file_id
            .get(&scope_file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn impl_signature(
        &self,
        owner: &DeclarationOwner,
    ) -> Option<&TypedImplSignature> {
        self.impl_signatures_by_owner.get(owner)
    }

    #[must_use]
    pub fn impl_attachment(
        &self,
        owner: &DeclarationOwner,
    ) -> Option<&TypedImplAttachment> {
        self.impl_attachments_by_owner.get(owner)
    }

    #[must_use]
    pub fn impl_owners_for_target(
        &self,
        item_id: ItemId,
    ) -> &[DeclarationOwner] {
        self.impl_owners_by_target_item_id
            .get(&item_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = (ItemId, &TypedItemData)> {
        self.by_item_id
            .iter()
            .map(|(item_id, item)| (*item_id, item))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_item_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_item_id.is_empty()
    }
}

#[must_use]
pub fn build_typed_item_table(
    global_items: &GlobalItemTable,
    signatures: &TypedSignatureTable,
) -> TypedItemTable {
    let mut by_item_id = BTreeMap::new();
    let mut by_kind: BTreeMap<TypedItemKind, Vec<ItemId>> = BTreeMap::new();
    let mut by_scope_file_id: BTreeMap<FileId, Vec<ItemId>> = BTreeMap::new();
    let mut issues = Vec::new();

    for global_item in global_items.iter() {
        let Some(typed_kind) = TypedItemKind::from_item_kind(global_item.kind)
        else {
            continue;
        };

        match signature_data_for_item(signatures, global_item.id, typed_kind) {
            Some(data) => {
                by_kind.entry(typed_kind).or_default().push(global_item.id);
                by_scope_file_id
                    .entry(global_item.containing_scope_file_id)
                    .or_default()
                    .push(global_item.id);
                by_item_id.insert(global_item.id, data);
            }
            None => {
                issues.push(TypedItemTableIssue {
                    associated_item_id: Some(global_item.id),
                    kind:
                        TypedItemTableIssueKind::MissingSignatureForGlobalItem {
                            item_kind: typed_kind,
                        },
                });
            }
        }
    }

    collect_signature_consistency_issues(
        global_items,
        &signatures.functions,
        TypedItemKind::Function,
        &mut issues,
    );
    collect_signature_consistency_issues(
        global_items,
        &signatures.structs,
        TypedItemKind::Struct,
        &mut issues,
    );
    collect_signature_consistency_issues(
        global_items,
        &signatures.enums,
        TypedItemKind::Enum,
        &mut issues,
    );
    collect_signature_consistency_issues(
        global_items,
        &signatures.protocols,
        TypedItemKind::Protocol,
        &mut issues,
    );

    let mut impl_signatures_by_owner = BTreeMap::new();
    let mut impl_attachments_by_owner = BTreeMap::new();
    let mut impl_owners_by_target_item_id: BTreeMap<
        ItemId,
        Vec<DeclarationOwner>,
    > = BTreeMap::new();

    for impl_signatures in signatures.impls_by_scope_file_id.values() {
        for impl_signature in impl_signatures {
            let owner = impl_signature.owner.clone();
            let target_item_id = named_item_id(&impl_signature.target);
            let conformance_item_id =
                impl_signature.conformance.as_ref().and_then(named_item_id);

            if impl_signatures_by_owner
                .insert(owner.clone(), impl_signature.clone())
                .is_some()
            {
                issues.push(TypedItemTableIssue {
                    associated_item_id: target_item_id,
                    kind: TypedItemTableIssueKind::DuplicateImplOwner {
                        owner: owner.clone(),
                    },
                });
                continue;
            }

            impl_attachments_by_owner.insert(
                owner.clone(),
                TypedImplAttachment {
                    owner: owner.clone(),
                    containing_scope_file_id: impl_signature
                        .containing_scope_file_id,
                    target_item_id,
                    conformance_item_id,
                },
            );

            if let Some(target_item_id) = target_item_id {
                impl_owners_by_target_item_id
                    .entry(target_item_id)
                    .or_default()
                    .push(owner);
            }
        }
    }

    TypedItemTable {
        by_item_id,
        by_kind,
        by_scope_file_id,
        impl_signatures_by_owner,
        impl_attachments_by_owner,
        impl_owners_by_target_item_id,
        issues,
    }
}

fn signature_data_for_item(
    signatures: &TypedSignatureTable,
    item_id: ItemId,
    kind: TypedItemKind,
) -> Option<TypedItemData> {
    match kind {
        TypedItemKind::Function => signatures
            .functions
            .get(&item_id)
            .cloned()
            .map(TypedItemData::Function),
        TypedItemKind::Struct => signatures
            .structs
            .get(&item_id)
            .cloned()
            .map(TypedItemData::Struct),
        TypedItemKind::Enum => signatures
            .enums
            .get(&item_id)
            .cloned()
            .map(TypedItemData::Enum),
        TypedItemKind::Protocol => signatures
            .protocols
            .get(&item_id)
            .cloned()
            .map(TypedItemData::Protocol),
    }
}

fn collect_signature_consistency_issues<T>(
    global_items: &GlobalItemTable,
    signatures: &BTreeMap<ItemId, T>,
    signature_kind: TypedItemKind,
    issues: &mut Vec<TypedItemTableIssue>,
) {
    for item_id in signatures.keys() {
        let Some(global_item) = global_items.get(*item_id) else {
            issues.push(TypedItemTableIssue {
                associated_item_id: Some(*item_id),
                kind: TypedItemTableIssueKind::SignatureWithoutGlobalItem {
                    signature_kind,
                },
            });
            continue;
        };

        let Some(global_kind) = TypedItemKind::from_item_kind(global_item.kind)
        else {
            issues.push(TypedItemTableIssue {
                associated_item_id: Some(*item_id),
                kind: TypedItemTableIssueKind::SignatureKindMismatch {
                    global_kind: global_item.kind,
                    signature_kind,
                },
            });
            continue;
        };

        if global_kind != signature_kind {
            issues.push(TypedItemTableIssue {
                associated_item_id: Some(*item_id),
                kind: TypedItemTableIssueKind::SignatureKindMismatch {
                    global_kind: global_item.kind,
                    signature_kind,
                },
            });
        }
    }
}

fn named_item_id(ty: &Type) -> Option<ItemId> {
    match ty {
        Type::Named { item_id, .. } => Some(*item_id),
        _ => None,
    }
}
