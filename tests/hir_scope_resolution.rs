use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{LocalKind, build_hir_local_binding_table};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DesugaredFile, HirExprId, HirExprKind, HirFile, HirModule, HirPatId,
    HirPatKind, lower_to_hir,
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

fn binding_pat_ids(module: &HirModule, name: &str) -> Vec<(HirPatId, usize)> {
    let mut pats = module
        .patterns
        .iter()
        .filter_map(|(pat_id, pat)| match &pat.kind {
            HirPatKind::Binding { name: pat_name } if pat_name == name => {
                Some((*pat_id, pat.origin.span.start))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    pats.sort_by_key(|(_, start)| *start);
    pats
}

#[test]
fn shadowing_uses_nearest_hir_binding() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f(_ x: i32) { let x = 1; { let x = 2; x; }; x; }",
    );

    let desugared_files = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let table = build_hir_local_binding_table(&hir_files, &hir_modules)
        .expect("HIR scope resolution should succeed");
    let module = hir_modules
        .get(&root.file_id)
        .expect("module should exist for root file");

    let x_exprs = path_expr_ids(module, "x");
    assert_eq!(x_exprs.len(), 2);

    let x_patterns = binding_pat_ids(module, "x");
    assert_eq!(x_patterns.len(), 2);
    let outer_x = table
        .binding_for_pat(root.file_id, x_patterns[0].0)
        .expect("outer let pattern binding");
    let inner_x = table
        .binding_for_pat(root.file_id, x_patterns[1].0)
        .expect("inner let pattern binding");

    let first_ref = table
        .binding_for_expr(root.file_id, x_exprs[0].0)
        .expect("inner x reference should resolve");
    let second_ref = table
        .binding_for_expr(root.file_id, x_exprs[1].0)
        .expect("outer x reference should resolve");

    assert_eq!(first_ref, inner_x);
    assert_eq!(second_ref, outer_x);
    assert_ne!(first_ref, second_ref);
}

#[test]
fn nested_block_scope_does_not_leak_hir_bindings() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f() { { let y = 1; y; }; y; }",
    );

    let desugared_files = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let table = build_hir_local_binding_table(&hir_files, &hir_modules)
        .expect("HIR scope resolution should succeed");
    let module = hir_modules
        .get(&root.file_id)
        .expect("module should exist for root file");

    let y_exprs = path_expr_ids(module, "y");
    assert_eq!(y_exprs.len(), 2);
    let y_patterns = binding_pat_ids(module, "y");
    assert_eq!(y_patterns.len(), 1);

    let inner_binding = table
        .binding_for_pat(root.file_id, y_patterns[0].0)
        .expect("y pattern binding should resolve");

    let inner_ref = table
        .binding_for_expr(root.file_id, y_exprs[0].0)
        .expect("inner y reference should resolve");
    let outer_ref = table.binding_for_expr(root.file_id, y_exprs[1].0);

    assert_eq!(inner_ref, inner_binding);
    assert!(outer_ref.is_none());
}

#[test]
fn local_let_binding_takes_precedence_over_parameter() {
    let mut db = SourceDb::new();
    let root = add_and_parse(
        &mut db,
        "src/root.cx",
        "fn f(_ x: i32) { let x = 1; x; }",
    );

    let desugared_files = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let table = build_hir_local_binding_table(&hir_files, &hir_modules)
        .expect("HIR scope resolution should succeed");
    let module = hir_modules
        .get(&root.file_id)
        .expect("module should exist for root file");

    let param_x = table
        .iter_bindings()
        .find(|binding| {
            binding.file_id == root.file_id
                && binding.name == "x"
                && binding.kind == LocalKind::Parameter
        })
        .map(|binding| binding.id)
        .expect("x parameter binding");

    let local_x_pat = binding_pat_ids(module, "x")
        .into_iter()
        .next()
        .expect("x local binding pattern")
        .0;
    let local_x = table
        .binding_for_pat(root.file_id, local_x_pat)
        .expect("x local binding from pattern");

    let x_ref_expr = path_expr_ids(module, "x")
        .into_iter()
        .next()
        .expect("x expression reference")
        .0;
    let resolved = table
        .binding_for_expr(root.file_id, x_ref_expr)
        .expect("x reference should resolve");

    assert_eq!(resolved, local_x);
    assert_ne!(resolved, param_x);
}
