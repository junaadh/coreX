use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    HirImportError, HirImportTables, HirPathResolution, build_hir_item_table,
    build_hir_path_resolution_table_with_graph_and_imports,
    resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DesugaredFile, HirExprId, HirExprKind, HirFile, HirItemRef, HirModule,
    ResolvedScopeKind, lower_to_hir,
};
use std::collections::BTreeMap;

fn parsed_to_desugared(parsed: core_x::frontend::ParsedFile) -> DesugaredFile {
    DesugaredFile {
        file_id: parsed.file_id,
        ast: parsed.ast,
        diagnostics: parsed.diagnostics,
        provenance_map: core_x::frontend::expansion::ProvenanceMap::new(
            parsed.file_id,
        ),
    }
}

fn add_and_parse(db: &mut SourceDb, path: &str, source: &str) -> DesugaredFile {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );
    parsed_to_desugared(parsed)
}

fn lower_project_hir(
    files: &[DesugaredFile],
) -> (Vec<HirFile>, BTreeMap<FileId, HirModule>) {
    let mut hir_files = Vec::new();
    let mut hir_modules = BTreeMap::new();

    for file in files {
        let (hir_file, hir_module) = lower_to_hir(file);
        hir_modules.insert(hir_file.file_id, hir_module);
        hir_files.push(hir_file);
    }

    (hir_files, hir_modules)
}

fn resolve_library_graph(
    db: &SourceDb,
    parsed_files: &[DesugaredFile],
    root_file_id: FileId,
) -> core_x::frontend::ScopeGraph {
    resolve_project_scopes(
        db,
        parsed_files,
        root_file_id,
        ResolvedScopeKind::Root,
    )
    .expect("scope graph should resolve")
}

fn item_ref_by_name_in_file(
    table: &core_x::frontend::HirItemTable,
    file_id: FileId,
    name: &str,
) -> HirItemRef {
    table
        .item_refs_in_file(file_id)
        .iter()
        .copied()
        .find(|item_ref| {
            table
                .get(*item_ref)
                .is_some_and(|item| item.name.as_str() == name)
        })
        .expect("expected item in file")
}

fn find_path_expr_id(module: &HirModule, name: &str) -> HirExprId {
    module
        .exprs
        .iter()
        .find_map(|(expr_id, expr)| match &expr.kind {
            HirExprKind::Path(path)
                if path.segments.as_slice() == [name.to_string()] =>
            {
                Some(*expr_id)
            }
            _ => None,
        })
        .expect("expected path expression")
}

fn namespace_segments(
    module: &HirModule,
    expr_id: HirExprId,
) -> Option<Vec<String>> {
    let expr = module.exprs.get(&expr_id)?;
    match &expr.kind {
        HirExprKind::Path(path) => Some(path.segments.clone()),
        HirExprKind::NamespaceField { base, name, .. } => {
            let mut segments = namespace_segments(module, *base)?;
            segments.push(name.clone());
            Some(segments)
        }
        _ => None,
    }
}

fn find_namespace_expr_id(module: &HirModule, expected: &[&str]) -> HirExprId {
    let expected = expected.iter().map(ToString::to_string).collect::<Vec<_>>();
    module
        .exprs
        .keys()
        .copied()
        .find(|expr_id| {
            namespace_segments(module, *expr_id).as_deref()
                == Some(expected.as_slice())
        })
        .expect("expected namespace expression")
}

#[test]
fn resolves_cross_file_function_via_module_path() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "scope net; fn run() { net::helper(); }",
    );
    let net = add_and_parse(&mut db, "src/net.cx", "fn helper() {}");
    let parsed_files = vec![root.clone(), net.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (hir_files, hir_modules) = lower_project_hir(&parsed_files);
    let item_table =
        build_hir_item_table(&hir_files, &hir_modules).expect("item table");
    let imports = HirImportTables::resolve_with_graph(
        &graph,
        &hir_files,
        &hir_modules,
        &item_table,
    )
    .expect("imports should resolve");
    let path_table = build_hir_path_resolution_table_with_graph_and_imports(
        &hir_files,
        &hir_modules,
        &graph,
        Some(&imports),
    )
    .expect("path resolution should succeed");

    let root_module = hir_modules.get(&root.file_id).expect("root module");
    let expr_id = find_namespace_expr_id(root_module, &["net", "helper"]);
    let helper_ref =
        item_ref_by_name_in_file(&item_table, net.file_id, "helper");
    let helper_segments = vec!["net".to_string(), "helper".to_string()];

    assert_eq!(
        path_table.by_expr(root.file_id, expr_id),
        Some(HirPathResolution::Item(helper_ref))
    );
    assert_eq!(
        path_table.by_path(root.file_id, expr_id, &helper_segments),
        Some(HirPathResolution::Item(helper_ref))
    );
}

#[test]
fn resolves_import_alias_to_cross_file_function() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; scope app;");
    let net = add_and_parse(&mut db, "src/net.cx", "fn helper() {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::helper as remote; fn run() { remote(); }",
    );
    let parsed_files = vec![root.clone(), net.clone(), app.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (hir_files, hir_modules) = lower_project_hir(&parsed_files);
    let item_table =
        build_hir_item_table(&hir_files, &hir_modules).expect("item table");
    let imports = HirImportTables::resolve_with_graph(
        &graph,
        &hir_files,
        &hir_modules,
        &item_table,
    )
    .expect("imports should resolve");
    let path_table = build_hir_path_resolution_table_with_graph_and_imports(
        &hir_files,
        &hir_modules,
        &graph,
        Some(&imports),
    )
    .expect("path resolution should succeed");

    let app_module = hir_modules.get(&app.file_id).expect("app module");
    let remote_expr = find_path_expr_id(app_module, "remote");
    let helper_ref =
        item_ref_by_name_in_file(&item_table, net.file_id, "helper");

    assert_eq!(
        path_table.by_expr(app.file_id, remote_expr),
        Some(HirPathResolution::Item(helper_ref))
    );
}

#[test]
fn unresolved_hir_import_is_reported_as_diagnostic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "scope net; scope app;");
    let net = add_and_parse(&mut db, "src/net.cx", "fn helper() {}");
    let app = add_and_parse(
        &mut db,
        "src/app.cx",
        "use root::net::missing; fn run() {}",
    );
    let parsed_files = vec![root.clone(), net, app.clone()];

    let graph = resolve_library_graph(&db, &parsed_files, root.file_id);
    let (hir_files, hir_modules) = lower_project_hir(&parsed_files);
    let item_table =
        build_hir_item_table(&hir_files, &hir_modules).expect("item table");
    let (imports, diagnostics) =
        HirImportTables::resolve_with_graph_and_named_roots_and_diagnostics(
            &graph,
            &hir_files,
            &hir_modules,
            &item_table,
            &BTreeMap::new(),
        )
        .expect("diagnostic import resolution should succeed");

    assert!(diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic,
            HirImportError::UnresolvedPath { from_file_id, path }
                if *from_file_id == app.file_id
                    && path
                        == &vec![
                            "root".to_string(),
                            "net".to_string(),
                            "missing".to_string()
                        ]
        )
    }));

    let app_imports = imports.get(app.file_id).expect("app import table");
    assert!(app_imports.get("missing").is_none());
}
