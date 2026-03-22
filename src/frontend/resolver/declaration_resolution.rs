use super::import_resolver::{ImportBindingKind, ResolvedImports};
use super::item_ids::ItemId;
use super::item_table::GlobalItemTable;
use super::model::ScopeGraph;
use crate::frontend::ast::{
    EnumCaseParam, EnumMember, FunctionDecl, ImplDecl, ImplMember, Item,
    ParamDecl, ProtocolMember, Spanned, StructMember, Type,
};
use crate::frontend::source::FileId;
use crate::frontend::DesugaredFile;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedItemRef {
    pub item_id: ItemId,
    pub full_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTypeRef {
    Named {
        segments: Vec<String>,
        resolved: Option<ResolvedItemRef>,
    },
    Lifetime(String),
    GenericApplication {
        base: Box<ResolvedTypeRef>,
        args: Vec<ResolvedTypeRef>,
    },
    SelfType,
    Reference {
        lifetime: Option<String>,
        inner: Box<ResolvedTypeRef>,
    },
    MutableReference {
        lifetime: Option<String>,
        inner: Box<ResolvedTypeRef>,
    },
    ConstPointer(Box<ResolvedTypeRef>),
    MutablePointer(Box<ResolvedTypeRef>),
    Array(Box<ResolvedTypeRef>),
    Optional(Box<ResolvedTypeRef>),
    Result {
        ok: Box<ResolvedTypeRef>,
        err: Box<ResolvedTypeRef>,
    },
    Tuple(Vec<ResolvedTypeRef>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedParamType {
    pub name: String,
    pub ty: ResolvedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFunctionSignature {
    pub params: Vec<ResolvedParamType>,
    pub return_type: Option<ResolvedTypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNamedFunctionSignature {
    pub name: String,
    pub signature: ResolvedFunctionSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStructFieldType {
    pub name: String,
    pub ty: ResolvedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnumPayloadType {
    pub name: Option<String>,
    pub ty: ResolvedTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnumCaseType {
    pub name: String,
    pub payload: Vec<ResolvedEnumPayloadType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAssociatedTypeBounds {
    pub name: String,
    pub bounds: Vec<ResolvedTypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStructDeclaration {
    pub fields: Vec<ResolvedStructFieldType>,
    pub methods: Vec<ResolvedNamedFunctionSignature>,
    pub initializers: Vec<ResolvedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnumDeclaration {
    pub cases: Vec<ResolvedEnumCaseType>,
    pub methods: Vec<ResolvedNamedFunctionSignature>,
    pub initializers: Vec<ResolvedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProtocolDeclaration {
    pub inheritance: Vec<ResolvedTypeRef>,
    pub properties: Vec<ResolvedStructFieldType>,
    pub methods: Vec<ResolvedNamedFunctionSignature>,
    pub initializers: Vec<ResolvedFunctionSignature>,
    pub associated_types: Vec<ResolvedAssociatedTypeBounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedDeclaration {
    Function(ResolvedFunctionSignature),
    Struct(ResolvedStructDeclaration),
    Enum(ResolvedEnumDeclaration),
    Protocol(ResolvedProtocolDeclaration),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeclarationOwner {
    Item(ItemId),
    Impl {
        scope_file_id: FileId,
        impl_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedDeclarationPath {
    pub owner: DeclarationOwner,
    pub containing_scope_file_id: FileId,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedLifetime {
    pub owner: Option<DeclarationOwner>,
    pub containing_scope_file_id: Option<FileId>,
    pub name: String,
    pub span: crate::frontend::ast::Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImplDeclaration {
    pub owner: DeclarationOwner,
    pub containing_scope_file_id: FileId,
    pub target: ResolvedTypeRef,
    pub conformance: Option<ResolvedTypeRef>,
    pub methods: Vec<ResolvedNamedFunctionSignature>,
    pub initializers: Vec<ResolvedFunctionSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDeclarationTable {
    pub by_item_id: BTreeMap<ItemId, ResolvedDeclaration>,
    pub impls_by_scope_file_id: BTreeMap<FileId, Vec<ResolvedImplDeclaration>>,
    pub unresolved_paths: Vec<UnresolvedDeclarationPath>,
    pub unresolved_lifetimes: Vec<UnresolvedLifetime>,
}

impl ResolvedDeclarationTable {
    #[must_use]
    pub fn get(&self, item_id: ItemId) -> Option<&ResolvedDeclaration> {
        self.by_item_id.get(&item_id)
    }

    #[must_use]
    pub fn impls_in_scope(
        &self,
        scope_file_id: FileId,
    ) -> &[ResolvedImplDeclaration] {
        self.impls_by_scope_file_id
            .get(&scope_file_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_item_id.is_empty() && self.impls_by_scope_file_id.is_empty()
    }
}

#[must_use]
pub fn resolve_declaration_types(
    graph: &ScopeGraph,
    parsed_files: &[DesugaredFile],
    imports: &BTreeMap<FileId, ResolvedImports>,
    item_table: &GlobalItemTable,
) -> ResolvedDeclarationTable {
    DeclarationResolver {
        graph,
        parsed_by_id: parsed_files
            .iter()
            .map(|parsed| (parsed.file_id, parsed))
            .collect(),
        imports,
        item_table,
        unresolved_paths: Vec::new(),
        unresolved_lifetimes: Vec::new(),
        in_scope_lifetimes: Vec::new(),
    }
    .resolve()
}

struct DeclarationResolver<'a> {
    graph: &'a ScopeGraph,
    parsed_by_id: BTreeMap<FileId, &'a DesugaredFile>,
    imports: &'a BTreeMap<FileId, ResolvedImports>,
    item_table: &'a GlobalItemTable,
    unresolved_paths: Vec<UnresolvedDeclarationPath>,
    unresolved_lifetimes: Vec<UnresolvedLifetime>,
    in_scope_lifetimes: Vec<String>,
}

impl<'a> DeclarationResolver<'a> {
    fn is_lifetime_in_scope(&self, name: &str) -> bool {
        self.in_scope_lifetimes.iter().any(|l| l == name)
    }

    fn push_lifetime(&mut self, name: String) {
        self.in_scope_lifetimes.push(name);
    }

    fn push_lifetimes(&mut self, lifetimes: &[String]) {
        for l in lifetimes {
            self.push_lifetime(l.clone());
        }
    }

    fn pop_lifetimes(&mut self, count: usize) {
        for _ in 0..count {
            self.in_scope_lifetimes.pop();
        }
    }

    fn resolve(mut self) -> ResolvedDeclarationTable {
        let mut by_item_id = BTreeMap::new();
        let mut impls_by_scope_file_id: BTreeMap<
            FileId,
            Vec<ResolvedImplDeclaration>,
        > = BTreeMap::new();

        for (scope_file_id, scope) in &self.graph.scopes {
            let Some(parsed) = self.parsed_by_id.get(scope_file_id) else {
                continue;
            };

            let mut impl_index = 0usize;
            for item in &parsed.ast.items {
                match &item.node {
                    Item::Function(function_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &function_decl.node.name,
                        ) else {
                            continue;
                        };
                        if by_item_id.contains_key(&item_id) {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let signature = self.resolve_function_signature(
                            &owner,
                            *scope_file_id,
                            &function_decl.node,
                        );
                        by_item_id.insert(
                            item_id,
                            ResolvedDeclaration::Function(signature),
                        );
                    }
                    Item::Struct(struct_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &struct_decl.node.name,
                        ) else {
                            continue;
                        };
                        if by_item_id.contains_key(&item_id) {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let lifetime_count = self.push_generic_lifetimes(
                            &struct_decl.node.generic_params,
                        );
                        let declaration = self.resolve_struct_declaration(
                            &owner,
                            *scope_file_id,
                            &struct_decl.node.members,
                        );
                        self.pop_lifetimes(lifetime_count);
                        by_item_id.insert(
                            item_id,
                            ResolvedDeclaration::Struct(declaration),
                        );
                    }
                    Item::Enum(enum_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &enum_decl.node.name,
                        ) else {
                            continue;
                        };
                        if by_item_id.contains_key(&item_id) {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let lifetime_count = self.push_generic_lifetimes(
                            &enum_decl.node.generic_params,
                        );
                        let declaration = self.resolve_enum_declaration(
                            &owner,
                            *scope_file_id,
                            &enum_decl.node.members,
                        );
                        self.pop_lifetimes(lifetime_count);
                        by_item_id.insert(
                            item_id,
                            ResolvedDeclaration::Enum(declaration),
                        );
                    }
                    Item::Protocol(protocol_decl) => {
                        let Some(item_id) = self.item_id_for_top_level(
                            scope_file_id,
                            &scope.scope_path,
                            &protocol_decl.node.name,
                        ) else {
                            continue;
                        };
                        if by_item_id.contains_key(&item_id) {
                            continue;
                        }
                        let owner = DeclarationOwner::Item(item_id);
                        let lifetime_count = self.push_generic_lifetimes(
                            &protocol_decl.node.generic_params,
                        );
                        let declaration = self.resolve_protocol_declaration(
                            &owner,
                            *scope_file_id,
                            &protocol_decl.node,
                        );
                        self.pop_lifetimes(lifetime_count);
                        by_item_id.insert(
                            item_id,
                            ResolvedDeclaration::Protocol(declaration),
                        );
                    }
                    Item::Impl(impl_decl) => {
                        let owner = DeclarationOwner::Impl {
                            scope_file_id: *scope_file_id,
                            impl_index,
                        };
                        impl_index = impl_index.saturating_add(1);
                        let lifetime_count = self.push_generic_lifetimes(
                            &impl_decl.node.lifetime_params,
                        );
                        let resolved_impl = self.resolve_impl_declaration(
                            &owner,
                            *scope_file_id,
                            &impl_decl.node,
                        );
                        self.pop_lifetimes(lifetime_count);
                        impls_by_scope_file_id
                            .entry(*scope_file_id)
                            .or_default()
                            .push(resolved_impl);
                    }
                    _ => {}
                }
            }
        }

        ResolvedDeclarationTable {
            by_item_id,
            impls_by_scope_file_id,
            unresolved_paths: self.unresolved_paths,
            unresolved_lifetimes: self.unresolved_lifetimes,
        }
    }

    fn item_id_for_top_level(
        &self,
        scope_file_id: &FileId,
        scope_path: &[String],
        name: &str,
    ) -> Option<ItemId> {
        let mut full_path = scope_path.to_vec();
        full_path.push(name.to_string());

        let item_id = self.item_table.item_id_by_full_path(&full_path)?;
        let item = self.item_table.get(item_id)?;
        if item.containing_scope_file_id == *scope_file_id {
            Some(item_id)
        } else {
            None
        }
    }

    fn resolve_struct_declaration(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        members: &[crate::frontend::ast::Spanned<StructMember>],
    ) -> ResolvedStructDeclaration {
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut initializers = Vec::new();

        for member in members {
            match &member.node {
                StructMember::Field(field) => {
                    fields.push(ResolvedStructFieldType {
                        name: field.node.name.clone(),
                        ty: self.resolve_type_ref(
                            owner,
                            scope_file_id,
                            &field.node.ty.node,
                        ),
                    });
                }
                StructMember::Function(function_decl) => {
                    if function_decl.node.init_origin.is_some() {
                        initializers.push(self.resolve_init_signature(
                            owner,
                            scope_file_id,
                            &function_decl.node.params,
                        ));
                    } else {
                        methods.push(ResolvedNamedFunctionSignature {
                            name: function_decl.node.name.clone(),
                            signature: self.resolve_function_signature(
                                owner,
                                scope_file_id,
                                &function_decl.node,
                            ),
                        });
                    }
                }
                StructMember::Init(init_decl) => {
                    initializers.push(self.resolve_init_signature(
                        owner,
                        scope_file_id,
                        &init_decl.node.params,
                    ));
                }
            }
        }

        ResolvedStructDeclaration {
            fields,
            methods,
            initializers,
        }
    }

    fn resolve_enum_declaration(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        members: &[crate::frontend::ast::Spanned<EnumMember>],
    ) -> ResolvedEnumDeclaration {
        let mut cases = Vec::new();
        let mut methods = Vec::new();
        let mut initializers = Vec::new();

        for member in members {
            match &member.node {
                EnumMember::Case(case_decl) => {
                    let payload = case_decl
                        .node
                        .payload
                        .iter()
                        .map(|param| match &param.node {
                            EnumCaseParam::Unnamed(ty) => {
                                ResolvedEnumPayloadType {
                                    name: None,
                                    ty: self.resolve_type_ref(
                                        owner,
                                        scope_file_id,
                                        &ty.node,
                                    ),
                                }
                            }
                            EnumCaseParam::Named { name, ty } => {
                                ResolvedEnumPayloadType {
                                    name: Some(name.clone()),
                                    ty: self.resolve_type_ref(
                                        owner,
                                        scope_file_id,
                                        &ty.node,
                                    ),
                                }
                            }
                        })
                        .collect();
                    cases.push(ResolvedEnumCaseType {
                        name: case_decl.node.name.clone(),
                        payload,
                    });
                }
                EnumMember::Function(function_decl) => {
                    if function_decl.node.init_origin.is_some() {
                        initializers.push(self.resolve_init_signature(
                            owner,
                            scope_file_id,
                            &function_decl.node.params,
                        ));
                    } else {
                        methods.push(ResolvedNamedFunctionSignature {
                            name: function_decl.node.name.clone(),
                            signature: self.resolve_function_signature(
                                owner,
                                scope_file_id,
                                &function_decl.node,
                            ),
                        });
                    }
                }
                EnumMember::Init(init_decl) => {
                    initializers.push(self.resolve_init_signature(
                        owner,
                        scope_file_id,
                        &init_decl.node.params,
                    ));
                }
            }
        }

        ResolvedEnumDeclaration {
            cases,
            methods,
            initializers,
        }
    }

    fn resolve_protocol_declaration(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        protocol_decl: &crate::frontend::ast::ProtocolDecl,
    ) -> ResolvedProtocolDeclaration {
        let inheritance = protocol_decl
            .inheritance
            .iter()
            .map(|inherit| {
                self.resolve_type_ref(owner, scope_file_id, &inherit.node)
            })
            .collect();

        let mut properties = Vec::new();
        let mut methods = Vec::new();
        let mut initializers = Vec::new();
        let mut associated_types = Vec::new();

        for member in &protocol_decl.members {
            match &member.node {
                ProtocolMember::Function(function_member) => {
                    if function_member.node.init_origin.is_some() {
                        initializers.push(self.resolve_init_signature(
                            owner,
                            scope_file_id,
                            &function_member.node.params,
                        ));
                    } else {
                        methods.push(ResolvedNamedFunctionSignature {
                            name: function_member.node.name.clone(),
                            signature: self
                                .resolve_function_signature_from_parts(
                                    owner,
                                    scope_file_id,
                                    &function_member.node.params,
                                    function_member.node.return_type.as_ref(),
                                ),
                        });
                    }
                }
                ProtocolMember::Initializer(init_member) => {
                    initializers.push(self.resolve_init_signature(
                        owner,
                        scope_file_id,
                        &init_member.node.params,
                    ));
                }
                ProtocolMember::Property(property_req) => {
                    properties.push(ResolvedStructFieldType {
                        name: property_req.node.name.clone(),
                        ty: self.resolve_type_ref(
                            owner,
                            scope_file_id,
                            &property_req.node.ty.node,
                        ),
                    });
                }
                ProtocolMember::AssociatedType(associated_type) => {
                    associated_types.push(ResolvedAssociatedTypeBounds {
                        name: associated_type.node.name.clone(),
                        bounds: associated_type
                            .node
                            .bounds
                            .iter()
                            .map(|bound| {
                                self.resolve_type_ref(
                                    owner,
                                    scope_file_id,
                                    &bound.node,
                                )
                            })
                            .collect(),
                    });
                }
            }
        }

        ResolvedProtocolDeclaration {
            inheritance,
            properties,
            methods,
            initializers,
            associated_types,
        }
    }

    fn resolve_impl_declaration(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        impl_decl: &ImplDecl,
    ) -> ResolvedImplDeclaration {
        let target =
            self.resolve_type_ref(owner, scope_file_id, &impl_decl.target.node);
        let conformance = impl_decl.conformance.as_ref().map(|conf| {
            self.resolve_type_ref(owner, scope_file_id, &conf.node)
        });

        let mut methods = Vec::new();
        let mut initializers = Vec::new();
        for member in &impl_decl.members {
            match &member.node {
                ImplMember::Function(function_decl) => {
                    if function_decl.node.init_origin.is_some() {
                        initializers.push(self.resolve_init_signature(
                            owner,
                            scope_file_id,
                            &function_decl.node.params,
                        ));
                    } else {
                        methods.push(ResolvedNamedFunctionSignature {
                            name: function_decl.node.name.clone(),
                            signature: self.resolve_function_signature(
                                owner,
                                scope_file_id,
                                &function_decl.node,
                            ),
                        });
                    }
                }
                ImplMember::Init(init_decl) => {
                    initializers.push(self.resolve_init_signature(
                        owner,
                        scope_file_id,
                        &init_decl.node.params,
                    ));
                }
                ImplMember::AssociatedType(_assoc) => {
                    // Associated type definitions in impl Protocol for Type
                    // These are resolved during protocol conformance checking
                }
            }
        }

        ResolvedImplDeclaration {
            owner: owner.clone(),
            containing_scope_file_id: scope_file_id,
            target,
            conformance,
            methods,
            initializers,
        }
    }

    fn resolve_function_signature(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        function_decl: &FunctionDecl,
    ) -> ResolvedFunctionSignature {
        let lifetime_count =
            self.push_generic_lifetimes(&function_decl.generic_params);
        let result = self.resolve_function_signature_from_parts(
            owner,
            scope_file_id,
            &function_decl.params,
            function_decl.return_type.as_ref(),
        );
        self.pop_lifetimes(lifetime_count);
        result
    }

    fn push_generic_lifetimes(
        &mut self,
        generic_params: &[Spanned<crate::frontend::ast::GenericParam>],
    ) -> usize {
        let mut count = 0;
        let mut seen = std::collections::HashSet::new();
        for param in generic_params {
            if let crate::frontend::ast::GenericParam::Lifetime { name } =
                &param.node
            {
                if !seen.insert(name.clone()) {
                    self.unresolved_lifetimes.push(UnresolvedLifetime {
                        owner: None,
                        containing_scope_file_id: None,
                        name: name.clone(),
                        span: param.span,
                    });
                }
                self.push_lifetime(name.clone());
                count += 1;
            }
        }
        count
    }

    fn resolve_function_signature_from_parts(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        params: &[crate::frontend::ast::Spanned<ParamDecl>],
        return_type: Option<&crate::frontend::ast::Spanned<Type>>,
    ) -> ResolvedFunctionSignature {
        let params = params
            .iter()
            .map(|param| ResolvedParamType {
                name: param.node.name.clone(),
                ty: self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &param.node.ty.node,
                ),
            })
            .collect();
        let return_type = return_type
            .map(|ty| self.resolve_type_ref(owner, scope_file_id, &ty.node));
        ResolvedFunctionSignature {
            params,
            return_type,
        }
    }

    fn resolve_init_signature(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        params: &[crate::frontend::ast::Spanned<ParamDecl>],
    ) -> ResolvedFunctionSignature {
        self.resolve_function_signature_from_parts(
            owner,
            scope_file_id,
            params,
            None,
        )
    }

    fn resolve_type_ref(
        &mut self,
        owner: &DeclarationOwner,
        scope_file_id: FileId,
        ty: &Type,
    ) -> ResolvedTypeRef {
        match ty {
            Type::Named { segments } => {
                let resolved =
                    self.resolve_named_type_path(scope_file_id, segments);
                if resolved.is_none() && !Self::is_builtin_type_path(segments) {
                    self.unresolved_paths.push(UnresolvedDeclarationPath {
                        owner: owner.clone(),
                        containing_scope_file_id: scope_file_id,
                        path: segments.clone(),
                    });
                }
                ResolvedTypeRef::Named {
                    segments: segments.clone(),
                    resolved,
                }
            }
            Type::GenericApplication { base, args } => {
                ResolvedTypeRef::GenericApplication {
                    base: Box::new(self.resolve_type_ref(
                        owner,
                        scope_file_id,
                        &base.node,
                    )),
                    args: args
                        .iter()
                        .map(|arg| {
                            self.resolve_type_ref(
                                owner,
                                scope_file_id,
                                &arg.node,
                            )
                        })
                        .collect(),
                }
            }
            Type::SelfType => ResolvedTypeRef::SelfType,
            Type::Lifetime(lifetime) => {
                if !self.is_lifetime_in_scope(&lifetime.name) {
                    self.unresolved_lifetimes.push(UnresolvedLifetime {
                        owner: Some(owner.clone()),
                        containing_scope_file_id: Some(scope_file_id),
                        name: lifetime.name.clone(),
                        span: lifetime.span,
                    });
                }
                ResolvedTypeRef::Lifetime(lifetime.name.clone())
            }
            Type::Reference { lifetime, inner } => {
                if let Some(l) = lifetime {
                    if !self.is_lifetime_in_scope(&l.name) {
                        self.unresolved_lifetimes.push(UnresolvedLifetime {
                            owner: Some(owner.clone()),
                            containing_scope_file_id: Some(scope_file_id),
                            name: l.name.clone(),
                            span: l.span,
                        });
                    }
                }
                ResolvedTypeRef::Reference {
                    lifetime: lifetime.as_ref().map(|l| l.name.clone()),
                    inner: Box::new(self.resolve_type_ref(
                        owner,
                        scope_file_id,
                        &inner.node,
                    )),
                }
            }
            Type::MutableReference { lifetime, inner } => {
                if let Some(l) = lifetime {
                    if !self.is_lifetime_in_scope(&l.name) {
                        self.unresolved_lifetimes.push(UnresolvedLifetime {
                            owner: Some(owner.clone()),
                            containing_scope_file_id: Some(scope_file_id),
                            name: l.name.clone(),
                            span: l.span,
                        });
                    }
                }
                ResolvedTypeRef::MutableReference {
                    lifetime: lifetime.as_ref().map(|l| l.name.clone()),
                    inner: Box::new(self.resolve_type_ref(
                        owner,
                        scope_file_id,
                        &inner.node,
                    )),
                }
            }
            Type::ConstPointer(inner) => {
                ResolvedTypeRef::ConstPointer(Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &inner.node,
                )))
            }
            Type::MutablePointer(inner) => {
                ResolvedTypeRef::MutablePointer(Box::new(
                    self.resolve_type_ref(owner, scope_file_id, &inner.node),
                ))
            }
            Type::Array(inner) => ResolvedTypeRef::Array(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
            Type::Optional(inner) => ResolvedTypeRef::Optional(Box::new(
                self.resolve_type_ref(owner, scope_file_id, &inner.node),
            )),
            Type::Result { ok, err } => ResolvedTypeRef::Result {
                ok: Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &ok.node,
                )),
                err: Box::new(self.resolve_type_ref(
                    owner,
                    scope_file_id,
                    &err.node,
                )),
            },
            Type::Tuple(elems) => ResolvedTypeRef::Tuple(
                elems
                    .iter()
                    .map(|e| {
                        self.resolve_type_ref(owner, scope_file_id, &e.node)
                    })
                    .collect(),
            ),
        }
    }

    fn resolve_named_type_path(
        &self,
        scope_file_id: FileId,
        segments: &[String],
    ) -> Option<ResolvedItemRef> {
        let first = segments.first()?;

        if let Some(binding) = self
            .imports
            .get(&scope_file_id)
            .and_then(|imports| imports.get(first))
        {
            if let Some(resolved) =
                self.resolve_named_path_from_import(binding, segments)
            {
                return Some(resolved);
            }
        }

        let scope = self.graph.scope(scope_file_id)?;
        let mut local_full_path = scope.scope_path.clone();
        local_full_path.extend(segments.iter().cloned());
        self.item_ref_by_full_path(local_full_path)
    }

    fn resolve_named_path_from_import(
        &self,
        binding: &crate::frontend::resolver::ResolvedImportBinding,
        segments: &[String],
    ) -> Option<ResolvedItemRef> {
        if segments.len() == 1 {
            return self.item_ref_by_full_path(binding.target_path.clone());
        }

        match binding.kind {
            ImportBindingKind::Scope => {
                let mut full_path = binding.target_path.clone();
                full_path.extend(segments.iter().skip(1).cloned());
                self.item_ref_by_full_path(full_path)
            }
            ImportBindingKind::Symbol(_) => None,
        }
    }

    fn item_ref_by_full_path(
        &self,
        full_path: Vec<String>,
    ) -> Option<ResolvedItemRef> {
        let item_id = self.item_table.item_id_by_full_path(&full_path)?;
        Some(ResolvedItemRef { item_id, full_path })
    }

    fn is_builtin_type_path(segments: &[String]) -> bool {
        if segments.len() != 1 {
            return false;
        }

        matches!(
            segments[0].as_str(),
            "i8" | "i16"
                | "i32"
                | "i64"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "isize"
                | "usize"
                | "f32"
                | "f64"
                | "bool"
                | "char"
                | "string"
                | "void"
                | "never"
        )
    }
}
