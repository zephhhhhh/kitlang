use core::panic;

use kitlang::{
    ast::IdentPath,
    intermediate::{
        hir::{
            HLIR, HirId,
            nodes::{Expr, ExprKind, HirNode, StatementKind},
        },
        resolver::errors::ResolverErrorKind,
    },
    prelude::ProgramMetaData,
};

mod common;
use common::*;

// Helper getters matching the style in HIR tests..

fn get_hir_node(hlir: &HLIR, hir_id: HirId) -> &HirNode {
    hlir.get_hir_node(hir_id).expect("Expected node to exist.")
}
fn get_first_statement<'a>(hlir: &'a HLIR, meta: &'a ProgramMetaData) -> &'a HirNode {
    let main_fn = get_main_function(hlir, meta);
    let func = main_fn.hir_function_ref().expect("Should be a function");
    let body = func.body.as_ref().expect("Function should have a body");
    let block = hlir.get_hir_node(body.block).expect("Block should exist");

    expect_match!(block, HirNode::Block(block) => {
		let first_stmt_id = block.statements[0];
		get_hir_node(hlir, first_stmt_id)
	}, "Expected block HIR node")
}
fn get_nth_statement<'a>(hlir: &'a HLIR, meta: &'a ProgramMetaData, index: usize) -> &'a HirNode {
    let main_fn = get_main_function(hlir, meta);
    let func = main_fn.hir_function_ref().expect("Should be a function");
    let body = func.body.as_ref().expect("Function should have a body");
    let block = hlir.get_hir_node(body.block).expect("Block should exist");

    expect_match!(block, HirNode::Block(block) => {
		let stmt_id = block.statements[index];
		get_hir_node(hlir, stmt_id)
	}, "Expected block HIR node")
}
fn get_expr(hlir: &HLIR, hir_id: HirId) -> &Expr {
    let node = get_hir_node(hlir, hir_id);
    expect_match!(node, HirNode::Expr(expr) => expr, "Expected expression HIR node")
}
fn expect_any_expr_stmt<'a>(hlir: &'a HLIR, node: &'a HirNode) -> &'a Expr {
    expect_match!(node, HirNode::Statement(stmt) => {
		match &stmt.kind {
			StatementKind::Semi(expr_id) | StatementKind::Expr(expr_id) => {
				get_expr(hlir, *expr_id)
			},
			_ => panic!("Statement is not an expression or semi-expression statement"),
		}
	}, "Expected statement HIR node")
}
fn get_first_expr<'a>(hlir: &'a HLIR, meta: &'a ProgramMetaData) -> &'a Expr {
    let stmt = get_first_statement(hlir, meta);
    expect_any_expr_stmt(hlir, stmt)
}

macro_rules! wrap_and_parse_expr {
    ($test_src:literal) => {{
        let src = main!($test_src);
        parse_lower_hir_full_ok(&src)
    }};
}

// Parsing + resolution helpers..

// The circle turns; now we test resolving.

#[test]
fn resolve_local_variable_path() {
    let (meta, hlir) = wrap_and_parse_expr!("let x = 1; x;");

    // Binding ID from the let statement.
    let let_stmt_node = get_nth_statement(&hlir, &meta, 0);
    let binding_id = expect_match!(let_stmt_node, HirNode::Statement(stmt) => {
		expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
			let_stmt.binding
		}, "Expected let statement")
	}, "Expected statement node");

    // Path expression `x` should resolve to the binding's HirId.
    let path_expr = expect_any_expr_stmt(&hlir, get_nth_statement(&hlir, &meta, 1));
    expect_match!(&path_expr.kind, ExprKind::Path(ref_path) => {
		assert!(ref_path.is_resolved(), "Path should be resolved");
		let resolved = ref_path.resolved_id().expect("Must have resolved id");
		assert_eq!(resolved, binding_id.into(), "Path should resolve to binding HirId");
	}, "Expected path expression");
}

#[test]
fn resolve_tuple_binding_paths() {
    let (meta, hlir) = wrap_and_parse_expr!("let (a, b) = (1, 2); a; b;");

    // Extract binding IDs for `a` and `b`.
    let (a_id, b_id) = {
        let let_stmt_node = get_nth_statement(&hlir, &meta, 0);
        expect_match!(let_stmt_node, HirNode::Statement(stmt) => {
			expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
				let pattern_node = get_hir_node(&hlir, let_stmt.binding);
				expect_match!(pattern_node, HirNode::Binding(binding) => {
					expect_match!(&binding.kind, kitlang::intermediate::hir::nodes::BindingKind::Tuple(ids) => {
						(ids[0], ids[1])
					}, "Expected tuple binding")
				}, "Expected binding node")
			}, "Expected let statement")
		}, "Expected statement node")
    };

    let a_expr = expect_any_expr_stmt(&hlir, get_nth_statement(&hlir, &meta, 1));
    let b_expr = expect_any_expr_stmt(&hlir, get_nth_statement(&hlir, &meta, 2));

    expect_match!(&a_expr.kind, ExprKind::Path(ref_path) => {
		assert!(ref_path.is_resolved());
		assert_eq!(ref_path.resolved_id().unwrap(), a_id.into());
	}, "Expected path expression for a");

    expect_match!(&b_expr.kind, ExprKind::Path(ref_path) => {
		assert!(ref_path.is_resolved());
		assert_eq!(ref_path.resolved_id().unwrap(), b_id.into());
	}, "Expected path expression for b");
}

