use super::hir_input::SemanticHirInput;
use super::signatures::TypedFunctionSignature;
use super::{BuiltinType, Mutability, NamedTypeKind, Type, TypedItemData, TypedItemTable};
use crate::frontend::resolver::{
    BodyKind, DeclarationOwner, ItemId, LocalId, LocalKind, LocalMutability,
    ResolvedBody, ResolvedBodyTable, ResolvedTypeRef,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyLocalBindingInfo {
    pub kind: LocalKind,
    pub mutability: LocalMutability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyTypeEnvironment {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub kind: BodyKind,
    pub containing_scope_file_id: FileId,
    pub expected_return_type: Type,
    /// Local type map keyed by HIR local-binding id.
    pub local_types: BTreeMap<LocalId, Type>,
    /// Local binding metadata keyed by HIR local-binding id.
    pub local_bindings: BTreeMap<LocalId, BodyLocalBindingInfo>,
    resolved_local_id_by_hir_local_id: BTreeMap<LocalId, LocalId>,
    hir_local_id_by_resolved_local_id: BTreeMap<LocalId, LocalId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyEnvIssueKind {
    MissingBodySignature,
    MissingParameterType {
        local_id: LocalId,
        parameter_index: usize,
    },
    MissingSelfType {
        local_id: LocalId,
    },
    UnresolvedLocalTypePath {
        local_id: LocalId,
        path: Vec<String>,
    },
    InvalidLocalTypeItem {
        local_id: LocalId,
        item_id: ItemId,
    },
    MissingTypedItemForLocalType {
        local_id: LocalId,
        item_id: ItemId,
    },
    UnsupportedLocalTypeSurface {
        local_id: LocalId,
        description: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyEnvIssue {
    pub owner: DeclarationOwner,
    pub body_index: usize,
    pub containing_scope_file_id: FileId,
    pub kind: BodyEnvIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyTypeEnvironmentTable {
    by_owner: BTreeMap<DeclarationOwner, Vec<BodyTypeEnvironment>>,
    pub issues: Vec<BodyEnvIssue>,
}

impl BodyTypeEnvironmentTable {
    #[must_use]
    pub fn envs_for_owner(
        &self,
        owner: &DeclarationOwner,
    ) -> &[BodyTypeEnvironment] {
        self.by_owner.get(owner).map(Vec::as_slice).unwrap_or(&[])
    }

    #[must_use]
    pub fn env(
        &self,
        owner: &DeclarationOwner,
        body_index: usize,
    ) -> Option<&BodyTypeEnvironment> {
        self.envs_for_owner(owner)
            .iter()
            .find(|env| env.body_index == body_index)
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &BodyTypeEnvironment> {
        self.by_owner.values().flat_map(|envs| envs.iter())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_owner.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_owner.values().all(Vec::is_empty)
    }
}

impl BodyTypeEnvironment {
    #[must_use]
    pub fn local_type_for_hir_local(&self, local_id: LocalId) -> Option<&Type> {
        self.local_types.get(&local_id)
    }

    #[must_use]
    pub fn local_type_for_resolved_local(
        &self,
        local_id: LocalId,
    ) -> Option<&Type> {
        let hir_local_id = self.hir_local_id_for_resolved_local(local_id)?;
        self.local_types.get(&hir_local_id)
    }

    #[must_use]
    pub fn local_binding_for_hir_local(
        &self,
        local_id: LocalId,
    ) -> Option<&BodyLocalBindingInfo> {
        self.local_bindings.get(&local_id)
    }

    #[must_use]
    pub fn local_binding_for_resolved_local(
        &self,
        local_id: LocalId,
    ) -> Option<&BodyLocalBindingInfo> {
        let hir_local_id = self.hir_local_id_for_resolved_local(local_id)?;
        self.local_bindings.get(&hir_local_id)
    }

    #[must_use]
    pub fn resolved_local_id_for_hir_local(
        &self,
        hir_local_id: LocalId,
    ) -> Option<LocalId> {
        self.resolved_local_id_by_hir_local_id
            .get(&hir_local_id)
            .copied()
    }

    #[must_use]
    pub fn hir_local_id_for_resolved_local(
        &self,
        resolved_local_id: LocalId,
    ) -> Option<LocalId> {
        self.hir_local_id_by_resolved_local_id
            .get(&resolved_local_id)
            .copied()
    }

    #[must_use]
    pub fn local_types_by_resolved_id(&self) -> BTreeMap<LocalId, Type> {
        self.local_types
            .iter()
            .filter_map(|(hir_local_id, ty)| {
                self.resolved_local_id_for_hir_local(*hir_local_id)
                    .map(|resolved_local_id| (resolved_local_id, ty.clone()))
            })
            .collect()
    }

    #[must_use]
    pub fn local_bindings_by_resolved_id(
        &self,
    ) -> BTreeMap<LocalId, BodyLocalBindingInfo> {
        self.local_bindings
            .iter()
            .filter_map(|(hir_local_id, info)| {
                self.resolved_local_id_for_hir_local(*hir_local_id)
                    .map(|resolved_local_id| (resolved_local_id, info.clone()))
            })
            .collect()
    }

    #[must_use]
    pub fn resolved_local_types_from_hir_map(
        &self,
        local_types_by_hir_local_id: &BTreeMap<LocalId, Type>,
    ) -> BTreeMap<LocalId, Type> {
        local_types_by_hir_local_id
            .iter()
            .filter_map(|(hir_local_id, ty)| {
                self.resolved_local_id_for_hir_local(*hir_local_id)
                    .map(|resolved_local_id| (resolved_local_id, ty.clone()))
            })
            .collect()
    }
}

#[must_use]
pub fn build_body_type_environments(
    hir_input: &SemanticHirInput,
    resolved_bodies: &ResolvedBodyTable,
    typed_items: &TypedItemTable,
) -> BodyTypeEnvironmentTable {
    let mut grouped_by_owner: BTreeMap<DeclarationOwner, Vec<&ResolvedBody>> =
        BTreeMap::new();
    for body in resolved_bodies.iter() {
        grouped_by_owner
            .entry(body.owner.clone())
            .or_default()
            .push(body);
    }

    let mut by_owner: BTreeMap<DeclarationOwner, Vec<BodyTypeEnvironment>> =
        BTreeMap::new();
    let mut issues = Vec::new();

    for (owner, mut bodies) in grouped_by_owner {
        bodies.sort_by_key(|body| body.body_index);

        let mut owner_envs = Vec::with_capacity(bodies.len());
        for body in bodies {
            owner_envs.push(build_one_body_env(
                hir_input,
                body,
                typed_items,
                &mut issues,
            ));
        }
        by_owner.insert(owner, owner_envs);
    }

    BodyTypeEnvironmentTable { by_owner, issues }
}

fn build_one_body_env(
    hir_input: &SemanticHirInput,
    body: &ResolvedBody,
    typed_items: &TypedItemTable,
    issues: &mut Vec<BodyEnvIssue>,
) -> BodyTypeEnvironment {
    let signature = body_signature(typed_items, body);
    if signature.is_none() {
        issues.push(BodyEnvIssue {
            owner: body.owner.clone(),
            body_index: body.body_index,
            containing_scope_file_id: body.containing_scope_file_id,
            kind: BodyEnvIssueKind::MissingBodySignature,
        });
    }

    let expected_return_type = signature
        .and_then(|sig| sig.return_type.clone())
        .unwrap_or_else(|| {
            if signature.is_some() {
                Type::void()
            } else {
                Type::error()
            }
        });

    let body_ref = hir_input.body_ref(&body.owner, body.body_index);
    let module = body_ref
        .and_then(|body_ref| hir_input.hir_modules.get(&body_ref.file_id));
    let mut resolved_local_id_by_hir_local_id = match (module, body_ref) {
        (Some(module), Some(body_ref)) => {
            hir_input.map_hir_local_ids_to_resolved(module, body, body_ref)
        }
        _ => BTreeMap::new(),
    };
    let mut hir_local_id_by_resolved_local_id =
        resolved_local_id_by_hir_local_id
            .iter()
            .map(|(hir_local_id, resolved_local_id)| {
                (*resolved_local_id, *hir_local_id)
            })
            .collect::<BTreeMap<_, _>>();

    let mut local_types = BTreeMap::new();
    let mut local_bindings = BTreeMap::new();
    let mut parameter_index = 0usize;
    let resolved_locals_by_id = body
        .locals
        .iter()
        .map(|local| (local.id, local))
        .collect::<BTreeMap<_, _>>();

    let mut hir_local_ids = body_ref
        .map(|body_ref| hir_input.local_binding_ids_for_body(body_ref).to_vec())
        .unwrap_or_default();
    hir_local_ids.sort();

    for hir_local_id in hir_local_ids {
        let Some(hir_binding) =
            hir_input.hir_local_bindings.binding(hir_local_id)
        else {
            continue;
        };

        local_bindings.insert(
            hir_local_id,
            BodyLocalBindingInfo {
                kind: hir_binding.kind,
                mutability: hir_binding.mutability,
            },
        );

        let resolved_local = resolved_local_id_by_hir_local_id
            .get(&hir_local_id)
            .and_then(|resolved_local_id| {
                resolved_locals_by_id.get(resolved_local_id)
            })
            .copied();

        let local_type = if hir_binding.kind == LocalKind::Parameter {
            if hir_binding.name == "self" {
                // Get the base Self type
                let self_type = self_type_for_owner(typed_items, &body.owner);

                // Apply receiver mutability based on receiver_kind
                let param_type = match body.receiver_kind {
                    Some(crate::frontend::ast::ReceiverKind::Owned) => {
                        // `self` has type Self
                        self_type
                    }
                    Some(crate::frontend::ast::ReceiverKind::Ref) => {
                        // `&self` has type &Self
                        self_type.map(|ty| Type::pointer(ty, Mutability::Const))
                    }
                    Some(crate::frontend::ast::ReceiverKind::MutRef) => {
                        // `&mut self` has type &mut Self
                        self_type.map(|ty| Type::pointer(ty, Mutability::Mut))
                    }
                    None => {
                        // "self" parameter without receiver_kind - error
                        issues.push(BodyEnvIssue {
                            owner: body.owner.clone(),
                            body_index: body.body_index,
                            containing_scope_file_id: body
                                .containing_scope_file_id,
                            kind: BodyEnvIssueKind::MissingSelfType {
                                local_id: hir_local_id,
                            },
                        });
                        None
                    }
                };

                param_type.unwrap_or_else(|| {
                    issues.push(BodyEnvIssue {
                        owner: body.owner.clone(),
                        body_index: body.body_index,
                        containing_scope_file_id: body
                            .containing_scope_file_id,
                        kind: BodyEnvIssueKind::MissingSelfType {
                            local_id: hir_local_id,
                        },
                    });
                    Type::error()
                })
            } else if let Some(signature) = signature {
                if let Some(ty) = signature.param_types.get(parameter_index) {
                    parameter_index = parameter_index.saturating_add(1);
                    ty.clone()
                } else {
                    issues.push(BodyEnvIssue {
                        owner: body.owner.clone(),
                        body_index: body.body_index,
                        containing_scope_file_id: body.containing_scope_file_id,
                        kind: BodyEnvIssueKind::MissingParameterType {
                            local_id: hir_local_id,
                            parameter_index,
                        },
                    });
                    parameter_index = parameter_index.saturating_add(1);
                    Type::error()
                }
            } else {
                Type::error()
            }
        } else {
            resolved_local
                .and_then(|resolved_local| {
                    resolved_local.declared_type.as_ref()
                })
                .map(|ty| {
                    lower_local_type_ref(
                        &body.owner,
                        body.body_index,
                        body.containing_scope_file_id,
                        hir_local_id,
                        ty,
                        typed_items,
                        issues,
                    )
                })
                .unwrap_or_else(Type::error)
        };

        local_types.insert(hir_local_id, local_type);
    }

    if local_bindings.is_empty() && !body.locals.is_empty() {
        for local in &body.locals {
            resolved_local_id_by_hir_local_id
                .entry(local.id)
                .or_insert(local.id);
            hir_local_id_by_resolved_local_id
                .entry(local.id)
                .or_insert(local.id);
            local_bindings.insert(
                local.id,
                BodyLocalBindingInfo {
                    kind: local.kind,
                    mutability: local.mutability,
                },
            );
            let local_type = if local.kind == LocalKind::Parameter {
                if local.name == "self" {
                    self_type_for_owner(typed_items, &body.owner)
                        .unwrap_or_else(|| {
                            issues.push(BodyEnvIssue {
                                owner: body.owner.clone(),
                                body_index: body.body_index,
                                containing_scope_file_id: body
                                    .containing_scope_file_id,
                                kind: BodyEnvIssueKind::MissingSelfType {
                                    local_id: local.id,
                                },
                            });
                            Type::error()
                        })
                } else if let Some(signature) = signature {
                    if let Some(ty) = signature.param_types.get(parameter_index)
                    {
                        parameter_index = parameter_index.saturating_add(1);
                        ty.clone()
                    } else {
                        issues.push(BodyEnvIssue {
                            owner: body.owner.clone(),
                            body_index: body.body_index,
                            containing_scope_file_id: body
                                .containing_scope_file_id,
                            kind: BodyEnvIssueKind::MissingParameterType {
                                local_id: local.id,
                                parameter_index,
                            },
                        });
                        parameter_index = parameter_index.saturating_add(1);
                        Type::error()
                    }
                } else {
                    Type::error()
                }
            } else {
                local
                    .declared_type
                    .as_ref()
                    .map(|ty| {
                        lower_local_type_ref(
                            &body.owner,
                            body.body_index,
                            body.containing_scope_file_id,
                            local.id,
                            ty,
                            typed_items,
                            issues,
                        )
                    })
                    .unwrap_or_else(Type::error)
            };
            local_types.insert(local.id, local_type);
        }
    }

    BodyTypeEnvironment {
        owner: body.owner.clone(),
        body_index: body.body_index,
        kind: body.kind,
        containing_scope_file_id: body.containing_scope_file_id,
        expected_return_type,
        local_types,
        local_bindings,
        resolved_local_id_by_hir_local_id,
        hir_local_id_by_resolved_local_id,
    }
}

fn body_signature<'a>(
    typed_items: &'a TypedItemTable,
    body: &ResolvedBody,
) -> Option<&'a TypedFunctionSignature> {
    match &body.owner {
        DeclarationOwner::Item(item_id) => match typed_items.get(*item_id)? {
            TypedItemData::Function(signature)
                if matches!(body.kind, BodyKind::Function)
                    && body.signature_index == 0 =>
            {
                Some(signature)
            }
            TypedItemData::Struct(signature_data) => match body.kind {
                BodyKind::Function => signature_data
                    .method_signatures
                    .get(body.signature_index)
                    .map(|method| &method.signature),
                BodyKind::Initializer => signature_data
                    .initializer_signatures
                    .get(body.signature_index),
                BodyKind::ProtocolDefaultFunction
                | BodyKind::ProtocolDefaultInitializer => None,
            },
            TypedItemData::Enum(signature_data) => match body.kind {
                BodyKind::Function => signature_data
                    .method_signatures
                    .get(body.signature_index)
                    .map(|method| &method.signature),
                BodyKind::Initializer => signature_data
                    .initializer_signatures
                    .get(body.signature_index),
                BodyKind::ProtocolDefaultFunction
                | BodyKind::ProtocolDefaultInitializer => None,
            },
            TypedItemData::Protocol(signature_data) => match body.kind {
                BodyKind::ProtocolDefaultFunction => signature_data
                    .method_signatures
                    .get(body.signature_index)
                    .map(|method| &method.signature),
                BodyKind::ProtocolDefaultInitializer => signature_data
                    .initializer_signatures
                    .get(body.signature_index),
                BodyKind::Function | BodyKind::Initializer => None,
            },
            TypedItemData::Function(_) => None,
        },
        DeclarationOwner::Impl { .. } => {
            let impl_signature = typed_items.impl_signature(&body.owner)?;
            match body.kind {
                BodyKind::Function => impl_signature
                    .method_signatures
                    .get(body.signature_index)
                    .map(|method| &method.signature),
                BodyKind::Initializer => impl_signature
                    .initializer_signatures
                    .get(body.signature_index),
                BodyKind::ProtocolDefaultFunction
                | BodyKind::ProtocolDefaultInitializer => None,
            }
        }
    }
}

fn self_type_for_owner(
    typed_items: &TypedItemTable,
    owner: &DeclarationOwner,
) -> Option<Type> {
    match owner {
        DeclarationOwner::Item(item_id) => {
            named_type_from_item_data(*item_id, typed_items.get(*item_id)?)
        }
        DeclarationOwner::Impl { .. } => {
            let attachment = typed_items.impl_attachment(owner)?;
            let target_item_id = attachment.target_item_id?;
            named_type_from_item_data(
                target_item_id,
                typed_items.get(target_item_id)?,
            )
        }
    }
}

fn named_type_from_item_data(
    item_id: ItemId,
    item_data: &TypedItemData,
) -> Option<Type> {
    match item_data {
        TypedItemData::Struct(_) => {
            Some(Type::named(item_id, NamedTypeKind::Struct))
        }
        TypedItemData::Enum(_) => {
            Some(Type::named(item_id, NamedTypeKind::Enum))
        }
        TypedItemData::Protocol(_) => {
            Some(Type::named(item_id, NamedTypeKind::Protocol))
        }
        TypedItemData::Function(_) => None,
    }
}

fn lower_local_type_ref(
    owner: &DeclarationOwner,
    body_index: usize,
    containing_scope_file_id: FileId,
    local_id: LocalId,
    ty: &ResolvedTypeRef,
    typed_items: &TypedItemTable,
    issues: &mut Vec<BodyEnvIssue>,
) -> Type {
    match ty {
        ResolvedTypeRef::Named { segments, resolved } => {
            if let Some(builtin) = builtin_from_segments(segments) {
                return Type::builtin(builtin);
            }
            let Some(resolved_item) = resolved else {
                issues.push(BodyEnvIssue {
                    owner: owner.clone(),
                    body_index,
                    containing_scope_file_id,
                    kind: BodyEnvIssueKind::UnresolvedLocalTypePath {
                        local_id,
                        path: segments.clone(),
                    },
                });
                return Type::error();
            };

            match typed_items.get(resolved_item.item_id) {
                Some(TypedItemData::Struct(_)) => {
                    Type::named(resolved_item.item_id, NamedTypeKind::Struct)
                }
                Some(TypedItemData::Enum(_)) => {
                    Type::named(resolved_item.item_id, NamedTypeKind::Enum)
                }
                Some(TypedItemData::Protocol(_)) => {
                    Type::named(resolved_item.item_id, NamedTypeKind::Protocol)
                }
                Some(TypedItemData::Function(_)) => {
                    issues.push(BodyEnvIssue {
                        owner: owner.clone(),
                        body_index,
                        containing_scope_file_id,
                        kind: BodyEnvIssueKind::InvalidLocalTypeItem {
                            local_id,
                            item_id: resolved_item.item_id,
                        },
                    });
                    Type::error()
                }
                None => {
                    issues.push(BodyEnvIssue {
                        owner: owner.clone(),
                        body_index,
                        containing_scope_file_id,
                        kind: BodyEnvIssueKind::MissingTypedItemForLocalType {
                            local_id,
                            item_id: resolved_item.item_id,
                        },
                    });
                    Type::error()
                }
            }
        }
        ResolvedTypeRef::Reference(inner) => Type::pointer(
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                inner,
                typed_items,
                issues,
            ),
            super::Mutability::Const,
        ),
        ResolvedTypeRef::MutableReference(inner)
        | ResolvedTypeRef::MutablePointer(inner) => Type::pointer(
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                inner,
                typed_items,
                issues,
            ),
            super::Mutability::Mut,
        ),
        ResolvedTypeRef::ConstPointer(inner) => Type::pointer(
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                inner,
                typed_items,
                issues,
            ),
            super::Mutability::Const,
        ),
        ResolvedTypeRef::Grouped(inner) => lower_local_type_ref(
            owner,
            body_index,
            containing_scope_file_id,
            local_id,
            inner,
            typed_items,
            issues,
        ),
        ResolvedTypeRef::SelfType => self_type_for_owner(typed_items, owner)
            .unwrap_or_else(|| {
                issues.push(BodyEnvIssue {
                    owner: owner.clone(),
                    body_index,
                    containing_scope_file_id,
                    kind: BodyEnvIssueKind::MissingSelfType { local_id },
                });
                Type::error()
            }),
        ResolvedTypeRef::GenericApplication { base, args } => {
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                base,
                typed_items,
                issues,
            );
            for arg in args {
                lower_local_type_ref(
                    owner,
                    body_index,
                    containing_scope_file_id,
                    local_id,
                    arg,
                    typed_items,
                    issues,
                );
            }
            issues.push(BodyEnvIssue {
                owner: owner.clone(),
                body_index,
                containing_scope_file_id,
                kind: BodyEnvIssueKind::UnsupportedLocalTypeSurface {
                    local_id,
                    description: "generic application",
                },
            });
            Type::error()
        }
        ResolvedTypeRef::Array(inner) => {
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                inner,
                typed_items,
                issues,
            );
            issues.push(BodyEnvIssue {
                owner: owner.clone(),
                body_index,
                containing_scope_file_id,
                kind: BodyEnvIssueKind::UnsupportedLocalTypeSurface {
                    local_id,
                    description: "array type",
                },
            });
            Type::error()
        }
        ResolvedTypeRef::Optional(inner) => {
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                inner,
                typed_items,
                issues,
            );
            issues.push(BodyEnvIssue {
                owner: owner.clone(),
                body_index,
                containing_scope_file_id,
                kind: BodyEnvIssueKind::UnsupportedLocalTypeSurface {
                    local_id,
                    description: "optional type",
                },
            });
            Type::error()
        }
        ResolvedTypeRef::Result { ok, err } => {
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                ok,
                typed_items,
                issues,
            );
            lower_local_type_ref(
                owner,
                body_index,
                containing_scope_file_id,
                local_id,
                err,
                typed_items,
                issues,
            );
            issues.push(BodyEnvIssue {
                owner: owner.clone(),
                body_index,
                containing_scope_file_id,
                kind: BodyEnvIssueKind::UnsupportedLocalTypeSurface {
                    local_id,
                    description: "result type",
                },
            });
            Type::error()
        }
    }
}

fn builtin_from_segments(segments: &[String]) -> Option<BuiltinType> {
    if segments.len() != 1 {
        return None;
    }

    match segments[0].as_str() {
        "bool" => Some(BuiltinType::Bool),
        "char" => Some(BuiltinType::Char),
        "string" => Some(BuiltinType::String),
        "i8" => Some(BuiltinType::I8),
        "i16" => Some(BuiltinType::I16),
        "i32" => Some(BuiltinType::I32),
        "i64" => Some(BuiltinType::I64),
        "u8" => Some(BuiltinType::U8),
        "u16" => Some(BuiltinType::U16),
        "u32" => Some(BuiltinType::U32),
        "u64" => Some(BuiltinType::U64),
        "isize" => Some(BuiltinType::ISize),
        "usize" => Some(BuiltinType::USize),
        "f32" => Some(BuiltinType::F32),
        "f64" => Some(BuiltinType::F64),
        "void" => Some(BuiltinType::Void),
        "never" => Some(BuiltinType::Never),
        _ => None,
    }
}
