use core_x::frontend::ast::Span;
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    NamedImportRoot, ScopeResolver,
    resolve_project_imports_with_named_roots_and_diagnostics,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DefinitionTarget, ExternalDefinitionLocation, ExternalSemanticLookup,
    ParsedFile, analyze_semantics_with_external_lookup,
    build_external_semantic_lookup, collect_item_definition_locations,
    completion_candidates_for_file, lookup_definition_target,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn parse_sources(
    sources: &[(&str, &str)],
) -> (SourceDb, Vec<ParsedFile>, BTreeMap<String, FileId>) {
    let mut db = SourceDb::new();
    let mut parsed_files = Vec::with_capacity(sources.len());
    let mut file_ids = BTreeMap::new();

    for &(path, source) in sources {
        let file_id = db.add_file(path, source);
        let file = db.file(file_id).expect("source file should exist");
        let parsed = parse_source_file_from_source_file(file)
            .expect("source should parse");
        parsed_files.push(parsed);
        file_ids.insert(path.to_string(), file_id);
    }

    (db, parsed_files, file_ids)
}

fn byte_offset(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("missing `{needle}` in source"))
}

#[test]
fn local_binding_definition_lookup() {
    let source = "fn main() { let value = 1; value; }";
    let (db, parsed_files, file_ids) =
        parse_sources(&[("src/root.cx", source)]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let empty_roots = BTreeMap::new();
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            &parsed_files,
            &empty_roots,
            &db,
        );
    let external = ExternalSemanticLookup::new();
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &graph,
        &parsed_files,
        &imports,
        &external,
    );
    let item_definitions = collect_item_definition_locations(
        &graph,
        &parsed_files,
        &semantic.global_items,
    );

    let reference_offset = byte_offset(source, "value; }");
    let target = lookup_definition_target(
        &semantic,
        &imports,
        &external,
        &item_definitions,
        root_file_id,
        reference_offset,
        Some("value"),
    )
    .expect("definition target");
    match target {
        DefinitionTarget::LocalBinding { location, .. } => {
            assert_eq!(location.file_id, root_file_id);
            assert!(location.span.start < reference_offset);
        }
        other => panic!("expected local binding definition, got {other:?}"),
    }
}

#[test]
fn current_target_item_definition_lookup() {
    let source = "fn helper() {} fn main() { helper(); }";
    let (db, parsed_files, file_ids) =
        parse_sources(&[("src/root.cx", source)]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let empty_roots = BTreeMap::new();
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            &parsed_files,
            &empty_roots,
            &db,
        );
    let external = ExternalSemanticLookup::new();
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &graph,
        &parsed_files,
        &imports,
        &external,
    );
    let item_definitions = collect_item_definition_locations(
        &graph,
        &parsed_files,
        &semantic.global_items,
    );

    let reference_offset = byte_offset(source, "helper();");
    let target = lookup_definition_target(
        &semantic,
        &imports,
        &external,
        &item_definitions,
        root_file_id,
        reference_offset,
        Some("helper"),
    )
    .expect("definition target");
    match target {
        DefinitionTarget::CurrentTargetItem { location, .. } => {
            assert_eq!(location.file_id, root_file_id);
            assert!(location.span.start < reference_offset);
        }
        other => {
            panic!("expected current-target item definition, got {other:?}")
        }
    }
}

#[test]
fn binary_to_library_definition_lookup_uses_external_target() {
    let main_source = "use app::shared_logic;\nfn main() { shared_logic(); }\n";
    let (db, parsed_files, file_ids) = parse_sources(&[
        ("src/root.cx", "pub fn shared_logic() {}\n"),
        ("src/main.cx", main_source),
    ]);
    let root_file_id = file_ids["src/root.cx"];
    let main_file_id = file_ids["src/main.cx"];

    let scope_resolver = ScopeResolver::new(&db, &parsed_files);
    let library_graph = scope_resolver
        .resolve_library_root(root_file_id)
        .expect("library graph");
    let binary_graph = scope_resolver
        .resolve_binary_root(main_file_id)
        .expect("binary graph");

    let mut named_roots = BTreeMap::new();
    let path_by_file_id = file_ids
        .iter()
        .map(|(path, file_id)| (*file_id, PathBuf::from(path)))
        .collect();
    named_roots.insert(
        "app".to_string(),
        NamedImportRoot::LoadedLibrary {
            graph: library_graph,
            parsed_files: parsed_files.clone(),
            path_by_file_id,
        },
    );
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &binary_graph,
            &parsed_files,
            &named_roots,
            &db,
        );
    let external_lookup = build_external_semantic_lookup(
        &db,
        &named_roots,
        &binary_graph,
        &parsed_files,
    );
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &binary_graph,
        &parsed_files,
        &imports,
        &external_lookup,
    );
    let item_definitions = collect_item_definition_locations(
        &binary_graph,
        &parsed_files,
        &semantic.global_items,
    );

    let reference_offset = byte_offset(main_source, "shared_logic();");
    let target = lookup_definition_target(
        &semantic,
        &imports,
        &external_lookup,
        &item_definitions,
        main_file_id,
        reference_offset,
        Some("shared_logic"),
    )
    .expect("definition target");
    match target {
        DefinitionTarget::ExternalItem {
            root_name,
            path,
            location,
        } => {
            assert_eq!(root_name, "app");
            assert_eq!(path, vec!["shared_logic".to_string()]);
            assert!(location.file_path.ends_with("src/root.cx"));
        }
        other => panic!("expected external definition, got {other:?}"),
    }
}

