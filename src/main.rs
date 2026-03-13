use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, ColorChoice, Parser, Subcommand, ValueEnum};
use core_x::foreign::{
    BindgenCliArgs, bindgen_success_message, run_bindgen_from_args,
};
use core_x::frontend::DiagnosticRenderer;
use core_x::frontend::ParsedFile;
use core_x::frontend::lexer::Lexer;
use core_x::frontend::parser::parse_source_file_from_source_file_with_recovery;
use core_x::frontend::resolver::{
    ResolvedScopeKind,
    resolve_project_imports_with_named_roots_and_diagnostics,
    resolve_project_scopes,
};
use core_x::frontend::source::{FileId, SourceDb};
use core_x::frontend::{
    ImportRootKind, NamedImportRoot, ProjectGraph, ProjectLoader,
    ProjectManifest, TargetRoots, build_target_roots,
    load_local_dependency_project_graph,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type DynError = Box<dyn Error>;

#[derive(Parser)]
#[command(name = "cxc")]
#[command(color = ColorChoice::Auto)]
#[command(styles = cli_styles())]
#[command(
    about = "coreX compiler and build driver",
    long_about = "cxc is the coreX compiler entrypoint and build driver. It combines compile-facing operations with deterministic frontend introspection across lexing, parsing, scope resolution, import resolution, and foreign-interface generation."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate coreX foreign-interface bindings from C headers.
    #[command(
        about = "Generate `.cx` foreign declarations and `corex.foreign.toml` from C headers",
        long_about = "Runs the Clang-backed bindgen pipeline, emits source-level foreign declarations, and materializes target-path mappings in manifest form for runtime loading."
    )]
    Bindgen(BindgenCliArgs),
    /// Dump deterministic frontend pipeline snapshots.
    #[command(
        about = "Dump frontend pipeline artifacts (lexer/parser/resolver)",
        long_about = "Produces deterministic frontend snapshots for a single file or project. This command hooks directly into concrete pipeline boundaries: lexer output (`tokens`), parser output (`ast`), parsed envelope output (`parsed`), scope-resolution output (`scopes`), and import-resolution output (`imports`).",
        after_long_help = "Dump kinds:\n  tokens   Emit the full token stream from lexer output.\n  ast      Emit recursive AST structure from parser output.\n  parsed   Emit ParsedFile envelope (file id, AST, diagnostics).\n  scopes   Emit resolved project scope graph rooted at src/root.cx or src/main.cx.\n  imports  Emit resolved imports and collected scope symbols over a scope graph."
    )]
    Dump(DumpArgs),
}

#[derive(Args)]
struct DumpArgs {
    /// Dump kind to emit from the frontend pipeline.
    kind: DumpKind,
    /// Single source file path (mutually exclusive with `--project`).
    path: Option<PathBuf>,
    /// Project directory root (mutually exclusive with `<path>`).
    #[arg(long)]
    project: Option<PathBuf>,
    /// Output format for emitted dump payload.
    #[arg(long, value_enum, default_value_t = DumpFormat::Text)]
    format: DumpFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DumpKind {
    Tokens,
    Ast,
    Scopes,
    Imports,
    Parsed,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DumpFormat {
    Text,
    Json,
}

enum DumpInput {
    File(PathBuf),
    Project(PathBuf),
}

struct ProjectContext {
    db: SourceDb,
    parsed_files: Vec<ParsedFile>,
    ordered_file_ids: Vec<FileId>,
    path_by_file_id: BTreeMap<FileId, PathBuf>,
    library_target: Option<TargetSelection>,
    binary_targets: Vec<TargetSelection>,
    current_library_import_root: Option<String>,
    dependency_named_roots: BTreeMap<String, NamedImportRoot>,
}

#[derive(Clone)]
struct TargetSelection {
    kind: ResolvedScopeKind,
    label: &'static str,
    root_file_id: FileId,
}

#[derive(Debug)]
struct TokenView {
    kind: String,
    start: usize,
    end: usize,
    text: String,
}

#[derive(Debug)]
struct FileTokenDump {
    file_id: FileId,
    path: String,
    tokens: Vec<TokenView>,
}

#[derive(Debug)]
struct FileAstDump {
    file_id: FileId,
    path: String,
    item_count: usize,
    ast_debug: String,
    diagnostics_count: usize,
    ast_json: Value,
}

#[derive(Debug)]
struct FileParsedDump {
    file_id: FileId,
    path: String,
    item_count: usize,
    diagnostics_count: usize,
    parsed_debug: String,
    ast_json: Value,
    diagnostics_json: Vec<Value>,
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", ui_error(&format!("error: {error}")));
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<(), DynError> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Bindgen(args) => run_bindgen(args)?,
        Commands::Dump(args) => run_dump(args)?,
    }
    Ok(())
}

fn run_bindgen(args: BindgenCliArgs) -> Result<(), DynError> {
    let output = run_bindgen_from_args(args)?;
    println!(
        "{}",
        bindgen_success_message(&output, ui_stdout_color_enabled())
    );
    Ok(())
}

fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .usage(AnsiColor::BrightBlue.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightCyan.on_default())
        .placeholder(AnsiColor::BrightGreen.on_default())
}

