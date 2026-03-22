//! Completion candidate generation for different contexts.

use crate::frontend::resolver::{ImportBindingKind, ItemId, ItemKind};
use crate::frontend::semantic::Type;
use crate::frontend::source::FileId;
use crate::midend::completion::CompletionInput;
use crate::midend::completion::types::{
    CompletionCandidate, CompletionKind, CompletionMetadata,
};
use crate::midend::type_check::{
    TypedEnumCaseSignature, TypedFunctionSignature,
};

/// Complete global scope items (types, functions, locals in scope).
pub fn complete_global(
    input: &CompletionInput,
    file_id: FileId,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    // Add local bindings from function bodies in this file
    // TODO: Add local bindings from body environments

    // Add imported items
    if let Some(file_imports) = input.imports.get(&file_id) {
        for binding in file_imports.bindings.values() {
            let kind = match binding.kind {
                ImportBindingKind::Scope => CompletionKind::Scope,
                ImportBindingKind::Symbol(symbol_kind) => match symbol_kind {
                    crate::frontend::resolver::SymbolKind::Function => {
                        CompletionKind::Function
                    }
                    crate::frontend::resolver::SymbolKind::Struct => {
                        CompletionKind::Struct
                    }
                    crate::frontend::resolver::SymbolKind::Enum => {
                        CompletionKind::Enum
                    }
                    crate::frontend::resolver::SymbolKind::Protocol => {
                        CompletionKind::Protocol
                    }
                    crate::frontend::resolver::SymbolKind::Scope => {
                        CompletionKind::Scope
                    }
                },
            };

            candidates.push(CompletionCandidate {
                label: binding.local_name.clone(),
                kind,
                detail: Some(format!(
                    "import {}",
                    binding.target_path.join("::")
                )),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: None,
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    // Add items in current scope
    for item in input.semantic.global_items.items_in_scope(file_id) {
        let kind = match item.kind {
            ItemKind::Scope => CompletionKind::Scope,
            ItemKind::Function => CompletionKind::Function,
            ItemKind::Struct => CompletionKind::Struct,
            ItemKind::Enum => CompletionKind::Enum,
            ItemKind::Protocol => CompletionKind::Protocol,
        };

        candidates.push(CompletionCandidate {
            label: item.name.clone(),
            kind,
            detail: Some(item.full_path.join("::")),
            documentation: None,
            metadata: CompletionMetadata {
                item_id: Some(item.id),
                deprecated: false,
                ty: None,
            },
        });
    }

    candidates
}

/// Complete path access (e.g., `scope::` or `super::`).
pub fn complete_path_access(
    input: &CompletionInput,
    file_id: FileId,
    scope_item: Option<ItemId>,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    // Match on the scope_item to determine what kind of path access this is
    match scope_item {
        Some(item_id) => {
            // This is a continuation like `scope::item::|`
            // Get the item and find its children or members
            if let Some(global_item) = input.semantic.global_items.get(item_id)
            {
                complete_items_in_scope(
                    input,
                    &global_item.full_path,
                    &mut candidates,
                );
            }
        }
        None => {
            // This is the start of a path like `::|`
            // Complete from the current file's scope
            complete_from_current_scope(input, file_id, &mut candidates);
        }
    }

    candidates
}

/// Complete items available in a specific scope path.
fn complete_items_in_scope(
    input: &CompletionInput,
    scope_path: &[String],
    candidates: &mut Vec<CompletionCandidate>,
) {
    // Find the scope by path
    if let Some(scope_item) =
        input.semantic.global_items.get_by_full_path(scope_path)
    {
        // If this is a scope/module, add items defined in this scope
        if matches!(scope_item.kind, ItemKind::Scope) {
            // Add items defined in this scope
            for item_id in input
                .semantic
                .global_items
                .ids_in_scope(scope_item.defining_file_id)
            {
                if let Some(item) = input.semantic.global_items.get(*item_id) {
                    let kind = match item.kind {
                        ItemKind::Scope => CompletionKind::Scope,
                        ItemKind::Function => CompletionKind::Function,
                        ItemKind::Struct => CompletionKind::Struct,
                        ItemKind::Enum => CompletionKind::Enum,
                        ItemKind::Protocol => CompletionKind::Protocol,
                    };

                    candidates.push(CompletionCandidate {
                        label: item.name.clone(),
                        kind,
                        detail: Some(item.full_path.join("::")),
                        documentation: None,
                        metadata: CompletionMetadata {
                            item_id: Some(item.id),
                            deprecated: false,
                            ty: None,
                        },
                    });
                }
            }
        }
    }
}

/// Complete from the current file's scope context.
fn complete_from_current_scope(
    input: &CompletionInput,
    file_id: FileId,
    candidates: &mut Vec<CompletionCandidate>,
) {
    // Add special path prefixes
    candidates.push(CompletionCandidate {
        label: "super".to_string(),
        kind: CompletionKind::Scope,
        detail: Some("parent scope".to_string()),
        documentation: None,
        metadata: CompletionMetadata {
            item_id: None,
            deprecated: false,
            ty: None,
        },
    });

    candidates.push(CompletionCandidate {
        label: "root".to_string(),
        kind: CompletionKind::Scope,
        detail: Some("project root".to_string()),
        documentation: None,
        metadata: CompletionMetadata {
            item_id: None,
            deprecated: false,
            ty: None,
        },
    });

    // Add imported external libraries
    if let Some(file_imports) = input.imports.get(&file_id) {
        for binding in file_imports.bindings.values() {
            if matches!(binding.kind, ImportBindingKind::Scope) {
                // Check if this is an external library
                if input.external_lookup.is_extern_library(&binding.local_name)
                {
                    candidates.push(CompletionCandidate {
                        label: binding.local_name.clone(),
                        kind: CompletionKind::Scope,
                        detail: Some(format!(
                            "extern library {}",
                            binding.local_name
                        )),
                        documentation: None,
                        metadata: CompletionMetadata {
                            item_id: None,
                            deprecated: false,
                            ty: None,
                        },
                    });
                } else {
                    // This is an imported module/scope
                    candidates.push(CompletionCandidate {
                        label: binding.local_name.clone(),
                        kind: CompletionKind::Scope,
                        detail: Some(format!(
                            "import {}",
                            binding.target_path.join("::")
                        )),
                        documentation: None,
                        metadata: CompletionMetadata {
                            item_id: None,
                            deprecated: false,
                            ty: None,
                        },
                    });
                }
            }
        }
    }

    // Add scopes visible in the current file
    for item in input.semantic.global_items.items_in_scope(file_id) {
        if matches!(item.kind, ItemKind::Scope) {
            candidates.push(CompletionCandidate {
                label: item.name.clone(),
                kind: CompletionKind::Scope,
                detail: Some(item.full_path.join("::")),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(item.id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }
}

/// Complete associated members (e.g., `Type::` for static methods, initializers).
pub fn complete_associated_access(
    input: &CompletionInput,
    base_type: &Type,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    let Type::Named { item_id, .. } = base_type else {
        return candidates;
    };

    // Get signature data for this type
    if let Some(struct_data) = input.signatures.struct_data(*item_id) {
        // Add initializers
        for sig in &struct_data.initializer_signatures {
            candidates.push(CompletionCandidate {
                label: "init".to_string(),
                kind: CompletionKind::Function,
                detail: Some(format_function_signature(sig)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }

        // Add static methods
        for method in &struct_data.method_signatures {
            // TODO: Filter to only static methods
            candidates.push(CompletionCandidate {
                label: method.name.clone(),
                kind: CompletionKind::Function,
                detail: Some(format_function_signature(&method.signature)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    if let Some(enum_data) = input.signatures.enum_data(*item_id) {
        // Add enum variants
        for variant in &enum_data.case_signatures {
            candidates.push(CompletionCandidate {
                label: variant.name.clone(),
                kind: CompletionKind::EnumVariant,
                detail: Some(format_enum_variant_signature(variant)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }

        // Add static methods
        for method in &enum_data.method_signatures {
            candidates.push(CompletionCandidate {
                label: method.name.clone(),
                kind: CompletionKind::Function,
                detail: Some(format_function_signature(&method.signature)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    candidates
}

/// Complete instance members (e.g., `value.` for methods and fields).
pub fn complete_member_access(
    input: &CompletionInput,
    receiver_type: &Type,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    let Type::Named { item_id, .. } = receiver_type else {
        return candidates;
    };

    // Get signature data for this type
    if let Some(struct_data) = input.signatures.struct_data(*item_id) {
        // Add fields
        for field in &struct_data.fields {
            candidates.push(CompletionCandidate {
                label: field.name.clone(),
                kind: CompletionKind::Field,
                detail: Some(format_type(&field.ty)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: Some(field.ty.clone()),
                },
            });
        }

        // Add instance methods (excluding static methods)
        for method in &struct_data.method_signatures {
            // TODO: Filter to only instance methods (not static)
            candidates.push(CompletionCandidate {
                label: method.name.clone(),
                kind: CompletionKind::Function,
                detail: Some(format_function_signature(&method.signature)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    // Handle enum methods and cases
    if let Some(enum_data) = input.signatures.enum_data(*item_id) {
        // Add enum cases/variants (for `a = .CaseName` syntax)
        for variant in &enum_data.case_signatures {
            candidates.push(CompletionCandidate {
                label: variant.name.clone(),
                kind: CompletionKind::EnumVariant,
                detail: Some(format_enum_variant_signature(variant)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }

        // Add instance methods
        for method in &enum_data.method_signatures {
            candidates.push(CompletionCandidate {
                label: method.name.clone(),
                kind: CompletionKind::Function,
                detail: Some(format_function_signature(&method.signature)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    // TODO: Add protocol conformance methods

    candidates
}

/// Complete enum cases/variants.
pub fn complete_enum_cases(
    input: &CompletionInput,
    enum_type: &Type,
) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();

    let Type::Named { item_id, .. } = enum_type else {
        return candidates;
    };

    if let Some(enum_data) = input.signatures.enum_data(*item_id) {
        for variant in &enum_data.case_signatures {
            candidates.push(CompletionCandidate {
                label: variant.name.clone(),
                kind: CompletionKind::EnumVariant,
                detail: Some(format_enum_variant_signature(variant)),
                documentation: None,
                metadata: CompletionMetadata {
                    item_id: Some(*item_id),
                    deprecated: false,
                    ty: None,
                },
            });
        }
    }

    candidates
}

/// Format a type for display in completion.
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Builtin(builtin) => builtin.to_string(),
        Type::Named { item_id, .. } => {
            // TODO: Look up the name from the item table
            format!("NamedType({:?})", item_id)
        }
        Type::Pointer {
            pointee,
            mutability,
        } => {
            format!(
                "{}*{}",
                if matches!(mutability, crate::frontend::Mutability::Mut) {
                    "mut "
                } else {
                    ""
                },
                format_type(pointee)
            )
        }
        Type::Error => "<error>".to_string(),
    }
}

/// Format a function signature for display.
fn format_function_signature(sig: &TypedFunctionSignature) -> String {
    let params: Vec<String> = sig.param_types.iter().map(format_type).collect();

    let ret = sig
        .return_type
        .as_ref()
        .map(format_type)
        .unwrap_or_else(|| "()".to_string());

    format!("({}) -> {}", params.join(", "), ret)
}

/// Format an enum variant signature for display.
fn format_enum_variant_signature(variant: &TypedEnumCaseSignature) -> String {
    if variant.payload_types.is_empty() {
        variant.name.clone()
    } else {
        let payload: Vec<String> =
            variant.payload_types.iter().map(format_type).collect();
        format!("{}({})", variant.name, payload.join(", "))
    }
}
