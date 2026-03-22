//! Semantic completion context determination.

use crate::frontend::semantic::Type;
use crate::frontend::source::FileId;
use crate::midend::completion::CompletionInput;
use crate::midend::completion::hir_lookup::{HirNodeContext, HirNodeKind};
use crate::midend::completion::types::CompletionContext;

/// Determine the completion context from HIR node at cursor position.
///
/// This function maps a file offset to the relevant HIR node and determines
/// what kind of completion is appropriate based on the semantic context.
///
/// # Arguments
/// * `input` - Shared analysis input
/// * `file_id` - The file where completion was triggered
/// * `offset` - The cursor offset in bytes
/// * `node_context` - The HIR node at the cursor position
///
/// # Returns
/// * `Some(CompletionContext)` if the cursor is in a completable location
/// * `None` if completion is not applicable here
pub fn determine_completion_context(
    _input: &CompletionInput,
    _file_id: FileId,
    _offset: usize,
    node_context: &HirNodeContext,
) -> Option<CompletionContext> {
    match node_context.kind {
        HirNodeKind::Path => {
            // Check if this is a path access (e.g., `foo::bar|`)
            // or a type-associated access (e.g., `MyEnum::|`)
            determine_path_context(_input, _file_id, _offset, node_context)
        }
        HirNodeKind::FieldAccess | HirNodeKind::NamespaceAccess => {
            // For `expr.|` or `Type::|`, determine the receiver type
            determine_access_context(_input, node_context)
        }
        HirNodeKind::MethodCall => {
            // After `expr.method|`, we might want to complete method arguments
            // For now, treat as member access
            determine_access_context(_input, node_context)
        }
        HirNodeKind::OtherExpr | HirNodeKind::NonExpr => {
            // Global completion in expression position
            Some(CompletionContext::Global)
        }
    }
}

/// Determine if a path access is a scope path or associated type access.
fn determine_path_context(
    input: &CompletionInput,
    file_id: FileId,
    offset: usize,
    node_context: &HirNodeContext,
) -> Option<CompletionContext> {
    // Check if we're after `.` or `::` in the source text
    // We need to look at the source to determine if the cursor is after these separators

    let hir_file = input.hir_files.get(&file_id)?;
    let source_file = input.source_db.file(hir_file.file_id)?;

    // Get the source text around the cursor
    let source_text = source_file.source();

    // Check if we're immediately after `::` (associated access)
    if offset >= 2 && &source_text[offset - 2..offset] == "::" {
        // This is associated access like `Enum::|`
        // Try to resolve what comes before `::`
        return resolve_associated_access_context(input, file_id, offset - 2);
    }

    // Check if we're immediately after `.` (field access or method call)
    if offset >= 1 && &source_text[offset - 1..offset] == "." {
        // This is field/method access like `value.|`
        // Try to get the type of the expression before `.`
        return resolve_field_access_context(input, file_id, offset - 1);
    }

    // Default to global completion
    Some(CompletionContext::Global)
}

/// Resolve the type for associated access (e.g., `Type::`).
fn resolve_associated_access_context(
    input: &CompletionInput,
    file_id: FileId,
    offset_before_colons: usize,
) -> Option<CompletionContext> {
    // Look backwards to find the identifier before `::`
    let hir_file = input.hir_files.get(&file_id)?;
    let source_file = input.source_db.file(file_id)?;

    // Find the word boundary before `::`
    let text_before = &source_file.source()[..offset_before_colons];
    let type_name = find_identifier_before_offset(text_before)?;

    // Try to resolve this as a type name in scope
    // For now, check if it matches any known enum/struct/protocol
    for item in input.semantic.global_items.items_in_scope(file_id) {
        if item.name == type_name {
            match item.kind {
                crate::frontend::resolver::ItemKind::Enum => {
                    // Found an enum - create associated access context
                    let enum_type = Type::Named {
                        item_id: item.id,
                        kind: crate::frontend::semantic::NamedTypeKind::Enum,
                    };
                    return Some(CompletionContext::EnumCaseAccess {
                        enum_type,
                    });
                }
                crate::frontend::resolver::ItemKind::Struct => {
                    // Found a struct - create associated access context
                    let struct_type = Type::Named {
                        item_id: item.id,
                        kind: crate::frontend::semantic::NamedTypeKind::Struct,
                    };
                    return Some(CompletionContext::AssociatedAccess {
                        base_type: struct_type,
                    });
                }
                _ => {
                    // For other types (protocols), use associated access
                    let type_type = Type::Named {
                        item_id: item.id,
                        kind:
                            crate::frontend::semantic::NamedTypeKind::Protocol,
                    };
                    return Some(CompletionContext::AssociatedAccess {
                        base_type: type_type,
                    });
                }
            }
        }
    }

    // Fallback to global if we can't resolve the type
    Some(CompletionContext::Global)
}