#[test]
fn resolve_global_function_call_target() {
    let (meta, hlir) = parse_lower_hir_full_ok("fn main() { test(); } fn test() {}");

    // Expected owner def from namespace.
    let expected_id = meta
        .namespace
        .find_definition(&IdentPath::new("::test"))
        .and_then(|ns| ns.id.owner_def_id())
        .expect("Function should be in namespace");

    let call_expr = get_first_expr(&hlir, &meta).clone();

    // Call target path should resolve to the function owner id.
    expect_match!(&call_expr.kind, ExprKind::Call(target_id, _args) => {
		let target_expr = get_expr(&hlir, *target_id);
		expect_match!(&target_expr.kind, ExprKind::Path(ref_path) => {
			assert!(ref_path.is_resolved());
			let resolved = ref_path.resolved_id().unwrap();
			assert_eq!(resolved, expected_id.into());
		}, "Expected path target in call");
	}, "Expected call expression");
}

#[test]
fn resolve_module_scoped_function_call() {
    let (meta, hlir) = parse_lower_hir_full_ok("fn main() { a::f(); } mod a { pub fn f() { } }");

    let expected_id = meta
        .namespace
        .find_definition(&IdentPath::new("::a::f"))
        .and_then(|ns| ns.id.owner_def_id())
        .expect("Function should be in namespace");

    let call_expr = get_first_expr(&hlir, &meta).clone();
    expect_match!(&call_expr.kind, ExprKind::Call(target_id, _args) => {
		let target_expr = get_expr(&hlir, *target_id);
		expect_match!(&target_expr.kind, ExprKind::Path(ref_path) => {
			assert!(ref_path.is_resolved());
			let resolved = ref_path.resolved_id().unwrap();
			assert_eq!(resolved, expected_id.into());
		}, "Expected path target in call");
	}, "Expected call expression");
}

#[test]
fn resolve_use_import_and_call() {
    let (meta, hlir) =
        parse_lower_hir_full_ok("mod m { pub fn f() {} } use ::m::f; fn main() { f(); }");

    let expected_id = meta
        .namespace
        .find_definition(&IdentPath::new("::m::f"))
        .and_then(|ns| ns.id.owner_def_id())
        .expect("Function should be in namespace");

    let call_expr = get_first_expr(&hlir, &meta).clone();
    expect_match!(&call_expr.kind, ExprKind::Call(target_id, _args) => {
		let target_expr = get_expr(&hlir, *target_id);
		expect_match!(&target_expr.kind, ExprKind::Path(ref_path) => {
			assert!(ref_path.is_resolved());
			let resolved = ref_path.resolved_id().unwrap();
			assert_eq!(resolved, expected_id.into());
		}, "Expected path target in call");
	}, "Expected call expression");
}

#[test]
fn unresolved_private_function_inaccessible() {
    let err = parse_lower_hir_full_err("mod m { fn f() {} } fn main() { m::f(); }");

    expect_match!(err.error_kind, ResolverErrorKind::UnresolvedReferences(refs) => {
		assert!(!refs.references.is_empty(), "Should have unresolved refs");
		let failure = refs.references[0].failure;
		assert_eq!(failure, kitlang::intermediate::resolver::errors::ResolutionFailure::Inaccessible);
	}, "Expected unresolved references due to inaccessible private function");
}

#[test]
fn unresolved_out_of_scope_local_variable() {
    let err = parse_lower_hir_full_err(main!("{ let x = 1; } x;"));

    expect_match!(err.error_kind, ResolverErrorKind::UnresolvedReferences(refs) => {
		assert!(!refs.references.is_empty(), "Should have unresolved refs");
		let failure = refs.references[0].failure;
		assert_eq!(failure, kitlang::intermediate::resolver::errors::ResolutionFailure::NotFound);
	}, "Expected unresolved references due to unknown identifier");
}

// Reuse helper from HIR tests to get main function owning node.
fn get_main_function<'a>(
    hlir: &'a HLIR,
    meta: &'a ProgramMetaData,
) -> &'a kitlang::intermediate::hir::nodes::OwningNode {
    let main_id = meta
        .namespace
        .find_definition(&IdentPath::new("::main"))
        .and_then(|ns| ns.id.owner_def_id())
        .expect("Function should be in namespace");

    hlir.owning_node(main_id).expect("No main function found.")
}