fn force_color_enabled() -> bool {
    std::env::var("CLICOLOR_FORCE")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false)
}

fn no_color_requested() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

fn ui_stdout_color_enabled() -> bool {
    if force_color_enabled() {
        return true;
    }
    if no_color_requested() {
        return false;
    }
    std::io::stdout().is_terminal()
}

fn ui_stderr_color_enabled() -> bool {
    if force_color_enabled() {
        return true;
    }
    if no_color_requested() {
        return false;
    }
    std::io::stderr().is_terminal()
}

fn ui_header(text: &str) -> String {
    if ui_stdout_color_enabled() {
        format!("\x1b[1;34m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn ui_section(text: &str) -> String {
    if ui_stdout_color_enabled() {
        format!("\x1b[1;36m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn ui_error(text: &str) -> String {
    if ui_stderr_color_enabled() {
        format!("\x1b[1;31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn run_dump(args: DumpArgs) -> Result<(), DynError> {
    let input = parse_dump_input(args.path, args.project)?;
    let output = match args.kind {
        DumpKind::Tokens => dump_tokens(input, args.format)?,
        DumpKind::Ast => dump_ast(input, args.format)?,
        DumpKind::Parsed => dump_parsed(input, args.format)?,
        DumpKind::Scopes => dump_scopes(input, args.format)?,
        DumpKind::Imports => dump_imports(input, args.format)?,
    };

    println!("{output}");
    Ok(())
}

fn parse_dump_input(
    path: Option<PathBuf>,
    project: Option<PathBuf>,
) -> Result<DumpInput, DynError> {
    match (path, project) {
        (Some(_), Some(_)) => {
            Err("provide either <path> or --project <dir>, not both".into())
        }
        (None, None) => Err("provide either <path> or --project <dir>".into()),
        (Some(path), None) => Ok(DumpInput::File(path)),
        (None, Some(project)) => Ok(DumpInput::Project(project)),
    }
}

fn dump_tokens(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            (vec![dump_tokens_for_file_path(&path)?], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let mut files = Vec::new();
            for file_id in &context.ordered_file_ids {
                let source_file =
                    context.db.file(*file_id).ok_or_else(|| {
                        format!("missing source file id {}", file_id.raw())
                    })?;
                files.push(FileTokenDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    tokens: lex_token_views(source_file.source())?,
                });
            }
            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_tokens_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "tokens",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "tokens": file.tokens.iter().map(|token| {
                        json!({
                            "kind": token.kind,
                            "start": token.start,
                            "end": token.end,
                            "text": token.text,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_ast(input: DumpInput, format: DumpFormat) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            let (db, parsed, file_id) = parse_single_file(&path)?;
            emit_file_diagnostics(&db, &parsed);
            let source_file = db.file(file_id).ok_or_else(|| {
                format!("missing source file id {}", file_id.raw())
            })?;
            (
                vec![FileAstDump {
                    file_id,
                    path: source_file.path().display().to_string(),
                    item_count: parsed.ast.items.len(),
                    ast_debug: format!("{:#?}", parsed.ast),
                    diagnostics_count: parsed.diagnostics.len(),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| format!("failed to encode AST JSON: {error}"),
                    )?,
                }],
                "file",
            )
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let parsed_by_id = parsed_by_id(&context.parsed_files);
            let mut files = Vec::new();

            for file_id in &context.ordered_file_ids {
                let parsed = parsed_by_id.get(file_id).ok_or_else(|| {
                    format!("missing parsed file id {}", file_id.raw())
                })?;
                files.push(FileAstDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    item_count: parsed.ast.items.len(),
                    ast_debug: format!("{:#?}", parsed.ast),
                    diagnostics_count: parsed.diagnostics.len(),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| format!("failed to encode AST JSON: {error}"),
                    )?,
                });
            }

            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_ast_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "ast",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "item_count": file.item_count,
                    "diagnostics_count": file.diagnostics_count,
                    "ast": file.ast_json,
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_parsed(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (files, mode) = match input {
        DumpInput::File(path) => {
            let (db, parsed, file_id) = parse_single_file(&path)?;
            emit_file_diagnostics(&db, &parsed);
            let source_file = db.file(file_id).ok_or_else(|| {
                format!("missing source file id {}", file_id.raw())
            })?;
            (
                vec![FileParsedDump {
                    file_id,
                    path: source_file.path().display().to_string(),
                    item_count: parsed.ast.items.len(),
                    diagnostics_count: parsed.diagnostics.len(),
                    parsed_debug: format!("{:#?}", parsed),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| {
                            format!("failed to encode parsed AST JSON: {error}")
                        },
                    )?,
                    diagnostics_json: diagnostics_to_json(&parsed.diagnostics),
                }],
                "file",
            )
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let parsed_by_id = parsed_by_id(&context.parsed_files);
            let mut files = Vec::new();

            for file_id in &context.ordered_file_ids {
                let parsed = parsed_by_id.get(file_id).ok_or_else(|| {
                    format!("missing parsed file id {}", file_id.raw())
                })?;
                files.push(FileParsedDump {
                    file_id: *file_id,
                    path: path_for_file_id(&context, *file_id),
                    item_count: parsed.ast.items.len(),
                    diagnostics_count: parsed.diagnostics.len(),
                    parsed_debug: format!("{:#?}", parsed),
                    ast_json: serde_json::to_value(&parsed.ast).map_err(
                        |error| {
                            format!("failed to encode parsed AST JSON: {error}")
                        },
                    )?,
                    diagnostics_json: diagnostics_to_json(&parsed.diagnostics),
                });
            }

            (files, "project")
        }
    };

    match format {
        DumpFormat::Text => Ok(format_parsed_text(&files)),
        DumpFormat::Json => Ok(serde_json::to_string_pretty(&json!({
            "kind": "parsed",
            "mode": mode,
            "files": files.iter().map(|file| {
                json!({
                    "file_id": file.file_id.raw(),
                    "path": file.path,
                    "item_count": file.item_count,
                    "diagnostics_count": file.diagnostics_count,
                    "parsed": {
                        "file_id": file.file_id.raw(),
                        "ast": file.ast_json,
                        "diagnostics": file.diagnostics_json,
                    },
                })
            }).collect::<Vec<_>>(),
        }))?),
    }
}

fn dump_scopes(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (context, targets, mode) = match input {
        DumpInput::File(path) => {
            let (project_dir, root_kind) = classify_single_root_target(&path)?;
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let target = single_target_from_context(&context, root_kind)?;
            (context, vec![target], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let targets = targets_from_context(&context)?;
            (context, targets, "project")
        }
    };

    let mut resolved = Vec::new();
    let resolver = core_x::frontend::ScopeResolver::new(
        &context.db,
        &context.parsed_files,
    );
    for target in targets {
        let (graph, scope_diagnostics) =
            resolve_target_scope_graph_with_diagnostics(
                &resolver,
                &context.db,
                &context.parsed_files,
                target.root_file_id,
                target.kind,
            );
        emit_diagnostics_bag(&context.db, &scope_diagnostics);
        let graph = graph.ok_or_else(|| {
            format!(
                "failed to build {} scope graph for {}",
                target.label,
                path_for_file_id(&context, target.root_file_id)
            )
        })?;
        resolved.push((target, graph));
    }

    match format {
        DumpFormat::Text => Ok(format_scopes_text(&context, &resolved)),
        DumpFormat::Json => {
            let targets_json = resolved
                .iter()
                .map(|(target, graph)| {
                    let root_path =
                        path_for_file_id(&context, target.root_file_id);

                    json!({
                        "target_kind": target.label,
                        "root_file_id": target.root_file_id.raw(),
                        "root_path": root_path,
                        "scope_graph_debug": format!("{:#?}", graph),
                    })
                })
                .collect::<Vec<_>>();

            Ok(serde_json::to_string_pretty(&json!({
                "kind": "scopes",
                "mode": mode,
                "targets": targets_json,
            }))?)
        }
    }
}

fn dump_imports(
    input: DumpInput,
    format: DumpFormat,
) -> Result<String, DynError> {
    let (context, targets, mode) = match input {
        DumpInput::File(path) => {
            let (project_dir, root_kind) = classify_single_root_target(&path)?;
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let target = single_target_from_context(&context, root_kind)?;
            (context, vec![target], "file")
        }
        DumpInput::Project(project_dir) => {
            let context = load_project_context(&project_dir)?;
            emit_context_diagnostics(&context);
            let targets = targets_from_context(&context)?;
            (context, targets, "project")
        }
    };

    let mut resolved = Vec::new();
    let resolver = core_x::frontend::ScopeResolver::new(
        &context.db,
        &context.parsed_files,
    );
    for target in targets {
        let (graph, scope_diagnostics) =
            resolve_target_scope_graph_with_diagnostics(
                &resolver,
                &context.db,
                &context.parsed_files,
                target.root_file_id,
                target.kind,
            );
        emit_diagnostics_bag(&context.db, &scope_diagnostics);
        let graph = graph.ok_or_else(|| {
            format!(
                "failed to build {} scope graph for {}",
                target.label,
                path_for_file_id(&context, target.root_file_id)
            )
        })?;
        let mut named_roots = context.dependency_named_roots.clone();
        if target.kind == ResolvedScopeKind::BinaryRoot {
            if let (Some(root_name), Some(library_target)) = (
                context.current_library_import_root.as_ref(),
                context.library_target.as_ref(),
            ) {
                let (library_graph, library_diagnostics) =
                    resolve_target_scope_graph_with_diagnostics(
                        &resolver,
                        &context.db,
                        &context.parsed_files,
                        library_target.root_file_id,
                        ResolvedScopeKind::Root,
                    );
                emit_diagnostics_bag(&context.db, &library_diagnostics);
                let library_graph = library_graph.ok_or_else(|| {
                    format!(
                        "failed to build library scope graph for {}",
                        path_for_file_id(&context, library_target.root_file_id)
                    )
                })?;
                named_roots.insert(
                    root_name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph: library_graph,
                        parsed_files: context.parsed_files.clone(),
                    },
                );
            }
        }

        let (symbols, imports, import_diagnostics) =
            resolve_project_imports_with_named_roots_and_diagnostics(
                &graph,
                &context.parsed_files,
                &named_roots,
                &context.db,
            );
        emit_diagnostics_bag(&context.db, &import_diagnostics);
        resolved.push((target, graph, symbols, imports));
    }

    match format {
        DumpFormat::Text => Ok(format_imports_text(&context, &resolved)),
        DumpFormat::Json => {
            let targets_json = resolved
                .iter()
                .map(|(target, graph, symbols, imports)| {
                    let root_path =
                        path_for_file_id(&context, target.root_file_id);

                    json!({
                        "target_kind": target.label,
                        "root_file_id": target.root_file_id.raw(),
                        "root_path": root_path,
                        "scope_graph_debug": format!("{:#?}", graph),
                        "scope_symbols_debug": format!("{:#?}", symbols),
                        "resolved_imports_debug": format!("{:#?}", imports),
                    })
                })
                .collect::<Vec<_>>();

            Ok(serde_json::to_string_pretty(&json!({
                "kind": "imports",
                "mode": mode,
                "targets": targets_json,
            }))?)
        }
    }
}

fn resolve_target_scope_graph_with_diagnostics(
    resolver: &core_x::frontend::ScopeResolver<'_>,
    db: &SourceDb,
    parsed_files: &[ParsedFile],
    root_file_id: FileId,
    kind: ResolvedScopeKind,
) -> (
    Option<core_x::frontend::ScopeGraph>,
    core_x::frontend::DiagnosticsBag,
) {
    match kind {
        ResolvedScopeKind::Root => {
            resolver.resolve_library_root_with_diagnostics(root_file_id, db)
        }
        ResolvedScopeKind::BinaryRoot => {
            resolver.resolve_binary_root_with_diagnostics(root_file_id, db)
        }
        other => {
            let mut diagnostics = core_x::frontend::DiagnosticsBag::new();
            match resolve_project_scopes(db, parsed_files, root_file_id, other)
            {
                Ok(graph) => (Some(graph), diagnostics),
                Err(error) => {
                    diagnostics.push(
                        core_x::frontend::diagnostic_from_resolve_error(
                            db, &error,
                        ),
                    );
                    (None, diagnostics)
                }
            }
        }
    }
}

fn emit_diagnostics_bag(db: &SourceDb, bag: &core_x::frontend::DiagnosticsBag) {
    if bag.is_empty() {
        return;
    }
    let renderer = DiagnosticRenderer::new(db);
    let rendered = renderer.render_all(bag.as_slice());
    if !rendered.is_empty() {
        eprintln!("{rendered}");
    }
}

fn dump_tokens_for_file_path(path: &Path) -> Result<FileTokenDump, DynError> {
    let (db, parsed, file_id) = parse_single_file(path)?;
    emit_file_diagnostics(&db, &parsed);
    let source_file = db
        .file(file_id)
        .ok_or_else(|| format!("missing source file id {}", file_id.raw()))?;
    let tokens = lex_token_views(source_file.source())?;
    Ok(FileTokenDump {
        file_id,
        path: path.display().to_string(),
        tokens,
    })
}

fn parse_single_file(
    path: &Path,
) -> Result<(SourceDb, ParsedFile, FileId), DynError> {
    let source = fs::read_to_string(path)?;
    let mut db = SourceDb::new();
    let file_id = db.add_file(path.to_path_buf(), source);
    let file = db
        .file(file_id)
        .ok_or_else(|| format!("missing source file id {}", file_id.raw()))?;
    let parsed = parse_source_file_from_source_file_with_recovery(file)
        .map_err(|error| {
            format!(
                "failed to initialize parser for {}: {error}",
                path.display()
            )
        })?;
    Ok((db, parsed, file_id))
}

fn load_project_context(
    project_dir: &Path,
) -> Result<ProjectContext, DynError> {
    let loaded_project = ProjectLoader::load_project(project_dir)?;
    let project_graph =
        load_local_dependency_project_graph(loaded_project.clone())?;
    let target_roots = build_target_roots(&project_graph)?;
    let current_library_import_root =
        target_roots.by_name.iter().find_map(|(name, root)| {
            (root.kind == ImportRootKind::CurrentLibrary).then(|| name.clone())
        });

    let manifest = loaded_project.manifest.clone();
    let project_root = loaded_project.project_dir.clone();
    let project_files = collect_project_cx_files(&project_root, &manifest)?;

    let mut db = SourceDb::new();
    let mut parsed_files = Vec::with_capacity(project_files.len());
    let mut ordered_file_ids = Vec::with_capacity(project_files.len());
    let mut path_by_file_id = BTreeMap::new();
    let mut file_id_by_path = BTreeMap::new();

    for absolute_path in project_files {
        let display_path =
            project_relative_or_absolute_path(&project_root, &absolute_path);
        let source = fs::read_to_string(&absolute_path)?;
        let file_id = db.add_file(display_path.clone(), source);
        let file = db.file(file_id).ok_or_else(|| {
            format!("missing source file id {}", file_id.raw())
        })?;
        let parsed = parse_source_file_from_source_file_with_recovery(file)
            .map_err(|error| {
                format!(
                    "failed to initialize parser for project file {}: {error}",
                    display_path.display()
                )
            })?;

        ordered_file_ids.push(file_id);
        path_by_file_id.insert(file_id, display_path);
        file_id_by_path.insert(absolute_path, file_id);
        parsed_files.push(parsed);
    }

    let library_target = if let Some(target) = manifest.library.as_ref() {
        let file_id =
            file_id_by_path.get(&target.root_file).ok_or_else(|| {
                format!(
                    "missing target root in parsed project files: {}",
                    target.root_file.display()
                )
            })?;
        Some(TargetSelection {
            kind: ResolvedScopeKind::Root,
            label: "library",
            root_file_id: *file_id,
        })
    } else {
        None
    };

    let mut binary_targets = Vec::with_capacity(manifest.binaries.len());
    for target in &manifest.binaries {
        let file_id =
            file_id_by_path.get(&target.root_file).ok_or_else(|| {
                format!(
                    "missing target root in parsed project files: {}",
                    target.root_file.display()
                )
            })?;
        binary_targets.push(TargetSelection {
            kind: ResolvedScopeKind::BinaryRoot,
            label: "binary",
            root_file_id: *file_id,
        });
    }

    let dependency_named_roots =
        build_dependency_named_roots(&project_graph, &target_roots)?;

    Ok(ProjectContext {
        db,
        parsed_files,
        ordered_file_ids,
        path_by_file_id,
        library_target,
        binary_targets,
        current_library_import_root,
        dependency_named_roots,
    })
}

fn collect_project_cx_files(
    project_dir: &Path,
    manifest: &ProjectManifest,
) -> Result<Vec<PathBuf>, DynError> {
    let mut files = BTreeSet::new();
    let src_dir = project_dir.join("src");
    if src_dir.is_dir() {
        collect_cx_files_recursive(&src_dir, &mut files)?;
    }

    if let Some(library_target) = &manifest.library {
        files.insert(library_target.root_file.clone());
    }
    for binary_target in &manifest.binaries {
        files.insert(binary_target.root_file.clone());
    }

    Ok(files.into_iter().collect())
}

fn collect_cx_files_recursive(
    dir: &Path,
    out: &mut BTreeSet<PathBuf>,
) -> Result<(), DynError> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_cx_files_recursive(&path, out)?;
            continue;
        }

        if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("cx")
        {
            out.insert(path);
        }
    }

    Ok(())
}

fn project_relative_or_absolute_path(
    project_dir: &Path,
    absolute_path: &Path,
) -> PathBuf {
    absolute_path
        .strip_prefix(project_dir)
        .map_or_else(|_| absolute_path.to_path_buf(), Path::to_path_buf)
}

fn build_dependency_named_roots(
    project_graph: &ProjectGraph,
    target_roots: &TargetRoots,
) -> Result<BTreeMap<String, NamedImportRoot>, DynError> {
    let mut named_roots = BTreeMap::new();

    for (name, root) in &target_roots.by_name {
        match root.kind {
            ImportRootKind::CurrentLibrary => {}
            ImportRootKind::UnloadedGitDependency => {
                named_roots
                    .insert(name.clone(), NamedImportRoot::UnloadedDependency);
            }
            ImportRootKind::LocalDependencyLibrary => {
                let dependency = project_graph
                    .local_dependencies
                    .iter()
                    .find(|dependency| dependency.dependency_name == *name)
                    .ok_or_else(|| {
                        format!(
                            "missing loaded local dependency project for root `{name}`"
                        )
                    })?;
                let library_target =
                    dependency.project.manifest.library.as_ref().ok_or_else(
                        || {
                            format!(
                                "dependency `{}` has no library target",
                                dependency.dependency_name
                            )
                        },
                    )?;
                let (db, parsed_files, file_id_by_path) =
                    parse_loaded_project_files(&dependency.project)?;
                let library_root_file_id = file_id_by_path
                    .get(&library_target.root_file)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "missing dependency library root file {}",
                            library_target.root_file.display()
                        )
                    })?;
                let graph = resolve_project_scopes(
                    &db,
                    &parsed_files,
                    library_root_file_id,
                    ResolvedScopeKind::Root,
                )?;
                named_roots.insert(
                    name.clone(),
                    NamedImportRoot::LoadedLibrary {
                        graph,
                        parsed_files,
                    },
                );
            }
        }
    }

    Ok(named_roots)
}

fn parse_loaded_project_files(
    project: &core_x::frontend::LoadedProject,
) -> Result<(SourceDb, Vec<ParsedFile>, BTreeMap<PathBuf, FileId>), DynError> {
    let project_files =
        collect_project_cx_files(&project.project_dir, &project.manifest)?;
    let mut db = SourceDb::new();
    let mut parsed_files = Vec::with_capacity(project_files.len());
    let mut file_id_by_path = BTreeMap::new();

    for absolute_path in project_files {
        let display_path = project_relative_or_absolute_path(
            &project.project_dir,
            &absolute_path,
        );
        let source = fs::read_to_string(&absolute_path)?;
        let file_id = db.add_file(display_path, source);
        let file = db.file(file_id).ok_or_else(|| {
            format!("missing dependency source file id {}", file_id.raw())
        })?;
        let parsed = parse_source_file_from_source_file_with_recovery(file)
            .map_err(|error| {
                format!(
                    "failed to initialize parser for dependency file {}: {error}",
                    absolute_path.display()
                )
            })?;

        parsed_files.push(parsed);
        file_id_by_path.insert(absolute_path, file_id);
    }

    Ok((db, parsed_files, file_id_by_path))
}

fn classify_single_root_target(
    path: &Path,
) -> Result<(PathBuf, ResolvedScopeKind), DynError> {
    let canonical = fs::canonicalize(path)?;
    let file_name = canonical.file_name().and_then(|name| name.to_str());
    let root_kind = match file_name {
        Some("root.cx") => ResolvedScopeKind::Root,
        Some("main.cx") => ResolvedScopeKind::BinaryRoot,
        _ => {
            return Err(
                "single-file mode only supports src/root.cx or src/main.cx"
                    .into(),
            );
        }
    };

    let src_dir = canonical
        .parent()
        .ok_or("single-file mode path is missing parent directory")?;
    if src_dir.file_name().and_then(|name| name.to_str()) != Some("src") {
        return Err(
            "single-file mode only supports src/root.cx or src/main.cx".into(),
        );
    }

    let project_dir = src_dir
        .parent()
        .ok_or("single-file mode path is missing project root directory")?;
    Ok((project_dir.to_path_buf(), root_kind))
}

fn single_target_from_context(
    context: &ProjectContext,
    root_kind: ResolvedScopeKind,
) -> Result<TargetSelection, DynError> {
    match root_kind {
        ResolvedScopeKind::Root => context
            .library_target
            .clone()
            .ok_or_else(|| "project does not declare a library target".into()),
        ResolvedScopeKind::BinaryRoot => context
            .binary_targets
            .iter()
            .find(|target| {
                context
                    .path_by_file_id
                    .get(&target.root_file_id)
                    .is_some_and(|path| path == Path::new("src/main.cx"))
            })
            .cloned()
            .ok_or_else(|| {
                "project does not declare binary target rooted at src/main.cx"
                    .into()
            }),
        _ => Err("single target root must be library or binary".into()),
    }
}

fn targets_from_context(
    context: &ProjectContext,
) -> Result<Vec<TargetSelection>, DynError> {
    let mut targets = Vec::new();
    if let Some(library_target) = &context.library_target {
        targets.push(library_target.clone());
    }
    targets.extend(context.binary_targets.iter().cloned());

    if targets.is_empty() {
        return Err(
            "project manifest does not define any compilation targets".into()
        );
    }

    Ok(targets)
}

fn parsed_by_id(parsed_files: &[ParsedFile]) -> BTreeMap<FileId, &ParsedFile> {
    parsed_files
        .iter()
        .map(|parsed| (parsed.file_id, parsed))
        .collect()
}

fn path_for_file_id(context: &ProjectContext, file_id: FileId) -> String {
    context
        .path_by_file_id
        .get(&file_id)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| format!("<unknown:{}>", file_id.raw()))
}

