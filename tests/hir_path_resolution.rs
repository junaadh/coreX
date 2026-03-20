use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    HirItemRef, HirPathResolution, build_hir_local_binding_table,
    build_hir_path_resolution_table,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DesugaredFile, HirExprId, HirExprKind, HirFile, HirItemKind, HirModule,
    HirPatId, HirPatKind, lower_to_hir,
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

fn path_expr_ids(module: &HirModule, name: &str) -> Vec<(HirExprId, usize)> {
    let mut exprs = module
        .exprs
        .iter()
        .filter_map(|(expr_id, expr)| match &expr.kind {
            HirExprKind::Path(path)
                if path.segments.as_slice() == [name.to_string()] =>
            {
                Some((*expr_id, expr.origin.span.start))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    exprs.sort_by_key(|(_, start)| *start);
    exprs
}

fn binding_pat_ids(module: &HirModule, name: &str) -> Vec<HirPatId> {
    module
        .patterns
        .iter()
        .filter_map(|(pat_id, pat)| match &pat.kind {
            HirPatKind::Binding { name: pat_name } if pat_name == name => {
                Some(*pat_id)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn resolves_hir_local_variable_references() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f(_ x: i32) { let y = x; y; }",
    );

    let desugared = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared);
    let path_table = build_hir_path_resolution_table(&hir_files, &hir_modules)
        .expect("path resolution should succeed");
    let local_table = build_hir_local_binding_table(&hir_files, &hir_modules)
        .expect("local scope resolution should succeed");
    let module = hir_modules.get(&root.file_id).expect("root module");

    let x_ref = path_expr_ids(module, "x")
        .into_iter()
        .next()
        .expect("x reference expression")
        .0;
    let y_ref = path_expr_ids(module, "y")
        .into_iter()
        .next()
        .expect("y reference expression")
        .0;

    let y_pat = binding_pat_ids(module, "y")
        .into_iter()
        .next()
        .expect("y binding pattern");
    let y_binding = local_table
        .binding_for_pat(root.file_id, y_pat)
        .expect("y pattern should map to local binding");

    assert!(matches!(
        path_table.by_expr(root.file_id, x_ref),
        Some(HirPathResolution::Local(_))
    ));
    assert_eq!(
        path_table.by_expr(root.file_id, y_ref),
        Some(HirPathResolution::Local(y_binding))
    );
}

#[test]
fn resolves_hir_function_call_name_to_item() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn helper() {} fn run() { helper(); }",
    );

    let desugared = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared);
    let path_table = build_hir_path_resolution_table(&hir_files, &hir_modules)
        .expect("path resolution should succeed");
    let module = hir_modules.get(&root.file_id).expect("root module");

    let helper_item_id = module
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            HirItemKind::Function(function) if function.name == "helper" => {
                Some(*item_id)
            }
            _ => None,
        })
        .expect("helper item id");

    let helper_expr = path_expr_ids(module, "helper")
        .into_iter()
        .next()
        .expect("helper call path expr")
        .0;
    let helper_segments = vec!["helper".to_string()];

    assert_eq!(
        path_table.by_expr(root.file_id, helper_expr),
        Some(HirPathResolution::Item(HirItemRef::new(
            root.file_id,
            helper_item_id
        )))
    );
    assert_eq!(
        path_table.by_path(root.file_id, helper_expr, &helper_segments),
        Some(HirPathResolution::Item(HirItemRef::new(
            root.file_id,
            helper_item_id
        )))
    );
}

#[test]
fn unresolved_hir_name_emits_diagnostic() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn run() { missing; }");

    let desugared = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared);
    let path_table = build_hir_path_resolution_table(&hir_files, &hir_modules)
        .expect("path resolution should succeed");
    let module = hir_modules.get(&root.file_id).expect("root module");
    let missing_expr = path_expr_ids(module, "missing")
        .into_iter()
        .next()
        .expect("missing path expr")
        .0;

    assert_eq!(path_table.unresolved_diagnostics.len(), 1);
    assert_eq!(path_table.unresolved_diagnostics[0].file_id, root.file_id);
    assert_eq!(path_table.unresolved_diagnostics[0].expr_id, missing_expr);
    assert_eq!(
        path_table.unresolved_diagnostics[0].segments,
        vec!["missing".to_string()]
    );
    assert_eq!(path_table.by_expr(root.file_id, missing_expr), None);
}
