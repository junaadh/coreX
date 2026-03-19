use core_x::frontend::ast::{
    Expr, IfStmtElse, InitOriginKind, Item, Modifier, Stmt, StructMember, Type,
};
use core_x::frontend::parser::parse_source_file_from_source_file;
use core_x::frontend::resolver::{
    BodyKind, DeclarationOwner, GlobalItemTable, ResolvedDeclaration,
    ResolvedScopeKind, resolve_bodies, resolve_declaration_types,
    resolve_project_imports, resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    DesugaredFile, ExpansionOptions, desugar_files, expand_parsed_files,
};

fn parse_single_file(
    db: &mut SourceDb,
    path: &str,
    source: &str,
) -> (FileId, Vec<core_x::frontend::ParsedFile>) {
    let file_id = db.add_file(path, source);
    let file = db.file(file_id).expect("file should exist");
    let parsed =
        parse_source_file_from_source_file(file).expect("parse should succeed");
    assert!(
        parsed.diagnostics.is_empty(),
        "strict parse should not emit diagnostics"
    );
    (file_id, vec![parsed])
}

fn expand_and_desugar(
    db: &SourceDb,
    parsed_files: &[core_x::frontend::ParsedFile],
) -> (Vec<core_x::frontend::ExpandedFile>, Vec<DesugaredFile>) {
    let expanded =
        expand_parsed_files(db, parsed_files, ExpansionOptions::default());
    let desugared = desugar_files(&expanded);
    (expanded, desugared)
}

#[test]
fn desugar_identity_pass_on_simple_file() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "fn simple(_ x: i32) -> i32 { x }",
    );
    let (expanded, desugared) = expand_and_desugar(&db, &parsed_files);

    assert_eq!(expanded.len(), 1);
    assert_eq!(desugared.len(), 1);
    assert_eq!(desugared[0].ast, expanded[0].ast);
    assert_eq!(desugared[0].diagnostics, expanded[0].diagnostics);
}

#[test]
fn desugar_removes_grouped_expression_wrappers() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "fn grouped_expr() -> i32 { (1) }",
    );
    let (_, desugared) = expand_and_desugar(&db, &parsed_files);

    let Item::Function(function_decl) = &desugared[0].ast.items[0].node else {
        panic!("expected function item");
    };
    let Some(tail_expr) = &function_decl.node.body.tail_expr else {
        panic!("expected tail expression");
    };
    assert!(matches!(tail_expr.node, Expr::IntegerLiteral(_)));
}

#[test]
fn desugar_removes_grouped_type_wrappers() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "fn grouped_type(_ x: (i32)) -> (i32) { x }",
    );
    let (_, desugared) = expand_and_desugar(&db, &parsed_files);

    let Item::Function(function_decl) = &desugared[0].ast.items[0].node else {
        panic!("expected function item");
    };
    assert!(matches!(
        function_decl.node.params[0].node.ty.node,
        Type::Named { .. }
    ));
    assert!(matches!(
        function_decl.node.return_type,
        Some(core_x::frontend::ast::Spanned {
            node: Type::Named { .. },
            ..
        })
    ));
}

#[test]
fn desugar_normalizes_if_and_block_shapes() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "fn f() -> i32 { if true { 1 } else 2 } fn g() { if true {} else if false {} }",
    );
    let (_, desugared) = expand_and_desugar(&db, &parsed_files);

    let Item::Function(f_decl) = &desugared[0].ast.items[0].node else {
        panic!("expected first function");
    };
    let Some(f_tail) = &f_decl.node.body.tail_expr else {
        panic!("expected tail expression for f");
    };
    let Expr::If {
        else_branch: Some(else_branch),
        ..
    } = &f_tail.node
    else {
        panic!("expected expression if with explicit else");
    };
    let Expr::Block(else_block) = &else_branch.node else {
        panic!("expected expression if else branch to be block");
    };
    assert!(else_block.statements.is_empty());
    assert!(matches!(
        else_block.tail_expr,
        Some(ref tail) if matches!(tail.node, Expr::IntegerLiteral(_))
    ));

    let Item::Function(g_decl) = &desugared[0].ast.items[1].node else {
        panic!("expected second function");
    };
    let Stmt::If(if_stmt) = &g_decl.node.body.statements[0].node else {
        panic!("expected if statement");
    };
    let Some(IfStmtElse::Block(else_block)) = &if_stmt.node.else_branch else {
        panic!("expected else-if to normalize to else block");
    };
    assert_eq!(else_block.statements.len(), 1);
    assert!(matches!(else_block.statements[0].node, Stmt::If(_)));
}