/// Resolve the type for field access (e.g., `value.`).
fn resolve_field_access_context(
    input: &CompletionInput,
    file_id: FileId,
    offset_before_dot: usize,
) -> Option<CompletionContext> {
    // Look backwards to find the expression before `.`
    let hir_file = input.hir_files.get(&file_id)?;
    let source_file = input.source_db.file(file_id)?;

    // Find the word boundary before `.`
    let text_before = &source_file.source()[..offset_before_dot];
    let identifier = find_identifier_before_offset(text_before)?;

    // Try to find a local binding or item with this name
    // and get its type
    if let Some(ty) = find_type_of_binding(input, file_id, &identifier) {
        // Check if this is an enum type for enum case completion
        if is_nominal_type(&ty) {
            // For enum types, `.` might mean enum case access in some contexts
            // But typically `.` means field/method access
            return Some(CompletionContext::MemberAccess { receiver_type: ty });
        } else {
            return Some(CompletionContext::MemberAccess { receiver_type: ty });
        }
    }

    // Couldn't resolve the type - fall back to global
    Some(CompletionContext::Global)
}

/// Find the identifier immediately before an offset.
fn find_identifier_before_offset(text: &str) -> Option<String> {
    // Find the last word before the offset
    let chars: Vec<char> = text.chars().collect();
    let mut end = chars.len();

    // Skip whitespace
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }

    // Find the start of the identifier
    let mut start = end;
    while start > 0
        && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_')
    {
        start -= 1;
    }

    if start < end {
        Some(text[start..end].to_string())
    } else {
        None
    }
}

/// Try to find the type of a binding (local variable or item) by name.
fn find_type_of_binding(
    input: &CompletionInput,
    file_id: FileId,
    name: &str,
) -> Option<Type> {
    // First, check local bindings in resolved bodies
    for body in input.semantic.resolved_bodies.iter() {
        if body.containing_scope_file_id != file_id {
            continue;
        }

        for local in &body.locals {
            if local.name == name {
                // Try to get the type from the typed body
                let env =
                    input.semantic.body_envs.env(&body.owner, body.body_index);
                if let Some(env) = env {
                    let typed_body = input
                        .semantic
                        .typed_bodies
                        .body(&body.owner, body.body_index);
                    if let Some(typed_body) = typed_body {
                        let hir_local_id =
                            env.hir_local_id_for_resolved_local(local.id)?;
                        if let Some(ty) =
                            typed_body.local_types.get(&hir_local_id)
                        {
                            return Some(ty.clone());
                        }
                    }
                }

                // If we can't get the typed body, fall back to checking if this is a known type
                // For enums, we might be able to infer from usage
            }
        }
    }

    // Next, check if it's a known enum/struct/protocol
    for item in input.semantic.global_items.items_in_scope(file_id) {
        if item.name == name {
            let kind = match item.kind {
                crate::frontend::resolver::ItemKind::Enum => {
                    crate::frontend::semantic::NamedTypeKind::Enum
                }
                crate::frontend::resolver::ItemKind::Struct => {
                    crate::frontend::semantic::NamedTypeKind::Struct
                }
                crate::frontend::resolver::ItemKind::Protocol => {
                    crate::frontend::semantic::NamedTypeKind::Protocol
                }
                crate::frontend::resolver::ItemKind::Function
                | crate::frontend::resolver::ItemKind::Scope => {
                    continue;
                }
            };

            return Some(Type::Named {
                item_id: item.id,
                kind,
            });
        }
    }

    None
}

/// Determine the receiver type for field/namespace access.
fn determine_access_context(
    _input: &CompletionInput,
    node_context: &HirNodeContext,
) -> Option<CompletionContext> {
    // Extract the receiver type from the expression info
    let (_expr_id, maybe_type) = node_context.expr_info.as_ref()?;

    match maybe_type {
        Some(ty) => {
            // Check if this is a namespace access on a type
            if is_nominal_type(ty) {
                Some(CompletionContext::AssociatedAccess {
                    base_type: ty.clone(),
                })
            } else {
                Some(CompletionContext::MemberAccess {
                    receiver_type: ty.clone(),
                })
            }
        }
        None => {
            // Type is unknown - this might be during editing before type checking
            // Degrade gracefully
            None
        }
    }
}

/// Check if a type is a nominal type (struct, enum, or protocol).
fn is_nominal_type(ty: &Type) -> bool {
    matches!(ty, Type::Named { .. })
}