fn lex_token_views(source: &str) -> Result<Vec<TokenView>, DynError> {
    let tokens = Lexer::new(source)
        .lex_all()
        .map_err(|error| format!("failed to lex source: {error}"))?;

    let mut rows = Vec::with_capacity(tokens.len());
    for token in tokens {
        let text = source
            .get(token.span.start..token.span.end)
            .unwrap_or("")
            .to_string();
        rows.push(TokenView {
            kind: format!("{:?}", token.kind),
            start: token.span.start,
            end: token.span.end,
            text,
        });
    }
    Ok(rows)
}

fn format_tokens_text(files: &[FileTokenDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            ui_header(&format!("== file: {} ==", file.path))
        ));
        for token in &file.tokens {
            out.push_str(&format!(
                "{} {}..{} {:?}\n",
                token.kind, token.start, token.end, token.text
            ));
        }
    }
    out.trim_end_matches('\n').to_string()
}

fn format_ast_text(files: &[FileAstDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            ui_header(&format!("== file: {} ==", file.path))
        ));
        out.push_str(&format!(
            "{} {}\n",
            ui_section("file_id:"),
            file.file_id.raw()
        ));
        out.push_str(&format!(
            "{} {}\n",
            ui_section("item_count:"),
            file.item_count
        ));
        out.push_str(&file.ast_debug);
        if !file.ast_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