#[test]
fn desugar_lowers_init_to_canonical_function_like_form() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "struct S { unsafe init?(_ x: i32) { x; } }",
    );
    let (_, desugared) = expand_and_desugar(&db, &parsed_files);

    let Item::Struct(struct_decl) = &desugared[0].ast.items[0].node else {
        panic!("expected struct item");
    };
    let StructMember::Function(function_decl) =
        &struct_decl.node.members[0].node
    else {
        panic!("expected init to lower into function member");
    };

    assert_eq!(function_decl.node.name, "init");
    assert!(function_decl.node.modifiers.contains(&Modifier::Unsafe));
    assert_eq!(
        function_decl.node.init_origin,
        Some(InitOriginKind::Optional),
    );
    assert!(function_decl.node.attributes.iter().all(|attribute| {
        !attribute.node.name.starts_with("__corex_desugared_init_")
    }));
    assert!(matches!(
        function_decl.node.return_type,
        Some(core_x::frontend::ast::Spanned {
            node: Type::Optional(_),
            ..
        })
    ));
    assert_eq!(function_decl.node.body.statements.len(), 1);
}

#[test]
fn desugar_preserves_provenance_map() {
    let mut db = SourceDb::new();
    let (_, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "macro identity { rule(input: Expr) => { input }; } fn main() { @identity(42); }",
    );
    let (expanded, desugared) = expand_and_desugar(&db, &parsed_files);

    assert_eq!(expanded.len(), 1);
    assert_eq!(desugared.len(), 1);
    assert!(
        expanded[0].provenance_map.len() > 0,
        "expected expansion to record provenance entries"
    );
    assert_eq!(desugared[0].provenance_map, expanded[0].provenance_map);
}

#[test]
fn integration_pipeline_parse_expand_desugar_resolve_runs() {
    let mut db = SourceDb::new();
    let (root_file_id, parsed_files) = parse_single_file(
        &mut db,
        "src/root.cx",
        "struct Boxed { init(_ value: i32) { value; } fn method() -> i32 { 1 } }",
    );
    let (_, desugared) = expand_and_desugar(&db, &parsed_files);

    let graph = resolve_project_scopes(
        &db,
        &desugared,
        root_file_id,
        ResolvedScopeKind::Root,
    )
    .expect("scope resolution should succeed");
    let item_table = GlobalItemTable::collect(&graph, &desugared);
    let (_, imports) =
        resolve_project_imports(&graph, &desugared).expect("imports");
    let declarations =
        resolve_declaration_types(&graph, &desugared, &imports, &item_table);
    let bodies = resolve_bodies(
        &graph,
        &desugared,
        &imports,
        &item_table,
        &declarations,
    );

    let boxed_id = item_table
        .item_id_by_full_path(&["Boxed".to_string()])
        .expect("missing Boxed item id");
    let Some(ResolvedDeclaration::Struct(struct_decl)) =
        declarations.get(boxed_id)
    else {
        panic!("expected resolved struct declaration for Boxed");
    };
    assert_eq!(struct_decl.initializers.len(), 1);
    assert_eq!(struct_decl.methods.len(), 1);

    let owner = DeclarationOwner::Item(boxed_id);
    let owner_bodies = bodies.bodies_for_owner(&owner);
    assert_eq!(owner_bodies.len(), 2);
    assert!(
        owner_bodies
            .iter()
            .any(|body| body.kind == BodyKind::Initializer)
    );
    assert!(
        owner_bodies
            .iter()
            .any(|body| body.kind == BodyKind::Function)
    );
}
