use core_x::frontend::ast::{Expr, Item, Stmt};
use core_x::frontend::source::FileId;
use core_x::frontend::{DesugaredFile, ExpansionOptions, FrontendContext};
use std::collections::BTreeMap;

fn main_expr(desugared: &DesugaredFile) -> &Expr {
    let function = desugared
        .ast
        .items
        .iter()
        .find_map(|item| match &item.node {
            Item::Function(function_decl)
                if function_decl.node.name == "main" =>
            {
                Some(&function_decl.node)
            }
            _ => None,
        })
        .expect("expected `main` function");
    let statement = function
        .body
        .statements
        .first()
        .expect("expected first statement in `main`");
    let Stmt::Expr { expr, .. } = &statement.node else {
        panic!("expected expression statement");
    };
    &expr.node
}

#[test]
fn pre_resolution_pipeline_resolves_imported_macro_definitions_across_files() {
    let mut context = FrontendContext::new();
    context.add_file("src/root.cx", "scope util {}\nscope consumer {}\n");
    context.add_file(
        "src/util.cx",
        "macro plus_one { rule(input: Expr) => { input + 1 }; }\n",
    );
    let consumer_id = context.add_file(
        "src/consumer.cx",
        "use root::util::plus_one;\nfn main() { @plus_one(41); }\n",
    );

    let files = context
        .pre_resolution_pipeline(&[consumer_id], ExpansionOptions::default())
        .expect("pre-resolution pipeline should succeed");
    let consumer = files.first().expect("consumer file should be present");

    assert!(
        consumer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        consumer.diagnostics.as_slice()
    );
    let Expr::Binary { lhs, rhs, .. } = main_expr(consumer) else {
        panic!("macro should expand to a binary expression");
    };
    assert!(matches!(
        lhs.node,
        Expr::IntegerLiteral(ref value) if value == "41"
    ));
    assert!(matches!(
        rhs.node,
        Expr::IntegerLiteral(ref value) if value == "1"
    ));
}

#[test]
fn glob_imported_macros_expand_without_reparsing() {
    let mut context = FrontendContext::new();
    context.add_file("src/root.cx", "scope util {}\nscope consumer {}\n");
    context.add_file(
        "src/util.cx",
        "macro plus_one { rule(input: Expr) => { input + 1 }; }\n\
         macro plus_two { rule(input: Expr) => { input + 2 }; }\n",
    );
    let consumer_id = context.add_file(
        "src/consumer.cx",
        "use root::util::*;\nfn main() { @plus_two(5); }\n",
    );
    let ordered_ids = context.ordered_file_ids().to_vec();

    let first = context
        .pre_resolution_pipeline(&[consumer_id], ExpansionOptions::default())
        .expect("first pre-resolution pipeline should succeed");
    let first_consumer = first.first().expect("consumer file should exist");
    assert!(
        first_consumer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        first_consumer.diagnostics.as_slice()
    );
    let Expr::Binary { lhs, rhs, .. } = main_expr(first_consumer) else {
        panic!("glob-imported macro should expand to a binary expression");
    };
    assert!(matches!(
        lhs.node,
        Expr::IntegerLiteral(ref value) if value == "5"
    ));
    assert!(matches!(
        rhs.node,
        Expr::IntegerLiteral(ref value) if value == "2"
    ));

    let parsed_ptrs = ordered_ids
        .iter()
        .map(|file_id| {
            (
                *file_id,
                std::ptr::from_ref(
                    context
                        .parsed_file(*file_id)
                        .expect("parsed file should be cached"),
                ),
            )
        })
        .collect::<BTreeMap<FileId, *const core_x::frontend::ParsedFile>>();
    let expanded_ptr = std::ptr::from_ref(
        context
            .expanded_file(consumer_id)
            .expect("expanded consumer should be cached"),
    );
    let desugared_ptr = std::ptr::from_ref(
        context
            .desugared_file_cached(consumer_id)
            .expect("desugared consumer should be cached"),
    );

    context
        .pre_resolution_pipeline(&[consumer_id], ExpansionOptions::default())
        .expect("second pre-resolution pipeline should reuse cache");

    for file_id in ordered_ids {
        assert_eq!(
            parsed_ptrs[&file_id],
            std::ptr::from_ref(
                context
                    .parsed_file(file_id)
                    .expect("parsed file should remain cached"),
            ),
            "parsed file cache should be reused for file {}",
            file_id.raw()
        );
    }
    assert_eq!(
        expanded_ptr,
        std::ptr::from_ref(
            context
                .expanded_file(consumer_id)
                .expect("expanded consumer should remain cached"),
        ),
        "expanded consumer should be reused without re-expansion"
    );
    assert_eq!(
        desugared_ptr,
        std::ptr::from_ref(
            context
                .desugared_file_cached(consumer_id)
                .expect("desugared consumer should remain cached"),
        ),
        "desugared consumer should be reused"
    );
}