fn format_parsed_text(files: &[FileParsedDump]) -> String {
    let mut out = String::new();
    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            ui_header(&format!("== file: {} ==", file.path))
        ));
        out.push_str(&format!(
            "{} {}\n",
            ui_section("file_id:"),
            file.file_id.raw()
        ));
        out.push_str(&format!(
            "{} {}\n",
            ui_section("item_count:"),
            file.item_count
        ));
        out.push_str(&format!(
            "{} {}\n",
            ui_section("diagnostics_count:"),
            file.diagnostics_count
        ));
        out.push_str(&file.parsed_debug);
        if !file.parsed_debug.ends_with('\n') {
            out.push('\n');
        }
    }
    out.trim_end_matches('\n').to_string()
}

fn format_scopes_text(
    context: &ProjectContext,
    resolved: &[(TargetSelection, core_x::frontend::ScopeGraph)],
) -> String {
    let mut out = String::new();
    for (index, (target, graph)) in resolved.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            ui_header(&format!(
                "== target: {} ({}) ==",
                target.label,
                path_for_file_id(context, target.root_file_id)
            ))
        ));
        out.push_str(&format!("{:#?}\n", graph));
    }
    out.trim_end_matches('\n').to_string()
}

fn format_imports_text(
    context: &ProjectContext,
    resolved: &[(
        TargetSelection,
        core_x::frontend::ScopeGraph,
        BTreeMap<FileId, core_x::frontend::ScopeSymbols>,
        BTreeMap<FileId, core_x::frontend::ResolvedImports>,
    )],
) -> String {
    let mut out = String::new();
    for (index, (target, graph, symbols, imports)) in
        resolved.iter().enumerate()
    {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n",
            ui_header(&format!(
                "== target: {} ({}) ==",
                target.label,
                path_for_file_id(context, target.root_file_id)
            ))
        ));
        out.push_str(&format!("{}\n", ui_section("scope_graph:")));
        out.push_str(&format!("{:#?}\n", graph));
        out.push_str(&format!("{}\n", ui_section("scope_symbols:")));
        out.push_str(&format!("{:#?}\n", symbols));
        out.push_str(&format!("{}\n", ui_section("resolved_imports:")));
        out.push_str(&format!("{:#?}\n", imports));
    }
    out.trim_end_matches('\n').to_string()
}