#[test]
fn dependency_root_definition_lookup_uses_external_context() {
    let source = "fn main() { util::fmt::Writer(); }\n";
    let (db, parsed_files, file_ids) =
        parse_sources(&[("src/root.cx", source)]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let empty_roots = BTreeMap::new();
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            &parsed_files,
            &empty_roots,
            &db,
        );
    let mut external_lookup = ExternalSemanticLookup::new();
    external_lookup.insert_named_root_definition(
        "util".to_string(),
        vec!["fmt".to_string(), "Writer".to_string()],
        ExternalDefinitionLocation {
            file_path: PathBuf::from("/deps/util/src/fmt.cx"),
            span: Span::new(10, 18),
        },
    );
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &graph,
        &parsed_files,
        &imports,
        &external_lookup,
    );
    let item_definitions = collect_item_definition_locations(
        &graph,
        &parsed_files,
        &semantic.global_items,
    );

    let reference_offset = byte_offset(source, "Writer()");
    let target = lookup_definition_target(
        &semantic,
        &imports,
        &external_lookup,
        &item_definitions,
        root_file_id,
        reference_offset,
        Some("Writer"),
    )
    .expect("definition target");
    match target {
        DefinitionTarget::ExternalItem {
            root_name,
            path,
            location,
        } => {
            assert_eq!(root_name, "util");
            assert_eq!(path, vec!["fmt".to_string(), "Writer".to_string()]);
            assert_eq!(
                location.file_path,
                PathBuf::from("/deps/util/src/fmt.cx")
            );
        }
        other => {
            panic!("expected external dependency definition, got {other:?}")
        }
    }
}

#[test]
fn definition_lookup_is_deterministic() {
    let source = "fn helper() {} fn main() { helper(); }\n";
    let (db, parsed_files, file_ids) =
        parse_sources(&[("src/root.cx", source)]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let empty_roots = BTreeMap::new();
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            &parsed_files,
            &empty_roots,
            &db,
        );
    let external = ExternalSemanticLookup::new();
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &graph,
        &parsed_files,
        &imports,
        &external,
    );
    let item_definitions = collect_item_definition_locations(
        &graph,
        &parsed_files,
        &semantic.global_items,
    );
    let reference_offset = byte_offset(source, "helper();");

    let first = lookup_definition_target(
        &semantic,
        &imports,
        &external,
        &item_definitions,
        root_file_id,
        reference_offset,
        Some("helper"),
    );
    let second = lookup_definition_target(
        &semantic,
        &imports,
        &external,
        &item_definitions,
        root_file_id,
        reference_offset,
        Some("helper"),
    );

    assert_eq!(first, second);
}

#[test]
fn external_definition_is_not_retagged_as_local_item() {
    let main_source = "fn main() { lib_and_bin::shared_logic(); }\n";
    let (db, parsed_files, file_ids) = parse_sources(&[
        ("src/root.cx", "pub fn shared_logic() {}\n"),
        ("src/main.cx", main_source),
    ]);
    let root_file_id = file_ids["src/root.cx"];
    let main_file_id = file_ids["src/main.cx"];

    let scope_resolver = ScopeResolver::new(&db, &parsed_files);
    let library_graph = scope_resolver
        .resolve_library_root(root_file_id)
        .expect("library graph");
    let binary_graph = scope_resolver
        .resolve_binary_root(main_file_id)
        .expect("binary graph");

    let mut named_roots = BTreeMap::new();
    let path_by_file_id = file_ids
        .iter()
        .map(|(path, file_id)| (*file_id, PathBuf::from(path)))
        .collect();
    named_roots.insert(
        "lib_and_bin".to_string(),
        NamedImportRoot::LoadedLibrary {
            graph: library_graph,
            parsed_files: parsed_files.clone(),
            path_by_file_id,
        },
    );
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &binary_graph,
            &parsed_files,
            &named_roots,
            &db,
        );
    let external_lookup = build_external_semantic_lookup(
        &db,
        &named_roots,
        &binary_graph,
        &parsed_files,
    );
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &binary_graph,
        &parsed_files,
        &imports,
        &external_lookup,
    );
    let item_definitions = collect_item_definition_locations(
        &binary_graph,
        &parsed_files,
        &semantic.global_items,
    );

    let reference_offset = byte_offset(main_source, "shared_logic()");
    let target = lookup_definition_target(
        &semantic,
        &imports,
        &external_lookup,
        &item_definitions,
        main_file_id,
        reference_offset,
        Some("shared_logic"),
    )
    .expect("definition target");

    assert!(
        matches!(target, DefinitionTarget::ExternalItem { .. }),
        "cross-target definition should remain external, got {target:?}"
    );
}

#[test]
fn completion_candidates_are_deterministic_and_include_semantic_sources() {
    let source = "fn helper() {} fn main() { let value = 1; value; }\n";
    let (db, parsed_files, file_ids) =
        parse_sources(&[("src/root.cx", source)]);
    let root_file_id = file_ids["src/root.cx"];

    let graph = ScopeResolver::new(&db, &parsed_files)
        .resolve_library_root(root_file_id)
        .expect("scope graph");
    let empty_roots = BTreeMap::new();
    let (_, imports, _) =
        resolve_project_imports_with_named_roots_and_diagnostics(
            &graph,
            &parsed_files,
            &empty_roots,
            &db,
        );
    let external = ExternalSemanticLookup::new();
    let semantic = analyze_semantics_with_external_lookup(
        &db,
        &graph,
        &parsed_files,
        &imports,
        &external,
    );

    let first =
        completion_candidates_for_file(&semantic, &imports, root_file_id);
    let second =
        completion_candidates_for_file(&semantic, &imports, root_file_id);
    assert_eq!(first, second);
    assert!(first.iter().any(|entry| entry.label == "value"));
    assert!(first.iter().any(|entry| entry.label == "helper"));
}
