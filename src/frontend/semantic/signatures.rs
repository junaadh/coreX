use super::hir_input::SemanticHirInput;
use super::{BuiltinType, Mutability, NamedTypeKind, Type};
use crate::frontend::hir::{
    HirFunction, HirFunctionSignature, HirItemKind, HirModule,
    HirProtocolFunction, HirTypeId, HirTypeKind,
};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ItemId, ItemKind,
};
use crate::frontend::source::FileId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureTypingIssueKind {
    UnresolvedPath {
        path: Vec<String>,
    },
    InvalidTypeItem {
        path: Vec<String>,
        item_id: ItemId,
        item_kind: ItemKind,
    },
    MissingResolvedItem {
        path: Vec<String>,
        item_id: ItemId,
    },
    MissingGlobalItemMetadata {
        item_id: ItemId,
    },
    UnsupportedTypeSurface {
        description: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureTypingIssue {
    pub owner: DeclarationOwner,
    pub containing_scope_file_id: Option<FileId>,
    pub kind: SignatureTypingIssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedFunctionSignature {
    pub param_types: Vec<Type>,
    pub return_type: Option<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedNamedFunctionSignature {
    pub name: String,
    pub signature: TypedFunctionSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStructField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStructSignatureData {
    pub fields: Vec<TypedStructField>,
    pub method_signatures: Vec<TypedNamedFunctionSignature>,
    pub initializer_signatures: Vec<TypedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedEnumCaseSignature {
    pub name: String,
    pub payload_types: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedEnumSignatureData {
    pub case_signatures: Vec<TypedEnumCaseSignature>,
    pub method_signatures: Vec<TypedNamedFunctionSignature>,
    pub initializer_signatures: Vec<TypedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedProtocolProperty {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedAssociatedTypeBounds {
    pub name: String,
    pub bounds: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedProtocolSignatureData {
    pub inheritance_types: Vec<Type>,
    pub properties: Vec<TypedProtocolProperty>,
    pub method_signatures: Vec<TypedNamedFunctionSignature>,
    pub initializer_signatures: Vec<TypedFunctionSignature>,
    pub associated_type_bounds: Vec<TypedAssociatedTypeBounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedImplSignature {
    pub owner: DeclarationOwner,
    pub containing_scope_file_id: FileId,
    pub target: Type,
    pub conformance: Option<Type>,
    pub method_signatures: Vec<TypedNamedFunctionSignature>,
    pub initializer_signatures: Vec<TypedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedSignatureTable {
    pub functions: BTreeMap<ItemId, TypedFunctionSignature>,
    pub structs: BTreeMap<ItemId, TypedStructSignatureData>,
    pub enums: BTreeMap<ItemId, TypedEnumSignatureData>,
    pub protocols: BTreeMap<ItemId, TypedProtocolSignatureData>,
    pub impls_by_scope_file_id: BTreeMap<FileId, Vec<TypedImplSignature>>,
    pub issues: Vec<SignatureTypingIssue>,
}

impl TypedSignatureTable {
    #[must_use]
    pub fn function(&self, item_id: ItemId) -> Option<&TypedFunctionSignature> {
        self.functions.get(&item_id)
    }

    #[must_use]
    pub fn struct_data(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedStructSignatureData> {
        self.structs.get(&item_id)
    }

    #[must_use]
    pub fn enum_data(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedEnumSignatureData> {
        self.enums.get(&item_id)
    }

    #[must_use]
    pub fn protocol(
        &self,
        item_id: ItemId,
    ) -> Option<&TypedProtocolSignatureData> {
        self.protocols.get(&item_id)
    }

    #[must_use]
    pub fn impls_in_scope(
        &self,
        scope_file_id: FileId,
    ) -> &[TypedImplSignature] {
        self.impls_by_scope_file_id
            .get(&scope_file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.structs.is_empty()
            && self.enums.is_empty()
            && self.protocols.is_empty()
            && self.impls_by_scope_file_id.is_empty()
    }
}

#[must_use]
pub fn type_declaration_signatures(
    hir_input: &SemanticHirInput,
    item_table: &GlobalItemTable,
) -> TypedSignatureTable {
    SignatureTyper::new(hir_input, item_table).type_declarations()
}

struct SignatureTyper<'a> {
    hir_input: &'a SemanticHirInput,
    item_table: &'a GlobalItemTable,
    item_ref_by_item_id: BTreeMap<ItemId, crate::frontend::HirItemRef>,
    issues: Vec<SignatureTypingIssue>,
}

#[derive(Clone)]
struct TypeContext {
    owner: DeclarationOwner,
    containing_scope_file_id: Option<FileId>,
    file_id: FileId,
}

impl<'a> SignatureTyper<'a> {
    fn new(
        hir_input: &'a SemanticHirInput,
        item_table: &'a GlobalItemTable,
    ) -> Self {
        let mut item_ref_by_item_id = BTreeMap::new();
        for (item_ref, item_id) in &hir_input.item_id_by_hir_item_ref {
            item_ref_by_item_id.entry(*item_id).or_insert(*item_ref);
        }

        Self {
            hir_input,
            item_table,
            item_ref_by_item_id,
            issues: Vec::new(),
        }
    }

    fn type_declarations(mut self) -> TypedSignatureTable {
        let mut functions = BTreeMap::new();
        let mut structs = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut protocols = BTreeMap::new();

        for global_item in self.item_table.iter() {
            let containing_scope_file_id =
                Some(global_item.containing_scope_file_id);
            let context = TypeContext {
                owner: DeclarationOwner::Item(global_item.id),
                containing_scope_file_id,
                file_id: global_item.containing_scope_file_id,
            };

            let Some(item_ref) =
                self.item_ref_by_item_id.get(&global_item.id).copied()
            else {
                self.issues.push(SignatureTypingIssue {
                    owner: context.owner,
                    containing_scope_file_id,
                    kind: SignatureTypingIssueKind::MissingGlobalItemMetadata {
                        item_id: global_item.id,
                    },
                });
                continue;
            };

            let Some(module) =
                self.hir_input.hir_modules.get(&item_ref.file_id)
            else {
                self.issues.push(SignatureTypingIssue {
                    owner: context.owner,
                    containing_scope_file_id,
                    kind: SignatureTypingIssueKind::MissingGlobalItemMetadata {
                        item_id: global_item.id,
                    },
                });
                continue;
            };

            let Some(hir_item) = module.items.get(&item_ref.item_id) else {
                self.issues.push(SignatureTypingIssue {
                    owner: context.owner,
                    containing_scope_file_id,
                    kind: SignatureTypingIssueKind::MissingGlobalItemMetadata {
                        item_id: global_item.id,
                    },
                });
                continue;
            };

            match (global_item.kind, &hir_item.kind) {
                (ItemKind::Function, HirItemKind::Function(function)) => {
                    functions.insert(
                        global_item.id,
                        self.type_function_signature(
                            &TypeContext {
                                file_id: item_ref.file_id,
                                ..context.clone()
                            },
                            module,
                            &function.signature,
                        ),
                    );
                }
                (ItemKind::Struct, HirItemKind::Struct(struct_decl)) => {
                    structs.insert(
                        global_item.id,
                        self.type_struct_signature_data(
                            &TypeContext {
                                file_id: item_ref.file_id,
                                ..context.clone()
                            },
                            module,
                            struct_decl,
                        ),
                    );
                }
                (ItemKind::Enum, HirItemKind::Enum(enum_decl)) => {
                    enums.insert(
                        global_item.id,
                        self.type_enum_signature_data(
                            &TypeContext {
                                file_id: item_ref.file_id,
                                ..context.clone()
                            },
                            module,
                            enum_decl,
                        ),
                    );
                }
                (ItemKind::Protocol, HirItemKind::Protocol(protocol_decl)) => {
                    protocols.insert(
                        global_item.id,
                        self.type_protocol_signature_data(
                            &TypeContext {
                                file_id: item_ref.file_id,
                                ..context.clone()
                            },
                            module,
                            protocol_decl,
                        ),
                    );
                }
                (ItemKind::Scope, _) => {}
                _ => {
                    self.push_unsupported_issue(
                        &context,
                        "global and HIR item kind mismatch",
                    );
                }
            }
        }

        let mut impls_by_scope_file_id: BTreeMap<
            FileId,
            Vec<TypedImplSignature>,
        > = BTreeMap::new();
        for hir_file in &self.hir_input.hir_files {
            let Some(module) =
                self.hir_input.hir_modules.get(&hir_file.file_id)
            else {
                continue;
            };

            let mut impl_index = 0usize;
            for item_id in &hir_file.root_items {
                let Some(item) = module.items.get(item_id) else {
                    continue;
                };
                let HirItemKind::Impl(impl_decl) = &item.kind else {
                    continue;
                };

                let owner = DeclarationOwner::Impl {
                    scope_file_id: hir_file.file_id,
                    impl_index,
                };
                impl_index = impl_index.saturating_add(1);

                let context = TypeContext {
                    owner: owner.clone(),
                    containing_scope_file_id: Some(hir_file.file_id),
                    file_id: hir_file.file_id,
                };

                let typed_impl =
                    self.type_impl_signature(&context, module, impl_decl);
                impls_by_scope_file_id
                    .entry(hir_file.file_id)
                    .or_default()
                    .push(typed_impl);
            }
        }

        TypedSignatureTable {
            functions,
            structs,
            enums,
            protocols,
            impls_by_scope_file_id,
            issues: self.issues,
        }
    }

    fn type_struct_signature_data(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        declaration: &crate::frontend::hir::HirStruct,
    ) -> TypedStructSignatureData {
        let (method_signatures, initializer_signatures) =
            self.type_hir_functions(context, module, &declaration.functions);

        TypedStructSignatureData {
            fields: declaration
                .fields
                .iter()
                .map(|field| TypedStructField {
                    name: field.name.clone(),
                    ty: self.type_ref(context, module, field.ty),
                })
                .collect(),
            method_signatures,
            initializer_signatures,
        }
    }

    fn type_enum_signature_data(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        declaration: &crate::frontend::hir::HirEnum,
    ) -> TypedEnumSignatureData {
        let (method_signatures, initializer_signatures) =
            self.type_hir_functions(context, module, &declaration.functions);

        TypedEnumSignatureData {
            case_signatures: declaration
                .variants
                .iter()
                .map(|case_| TypedEnumCaseSignature {
                    name: case_.name.clone(),
                    payload_types: case_
                        .payload
                        .iter()
                        .map(|payload| self.type_ref(context, module, *payload))
                        .collect(),
                })
                .collect(),
            method_signatures,
            initializer_signatures,
        }
    }

    fn type_protocol_signature_data(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        declaration: &crate::frontend::hir::HirProtocol,
    ) -> TypedProtocolSignatureData {
        let (method_signatures, initializer_signatures) = self
            .type_hir_protocol_functions(
                context,
                module,
                &declaration.functions,
            );

        TypedProtocolSignatureData {
            inheritance_types: declaration
                .inherited_types
                .iter()
                .map(|inherit| self.type_ref(context, module, *inherit))
                .collect(),
            properties: declaration
                .properties
                .iter()
                .map(|property| TypedProtocolProperty {
                    name: property.name.clone(),
                    ty: self.type_ref(context, module, property.ty),
                })
                .collect(),
            method_signatures,
            initializer_signatures,
            associated_type_bounds: declaration
                .associated_types
                .iter()
                .map(|associated_type| TypedAssociatedTypeBounds {
                    name: associated_type.name.clone(),
                    bounds: associated_type
                        .bounds
                        .iter()
                        .map(|bound| self.type_ref(context, module, *bound))
                        .collect(),
                })
                .collect(),
        }
    }

    fn type_impl_signature(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        declaration: &crate::frontend::hir::HirImpl,
    ) -> TypedImplSignature {
        let (method_signatures, initializer_signatures) =
            self.type_hir_functions(context, module, &declaration.functions);

        TypedImplSignature {
            owner: context.owner.clone(),
            containing_scope_file_id: context
                .containing_scope_file_id
                .unwrap_or(context.file_id),
            target: self.type_ref(context, module, declaration.target),
            conformance: declaration
                .conformance
                .map(|ty| self.type_ref(context, module, ty)),
            method_signatures,
            initializer_signatures,
        }
    }

    fn type_hir_functions(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        functions: &[HirFunction],
    ) -> (
        Vec<TypedNamedFunctionSignature>,
        Vec<TypedFunctionSignature>,
    ) {
        let mut methods = Vec::new();
        let mut initializers = Vec::new();

        for function in functions {
            let typed_signature = self.type_function_signature(
                context,
                module,
                &function.signature,
            );
            if function.init_origin.is_some() {
                initializers.push(typed_signature);
            } else {
                methods.push(TypedNamedFunctionSignature {
                    name: function.name.clone(),
                    signature: typed_signature,
                });
            }
        }

        (methods, initializers)
    }

    fn type_hir_protocol_functions(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        functions: &[HirProtocolFunction],
    ) -> (
        Vec<TypedNamedFunctionSignature>,
        Vec<TypedFunctionSignature>,
    ) {
        let mut methods = Vec::new();
        let mut initializers = Vec::new();

        for function in functions {
            let typed_signature = self.type_function_signature(
                context,
                module,
                &function.signature,
            );
            if function.init_origin.is_some() {
                initializers.push(typed_signature);
            } else {
                methods.push(TypedNamedFunctionSignature {
                    name: function.name.clone(),
                    signature: typed_signature,
                });
            }
        }

        (methods, initializers)
    }

    fn type_function_signature(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        signature: &HirFunctionSignature,
    ) -> TypedFunctionSignature {
        TypedFunctionSignature {
            param_types: signature
                .params
                .iter()
                .map(|param| self.type_ref(context, module, param.ty))
                .collect(),
            return_type: signature
                .return_type
                .map(|ty| self.type_ref(context, module, ty)),
        }
    }

    fn type_ref(
        &mut self,
        context: &TypeContext,
        module: &HirModule,
        ty_id: HirTypeId,
    ) -> Type {
        let Some(ty) = module.types.get(&ty_id) else {
            self.push_unsupported_issue(context, "missing HIR type id");
            return Type::error();
        };

        match &ty.kind {
            HirTypeKind::Path(path) => {
                if let Some(builtin) =
                    Self::builtin_from_segments(&path.segments)
                {
                    return Type::builtin(builtin);
                }

                match self.resolve_item_id_from_type_path(
                    context.file_id,
                    &path.segments,
                ) {
                    Some(item_id) => self.type_for_item_path(
                        context,
                        &path.segments,
                        item_id,
                    ),
                    None => {
                        self.issues.push(SignatureTypingIssue {
                            owner: context.owner.clone(),
                            containing_scope_file_id: context
                                .containing_scope_file_id,
                            kind: SignatureTypingIssueKind::UnresolvedPath {
                                path: path.segments.clone(),
                            },
                        });
                        Type::error()
                    }
                }
            }
            HirTypeKind::Reference { mutable, inner }
            | HirTypeKind::Pointer { mutable, inner } => Type::pointer(
                self.type_ref(context, module, *inner),
                if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Const
                },
            ),
            HirTypeKind::GenericApplication { base, args } => {
                self.type_ref(context, module, *base);
                for arg in args {
                    self.type_ref(context, module, *arg);
                }
                self.push_unsupported_issue(context, "generic application");
                Type::error()
            }
            HirTypeKind::SelfType => self.resolve_self_type(context),
            HirTypeKind::Optional { inner } => {
                self.type_ref(context, module, *inner);
                self.push_unsupported_issue(context, "optional type");
                Type::error()
            }
            HirTypeKind::Result { ok, err } => {
                self.type_ref(context, module, *ok);
                self.type_ref(context, module, *err);
                self.push_unsupported_issue(context, "result type");
                Type::error()
            }
        }
    }

    /// Resolve `Self` to the appropriate type based on the declaration owner.
    ///
    /// For struct/enum/protocol declarations, `Self` resolves to the type itself.
    /// For impl blocks, `Self` resolves to the impl's target type.
    /// For free functions, `Self` is invalid and an error is reported.
    fn resolve_self_type(
        &mut self,
        context: &TypeContext,
    ) -> Type {
        match &context.owner {
            crate::frontend::resolver::DeclarationOwner::Item(item_id) => {
                // Look up the item to determine its kind
                let item_kind = self
                    .item_table
                    .get(*item_id)
                    .map(|item| item.kind);

                match item_kind {
                    Some(crate::frontend::resolver::ItemKind::Struct) => {
                        Type::named(*item_id, NamedTypeKind::Struct)
                    }
                    Some(crate::frontend::resolver::ItemKind::Enum) => {
                        Type::named(*item_id, NamedTypeKind::Enum)
                    }
                    Some(crate::frontend::resolver::ItemKind::Protocol) => {
                        Type::named(*item_id, NamedTypeKind::Protocol)
                    }
                    Some(crate::frontend::resolver::ItemKind::Function) => {
                        // Self in a free function is invalid
                        self.issues.push(SignatureTypingIssue {
                            owner: context.owner.clone(),
                            containing_scope_file_id: context
                                .containing_scope_file_id,
                            kind: SignatureTypingIssueKind::UnsupportedTypeSurface {
                                description: "Self type in free function",
                            },
                        });
                        Type::error()
                    }
                    _ => {
                        // Other item kinds don't support Self
                        self.issues.push(SignatureTypingIssue {
                            owner: context.owner.clone(),
                            containing_scope_file_id: context
                                .containing_scope_file_id,
                            kind: SignatureTypingIssueKind::UnsupportedTypeSurface {
                                description: "Self type in unsupported declaration",
                            },
                        });
                        Type::error()
                    }
                }
            }
            crate::frontend::resolver::DeclarationOwner::Impl { .. } => {
                // For impl blocks, we need to resolve the target type
                // This requires looking up the impl declaration to find its target
                // For now, we'll report this as unsupported and return error
                // TODO: Implement impl block target type resolution
                self.issues.push(SignatureTypingIssue {
                    owner: context.owner.clone(),
                    containing_scope_file_id: context.containing_scope_file_id,
                    kind: SignatureTypingIssueKind::UnsupportedTypeSurface {
                        description: "Self type in impl block (not yet implemented)",
                    },
                });
                Type::error()
            }
        }
    }

    fn resolve_item_id_from_type_path(
        &self,
        file_id: FileId,
        segments: &[String],
    ) -> Option<ItemId> {
        let imports = &self.hir_input.hir_imports;
        let first = segments.first()?;

        if let Some(binding) =
            imports.get(file_id).and_then(|table| table.get(first))
        {
            if segments.len() == 1 {
                if binding.kind
                    == crate::frontend::resolver::HirImportBindingKind::Item
                {
                    let item_ref = binding.target_item?;
                    return self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(&item_ref)
                        .copied();
                }
            } else if binding.kind
                == crate::frontend::resolver::HirImportBindingKind::Scope
            {
                let mut full_path = binding.target_path.clone();
                full_path.extend(segments.iter().skip(1).cloned());
                let root_name = binding.source_root.as_deref();
                if let Some(item_ref) = imports
                    .item_paths_for_root(root_name)
                    .and_then(|paths| paths.get(&full_path))
                {
                    return self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(item_ref)
                        .copied();
                }
            }
        }

        if first == "root" && segments.len() > 1 {
            let rooted = segments.iter().skip(1).cloned().collect::<Vec<_>>();
            if let Some(item_ref) = imports
                .item_paths_for_root(None)
                .and_then(|paths| paths.get(&rooted))
            {
                return self
                    .hir_input
                    .item_id_by_hir_item_ref
                    .get(item_ref)
                    .copied();
            }
        }

        if first == "super" && segments.len() > 1 {
            if let Some(scope_path) = imports.scope_path_for_file(file_id) {
                let mut parent_scope = scope_path.to_vec();
                parent_scope.pop();
                parent_scope.extend(segments.iter().skip(1).cloned());
                if let Some(item_ref) = imports
                    .item_paths_for_root(None)
                    .and_then(|paths| paths.get(&parent_scope))
                {
                    return self
                        .hir_input
                        .item_id_by_hir_item_ref
                        .get(item_ref)
                        .copied();
                }
            }
        }

        if segments.len() > 1 {
            let named_root = segments[0].as_str();
            let remainder =
                segments.iter().skip(1).cloned().collect::<Vec<_>>();
            if let Some(item_ref) = imports
                .item_paths_for_root(Some(named_root))
                .and_then(|paths| paths.get(&remainder))
            {
                return self
                    .hir_input
                    .item_id_by_hir_item_ref
                    .get(item_ref)
                    .copied();
            }
        }

        let mut local_full_path =
            imports.scope_path_for_file(file_id)?.to_vec();
        local_full_path.extend(segments.iter().cloned());
        let item_ref = imports
            .item_paths_for_root(None)?
            .get(&local_full_path)
            .copied()?;
        self.hir_input
            .item_id_by_hir_item_ref
            .get(&item_ref)
            .copied()
    }

    fn type_for_item_path(
        &mut self,
        context: &TypeContext,
        segments: &[String],
        item_id: ItemId,
    ) -> Type {
        let Some(item) = self.item_table.get(item_id) else {
            self.issues.push(SignatureTypingIssue {
                owner: context.owner.clone(),
                containing_scope_file_id: context.containing_scope_file_id,
                kind: SignatureTypingIssueKind::MissingResolvedItem {
                    path: segments.to_vec(),
                    item_id,
                },
            });
            return Type::error();
        };

        match item.kind {
            ItemKind::Struct => Type::named(item_id, NamedTypeKind::Struct),
            ItemKind::Enum => Type::named(item_id, NamedTypeKind::Enum),
            ItemKind::Protocol => Type::named(item_id, NamedTypeKind::Protocol),
            ItemKind::Scope | ItemKind::Function => {
                self.issues.push(SignatureTypingIssue {
                    owner: context.owner.clone(),
                    containing_scope_file_id: context.containing_scope_file_id,
                    kind: SignatureTypingIssueKind::InvalidTypeItem {
                        path: segments.to_vec(),
                        item_id,
                        item_kind: item.kind,
                    },
                });
                Type::error()
            }
        }
    }

    fn push_unsupported_issue(
        &mut self,
        context: &TypeContext,
        description: &'static str,
    ) {
        self.issues.push(SignatureTypingIssue {
            owner: context.owner.clone(),
            containing_scope_file_id: context.containing_scope_file_id,
            kind: SignatureTypingIssueKind::UnsupportedTypeSurface {
                description,
            },
        });
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
}