#[test]
fn repeated_pipeline_runs_reuse_cached_state_for_prior_analysis_results() {
    let mut context = FrontendContext::new();
    context.add_file("src/root.cx", "scope util {}\nscope a {}\nscope b {}\n");
    context.add_file(
        "src/util.cx",
        "macro plus_one { rule(input: Expr) => { input + 1 }; }\n",
    );
    let consumer_a_id = context.add_file(
        "src/a.cx",
        "use root::util::plus_one;\nfn main() { @plus_one(1); }\n",
    );
    let consumer_b_id = context.add_file(
        "src/b.cx",
        "use root::util::plus_one;\nfn main() { @plus_one(2); }\n",
    );
    let ordered_ids = context.ordered_file_ids().to_vec();

    context
        .pre_resolution_pipeline(&[consumer_a_id], ExpansionOptions::default())
        .expect("first analysis should succeed");

    let parsed_ptrs = ordered_ids
        .iter()
        .map(|file_id| {
            (
                *file_id,
                std::ptr::from_ref(
                    context
                        .parsed_file(*file_id)
                        .expect("parsed file should be cached"),
                ),
            )
        })
        .collect::<BTreeMap<FileId, *const core_x::frontend::ParsedFile>>();
    let expanded_a_ptr = std::ptr::from_ref(
        context
            .expanded_file(consumer_a_id)
            .expect("expanded A should be cached"),
    );
    let desugared_a_ptr = std::ptr::from_ref(
        context
            .desugared_file_cached(consumer_a_id)
            .expect("desugared A should be cached"),
    );

    let second = context
        .pre_resolution_pipeline(
            &[consumer_a_id, consumer_b_id],
            ExpansionOptions::default(),
        )
        .expect("second analysis should succeed");
    assert_eq!(second.len(), 2);

    for file_id in ordered_ids {
        assert_eq!(
            parsed_ptrs[&file_id],
            std::ptr::from_ref(
                context
                    .parsed_file(file_id)
                    .expect("parsed file should remain cached"),
            ),
            "parsed file cache should be reused for file {}",
            file_id.raw()
        );
    }
    assert_eq!(
        expanded_a_ptr,
        std::ptr::from_ref(
            context
                .expanded_file(consumer_a_id)
                .expect("expanded A should remain cached"),
        ),
        "consumer A should not re-expand on repeated analysis"
    );
    assert_eq!(
        desugared_a_ptr,
        std::ptr::from_ref(
            context
                .desugared_file_cached(consumer_a_id)
                .expect("desugared A should remain cached"),
        ),
        "consumer A desugared output should be reused"
    );
    assert!(
        context.expanded_file(consumer_b_id).is_some(),
        "consumer B should be expanded during second analysis"
    );
    assert!(
        context.desugared_file_cached(consumer_b_id).is_some(),
        "consumer B should be desugared during second analysis"
    );
}
