use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    HirCollectedItemKind, HirItemTableError, build_hir_item_table,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{DesugaredFile, HirFile, HirModule, lower_to_hir};
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

#[test]
fn collects_multiple_hir_item_kinds_with_file_context() {
    let mut db = SourceDb::new();
    let root =
        add_and_parse(&mut db, "src/root.cx", "fn top() {} struct Client {}");
    let extra = add_and_parse(
        &mut db,
        "src/extra.cx",
        "enum Mode { Ready } protocol Service {} impl Client { fn build() {} } extern libc { fn strlen(s: *void) -> usize; }",
    );

    let desugared_files = vec![root.clone(), extra.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let table =
        build_hir_item_table(&hir_files, &hir_modules).expect("table build");

    assert_eq!(table.len(), 6);
    assert_eq!(table.item_refs_in_file(root.file_id).len(), 2);
    assert_eq!(table.item_refs_in_file(extra.file_id).len(), 4);

    let top = table
        .get(table.item_ref_by_name("top").expect("top ref"))
        .expect("top item");
    assert_eq!(top.kind, HirCollectedItemKind::Function);
    assert_eq!(top.file_id, root.file_id);

    let client = table
        .get(table.item_ref_by_name("Client").expect("client ref"))
        .expect("client item");
    assert_eq!(client.kind, HirCollectedItemKind::Struct);
    assert_eq!(client.file_id, root.file_id);

    let mode = table
        .get(table.item_ref_by_name("Mode").expect("mode ref"))
        .expect("mode item");
    assert_eq!(mode.kind, HirCollectedItemKind::Enum);
    assert_eq!(mode.file_id, extra.file_id);

    let service = table
        .get(table.item_ref_by_name("Service").expect("service ref"))
        .expect("service item");
    assert_eq!(service.kind, HirCollectedItemKind::Protocol);
    assert_eq!(service.file_id, extra.file_id);

    let impl_client = table
        .get(
            table
                .item_ref_by_name("impl Client")
                .expect("impl client ref"),
        )
        .expect("impl client item");
    assert_eq!(impl_client.kind, HirCollectedItemKind::Impl);
    assert_eq!(impl_client.file_id, extra.file_id);

    let extern_libc = table
        .get(
            table
                .item_ref_by_name("extern libc")
                .expect("extern libc ref"),
        )
        .expect("extern libc item");
    assert_eq!(extern_libc.kind, HirCollectedItemKind::Extern);
    assert_eq!(extern_libc.file_id, extra.file_id);
}

#[test]
fn same_bare_name_in_different_files_is_allowed() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn dup() {}");
    let other = add_and_parse(&mut db, "src/other.cx", "fn dup() {}");

    let desugared_files = vec![root.clone(), other.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let table =
        build_hir_item_table(&hir_files, &hir_modules).expect("table build");

    assert_eq!(table.len(), 2);
    assert_eq!(table.item_refs_in_file(root.file_id).len(), 1);
    assert_eq!(table.item_refs_in_file(other.file_id).len(), 1);

    let dup_file_ids = table
        .iter()
        .filter(|item| item.name == "dup")
        .map(|item| item.file_id)
        .collect::<Vec<_>>();
    assert_eq!(dup_file_ids.len(), 2);
    assert!(dup_file_ids.contains(&root.file_id));
    assert!(dup_file_ids.contains(&other.file_id));
}

#[test]
fn duplicate_hir_item_name_in_same_file_is_reported() {
    let mut db = SourceDb::new();
    let root = add_and_parse(&mut db, "src/root.cx", "fn dup() {} fn dup() {}");

    let desugared_files = vec![root.clone()];
    let (hir_files, hir_modules) = lower_project_hir(&desugared_files);
    let error = build_hir_item_table(&hir_files, &hir_modules)
        .expect_err("same-scope duplicate should be detected");

    match error {
        HirItemTableError::DuplicateName {
            name,
            first,
            duplicate,
        } => {
            assert_eq!(name, "dup");
            assert_eq!(first.file_id, root.file_id);
            assert_eq!(duplicate.file_id, root.file_id);
            assert_ne!(first.item_id, duplicate.item_id);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
