use super::{BuiltinType, Mutability, NamedTypeKind, Type};
use crate::frontend::resolver::{
    DeclarationOwner, GlobalItemTable, ItemId, ItemKind, ResolvedDeclaration,
    ResolvedDeclarationTable, ResolvedEnumDeclaration,
    ResolvedFunctionSignature, ResolvedImplDeclaration,
    ResolvedProtocolDeclaration, ResolvedStructDeclaration, ResolvedTypeRef,
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
    declarations: &ResolvedDeclarationTable,
    item_table: &GlobalItemTable,
) -> TypedSignatureTable {
    SignatureTyper {
        item_table,
        issues: Vec::new(),
    }
    .type_declarations(declarations)
}

struct SignatureTyper<'a> {
    item_table: &'a GlobalItemTable,
    issues: Vec<SignatureTypingIssue>,
}

#[derive(Clone)]
struct TypeContext {
    owner: DeclarationOwner,
    containing_scope_file_id: Option<FileId>,
}

impl<'a> SignatureTyper<'a> {
    fn type_declarations(
        mut self,
        declarations: &ResolvedDeclarationTable,
    ) -> TypedSignatureTable {
        let mut functions = BTreeMap::new();
        let mut structs = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut protocols = BTreeMap::new();

        for (item_id, declaration) in &declarations.by_item_id {
            let containing_scope_file_id = self
                .item_table
                .get(*item_id)
                .map(|item| item.containing_scope_file_id);
            if containing_scope_file_id.is_none() {
                self.issues.push(SignatureTypingIssue {
                    owner: DeclarationOwner::Item(*item_id),
                    containing_scope_file_id: None,
                    kind: SignatureTypingIssueKind::MissingGlobalItemMetadata {
                        item_id: *item_id,
                    },
                });
            }

            let context = TypeContext {
                owner: DeclarationOwner::Item(*item_id),
                containing_scope_file_id,
            };

            match declaration {
                ResolvedDeclaration::Function(signature) => {
                    functions.insert(
                        *item_id,
                        self.type_function_signature(&context, signature),
                    );
                }
                ResolvedDeclaration::Struct(struct_decl) => {
                    structs.insert(
                        *item_id,
                        self.type_struct_signature_data(&context, struct_decl),
                    );
                }
                ResolvedDeclaration::Enum(enum_decl) => {
                    enums.insert(
                        *item_id,
                        self.type_enum_signature_data(&context, enum_decl),
                    );
                }
                ResolvedDeclaration::Protocol(protocol_decl) => {
                    protocols.insert(
                        *item_id,
                        self.type_protocol_signature_data(
                            &context,
                            protocol_decl,
                        ),
                    );
                }
            }
        }

        let mut impls_by_scope_file_id = BTreeMap::new();
        for (scope_file_id, impls) in &declarations.impls_by_scope_file_id {
            let typed = impls
                .iter()
                .map(|resolved_impl| {
                    self.type_impl_signature(
                        &TypeContext {
                            owner: resolved_impl.owner.clone(),
                            containing_scope_file_id: Some(
                                resolved_impl.containing_scope_file_id,
                            ),
                        },
                        resolved_impl,
                    )
                })
                .collect();
            impls_by_scope_file_id.insert(*scope_file_id, typed);
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
        declaration: &ResolvedStructDeclaration,
    ) -> TypedStructSignatureData {
        TypedStructSignatureData {
            fields: declaration
                .fields
                .iter()
                .map(|field| TypedStructField {
                    name: field.name.clone(),
                    ty: self.type_ref(context, &field.ty),
                })
                .collect(),
            method_signatures: declaration
                .methods
                .iter()
                .map(|method| TypedNamedFunctionSignature {
                    name: method.name.clone(),
                    signature: self
                        .type_function_signature(context, &method.signature),
                })
                .collect(),
            initializer_signatures: declaration
                .initializers
                .iter()
                .map(|init| self.type_function_signature(context, init))
                .collect(),
        }
    }

    fn type_enum_signature_data(
        &mut self,
        context: &TypeContext,
        declaration: &ResolvedEnumDeclaration,
    ) -> TypedEnumSignatureData {
        TypedEnumSignatureData {
            case_signatures: declaration
                .cases
                .iter()
                .map(|case_| TypedEnumCaseSignature {
                    name: case_.name.clone(),
                    payload_types: case_
                        .payload
                        .iter()
                        .map(|payload| self.type_ref(context, &payload.ty))
                        .collect(),
                })
                .collect(),
            method_signatures: declaration
                .methods
                .iter()
                .map(|method| TypedNamedFunctionSignature {
                    name: method.name.clone(),
                    signature: self
                        .type_function_signature(context, &method.signature),
                })
                .collect(),
            initializer_signatures: declaration
                .initializers
                .iter()
                .map(|init| self.type_function_signature(context, init))
                .collect(),
        }
    }

    fn type_protocol_signature_data(
        &mut self,
        context: &TypeContext,
        declaration: &ResolvedProtocolDeclaration,
    ) -> TypedProtocolSignatureData {
        TypedProtocolSignatureData {
            inheritance_types: declaration
                .inheritance
                .iter()
                .map(|inherit| self.type_ref(context, inherit))
                .collect(),
            properties: declaration
                .properties
                .iter()
                .map(|property| TypedProtocolProperty {
                    name: property.name.clone(),
                    ty: self.type_ref(context, &property.ty),
                })
                .collect(),
            method_signatures: declaration
                .methods
                .iter()
                .map(|method| TypedNamedFunctionSignature {
                    name: method.name.clone(),
                    signature: self
                        .type_function_signature(context, &method.signature),
                })
                .collect(),
            initializer_signatures: declaration
                .initializers
                .iter()
                .map(|init| self.type_function_signature(context, init))
                .collect(),
            associated_type_bounds: declaration
                .associated_types
                .iter()
                .map(|associated_type| TypedAssociatedTypeBounds {
                    name: associated_type.name.clone(),
                    bounds: associated_type
                        .bounds
                        .iter()
                        .map(|bound| self.type_ref(context, bound))
                        .collect(),
                })
                .collect(),
        }
    }

    fn type_impl_signature(
        &mut self,
        context: &TypeContext,
        declaration: &ResolvedImplDeclaration,
    ) -> TypedImplSignature {
        TypedImplSignature {
            owner: declaration.owner.clone(),
            containing_scope_file_id: declaration.containing_scope_file_id,
            target: self.type_ref(context, &declaration.target),
            conformance: declaration
                .conformance
                .as_ref()
                .map(|ty| self.type_ref(context, ty)),
            method_signatures: declaration
                .methods
                .iter()
                .map(|method| TypedNamedFunctionSignature {
                    name: method.name.clone(),
                    signature: self
                        .type_function_signature(context, &method.signature),
                })
                .collect(),
            initializer_signatures: declaration
                .initializers
                .iter()
                .map(|init| self.type_function_signature(context, init))
                .collect(),
        }
    }

    fn type_function_signature(
        &mut self,
        context: &TypeContext,
        signature: &ResolvedFunctionSignature,
    ) -> TypedFunctionSignature {
        TypedFunctionSignature {
            param_types: signature
                .params
                .iter()
                .map(|param| self.type_ref(context, &param.ty))
                .collect(),
            return_type: signature
                .return_type
                .as_ref()
                .map(|ty| self.type_ref(context, ty)),
        }
    }

    fn type_ref(
        &mut self,
        context: &TypeContext,
        ty: &ResolvedTypeRef,
    ) -> Type {
        match ty {
            ResolvedTypeRef::Named { segments, resolved } => {
                if let Some(builtin) = Self::builtin_from_segments(segments) {
                    return Type::builtin(builtin);
                }

                match resolved {
                    Some(resolved_item) => self.type_for_item_path(
                        context,
                        segments,
                        resolved_item.item_id,
                    ),
                    None => {
                        self.issues.push(SignatureTypingIssue {
                            owner: context.owner.clone(),
                            containing_scope_file_id: context
                                .containing_scope_file_id,
                            kind: SignatureTypingIssueKind::UnresolvedPath {
                                path: segments.clone(),
                            },
                        });
                        Type::error()
                    }
                }
            }
            ResolvedTypeRef::Reference(inner) => {
                Type::pointer(self.type_ref(context, inner), Mutability::Const)
            }
            ResolvedTypeRef::MutableReference(inner)
            | ResolvedTypeRef::MutablePointer(inner) => {
                Type::pointer(self.type_ref(context, inner), Mutability::Mut)
            }
            ResolvedTypeRef::ConstPointer(inner) => {
                Type::pointer(self.type_ref(context, inner), Mutability::Const)
            }
            ResolvedTypeRef::Grouped(inner) => self.type_ref(context, inner),
            ResolvedTypeRef::GenericApplication { base, args } => {
                self.type_ref(context, base);
                for arg in args {
                    self.type_ref(context, arg);
                }
                self.push_unsupported_issue(context, "generic application");
                Type::error()
            }
            ResolvedTypeRef::SelfType => {
                self.push_unsupported_issue(context, "self type");
                Type::error()
            }
            ResolvedTypeRef::Array(inner) => {
                self.type_ref(context, inner);
                self.push_unsupported_issue(context, "array type");
                Type::error()
            }
            ResolvedTypeRef::Optional(inner) => {
                self.type_ref(context, inner);
                self.push_unsupported_issue(context, "optional type");
                Type::error()
            }
            ResolvedTypeRef::Result { ok, err } => {
                self.type_ref(context, ok);
                self.type_ref(context, err);
                self.push_unsupported_issue(context, "result type");
                Type::error()
            }
        }
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