fn diagnostics_to_json(
    diagnostics: &core_x::frontend::DiagnosticsBag,
) -> Vec<Value> {
    diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| {
            json!({
                "severity": format!("{:?}", diagnostic.severity),
                "message": diagnostic.message,
                "labels": diagnostic.labels.iter().map(|label| {
                    json!({
                        "kind": format!("{:?}", label.kind),
                        "span": {
                            "file_id": label.span.file_id.raw(),
                            "start": label.span.span.start,
                            "end": label.span.span.end,
                        },
                        "message": label.message,
                    })
                }).collect::<Vec<_>>(),
                "notes": diagnostic.notes,
                "help": diagnostic.help,
            })
        })
        .collect()
}

fn emit_file_diagnostics(db: &SourceDb, parsed: &ParsedFile) {
    if parsed.diagnostics.is_empty() {
        return;
    }
    let renderer = DiagnosticRenderer::new(db);
    let rendered = renderer.render_all(parsed.diagnostics.as_slice());
    if !rendered.is_empty() {
        eprintln!("{rendered}");
    }
}

fn emit_context_diagnostics(context: &ProjectContext) {
    let parsed_by_id = parsed_by_id(&context.parsed_files);
    let renderer = DiagnosticRenderer::new(&context.db);
    let mut rendered_per_file = Vec::new();

    for file_id in &context.ordered_file_ids {
        let Some(parsed) = parsed_by_id.get(file_id) else {
            continue;
        };
        if parsed.diagnostics.is_empty() {
            continue;
        }
        let rendered = renderer.render_all(parsed.diagnostics.as_slice());
        if !rendered.is_empty() {
            rendered_per_file.push(rendered);
        }
    }

    if !rendered_per_file.is_empty() {
        eprintln!("{}", rendered_per_file.join("\n\n"));
    }
}
