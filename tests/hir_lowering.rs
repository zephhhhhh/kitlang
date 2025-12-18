use kitlang::{
    ast::{BinaryOpKind, Literal, Mutability, Ty, UnaryOpKind, Visibility},
    intermediate::{
        hir::{
            HLIR, HirId, LocalDefId, OwnerDefId,
            nodes::{Expr, ExprKind, HirNode, OwningNode, StatementKind, Type},
        },
        types::{KitFloat, KitInt, KitTy},
    },
};

mod common;
use common::*;

// Test verification macros..

macro_rules! expect_literal {
    ($expr:expr, $expected:expr) => {
        expect_match!(&$expr.kind, ExprKind::Literal(found) => {
            assert_eq!(*found, $expected, "Expected literal '{:?}' but found '{:?}'", $expected, found);
        }, "Expected literal but found '{:?}'", $expr.kind)
    };
}
macro_rules! expect_path {
    ($expr:expr, $expected:literal) => {
        expect_match!(&$expr.kind, ExprKind::Path(ref_path) => {
            let path = ref_path.spanned_ident_path();
            assert_eq!(path.to_string(), $expected, "Expected path '{}' but found '{}'", $expected, path);
        }, "Expected path expression but found '{:?}'", $expr.kind);
    };
    (expr, $expr:expr, $expected:expr) => {
        expect_match!(&$expr.kind, ExprKind::Path(ref_path) => {
            let path = ref_path.spanned_ident_path();
            assert_eq!(path.to_string(), $expected, "Expected path '{}' but found '{}'", $expected, path);
        }, "Expected path expression but found '{:?}'", $expr.kind);
    };
}
macro_rules! expect_any_expr_stmt {
    ($hlir:expr, $node:expr) => {
        expect_match!($node, HirNode::Statement(stmt) => {
            match &stmt.kind {
                StatementKind::Semi(expr_id) | StatementKind::Expr(expr_id) => {
                    let expr_node = get_hir_node($hlir, *expr_id);
                    expect_match!(expr_node, HirNode::Expr(expr) => expr, "Expected expression HIR node")
                },
                _ => panic!("Statement is not an expression or semi-expression statement"),
            }
        }, "Expected statement HIR node")
    };
}
macro_rules! expect_block {
    (id, $hlir:expr, $block_id:expr) => {
        expect_match!(get_hir_node(&$hlir, $block_id), HirNode::Block(block) => block, "Expected node to be HirNode::Block")
    };
    (expr_id, $hlir:expr, $expr_id:expr) => {
        expect_match!(get_hir_node(&$hlir, $expr_id), HirNode::Expr(e) => match &e.kind {
            ExprKind::Block(block_id) => {
                expect_match!(get_hir_node(&$hlir, *block_id), HirNode::Block(b) => b, "Expected block id in block expression to be a block.")
            },
            _ => panic!("Expected block expression"),
        }, "HirNode to be an expression")
    };
}
macro_rules! wrap_and_parse_expr {
    ($test_src:literal) => {
        get_first_expr(&wrap_and_parse!(hir, $test_src)).clone()
    };
}
macro_rules! wrap_and_parse_statement {
    ($test_src:literal) => {
        get_first_statement(&wrap_and_parse!(hir, $test_src)).clone()
    };
}

// Helper getters..

fn get_owning_node(hlir: &HLIR, owner_id: u32) -> &OwningNode {
    hlir.owning_node(OwnerDefId(owner_id))
        .expect("No owning node found.")
}
fn get_main_function(hlir: &HLIR) -> &OwningNode {
    // Main function is typically at index 1 (0 is root module)
    hlir.owning_node(OwnerDefId(1))
        .expect("No main function found.")
}
fn get_hir_node(hlir: &HLIR, hir_id: HirId) -> &HirNode {
    hlir.get_hir_node(hir_id).expect("Expected node to exist.")
}
fn get_hir_node_from(hlir: &HLIR, owner_id: OwnerDefId, node_id: u32) -> &HirNode {
    let hir_id = HirId {
        owner: owner_id,
        id: LocalDefId(node_id),
    };
    get_hir_node(hlir, hir_id)
}
fn get_nth_statement(hlir: &HLIR, index: usize) -> &HirNode {
    let main_fn = get_main_function(hlir);
    let func = main_fn.hir_function_ref().expect("Should be a function");
    let body = func.body.as_ref().expect("Function should have a body");
    let block = get_hir_node(hlir, body.block);

    expect_match!(block, HirNode::Block(block) => {
        let first_stmt_id = block.statements[index];
        get_hir_node(hlir, first_stmt_id)
    }, "Expected block HIR node")
}
fn get_first_statement(hlir: &HLIR) -> &HirNode {
    get_nth_statement(hlir, 0)
}
fn get_nth_expr(hlir: &HLIR, index: usize) -> &Expr {
    let stmt = get_nth_statement(hlir, index);
    expect_any_expr_stmt!(hlir, stmt)
}
fn get_expr(hlir: &HLIR, hir_id: HirId) -> &Expr {
    let node = get_hir_node(hlir, hir_id);
    expect_match!(node, HirNode::Expr(expr) => expr, "Expected expression HIR node")
}
fn get_first_expr(hlir: &HLIR) -> &Expr {
    let stmt = get_first_statement(hlir);
    expect_any_expr_stmt!(hlir, stmt)
}

// The circle is drawn and the air trembles with unseen power.
// By oath and shadow, by breath and will, we open the way.
// Step forward, seekers, for the hour grows thin.
// For the time has come to let the testing begin.

#[test]
fn lower_literal_integers() {
    let expr = wrap_and_parse_expr!("22;");
    expect_literal!(expr, Literal::Integer(22));
}

#[test]
fn lower_literal_floats() {
    let expr = wrap_and_parse_expr!("7.12023;");
    expect_literal!(expr, Literal::Float(7.12023));
}

#[test]
fn lower_literal_strings() {
    let expr = wrap_and_parse_expr!(r#""hello world";"#);
    expect_literal!(expr, Literal::String("hello world".to_string()));
}

#[test]
fn lower_literal_booleans() {
    let hlir = wrap_and_parse!(hir, "true; false;");

    let expr_true = get_first_expr(&hlir);
    let expr_false = get_nth_expr(&hlir, 1);

    expect_literal!(expr_true, Literal::Boolean(true));
    expect_literal!(expr_false, Literal::Boolean(false));
}

fn run_binary_op_test_cases(test_cases: &[(&str, BinaryOpKind, i64, i64)]) {
    for (source, expected_op, expected_lhs, expected_rhs) in test_cases {
        let hlir = parse!(hir, source);
        let expr = get_first_expr(&hlir).clone();

        expect_match!(&expr.kind, ExprKind::BinaryOp(op, lhs, rhs) => {
            assert_eq!(*op, *expected_op, "Failed for: {}", source);

            expect_literal!(get_expr(&hlir, *lhs), Literal::Integer(*expected_lhs));
            expect_literal!(get_expr(&hlir, *rhs), Literal::Integer(*expected_rhs));
        }, "Expected binary operation for: {}", source);
    }
}

macro_rules! binary_op_test_case {
    ($symbol:literal, $expected_op:expr, $expected_lhs:literal, $expected_rhs:literal) => {
        (
            concat!(
                "fn main() { ",
                $expected_lhs,
                " ",
                $symbol,
                " ",
                $expected_rhs,
                "; }"
            ),
            $expected_op,
            $expected_lhs,
            $expected_rhs,
        )
    };
}

#[test]
fn lower_binary_ops_arithmetic() {
    let test_cases = vec![
        binary_op_test_case!("+", BinaryOpKind::Add, 1, 2),
        binary_op_test_case!("-", BinaryOpKind::Sub, 3, 4),
        binary_op_test_case!("*", BinaryOpKind::Mul, 5, 6),
        binary_op_test_case!("/", BinaryOpKind::Div, 7, 8),
        binary_op_test_case!("%", BinaryOpKind::Mod, 9, 10),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn lower_binary_ops_comparison() {
    let test_cases = vec![
        binary_op_test_case!("==", BinaryOpKind::Equal, 1, 2),
        binary_op_test_case!("!=", BinaryOpKind::NotEqual, 3, 4),
        binary_op_test_case!("<", BinaryOpKind::LessThan, 5, 6),
        binary_op_test_case!(">", BinaryOpKind::GreaterThan, 7, 8),
        binary_op_test_case!("<=", BinaryOpKind::LessThanOrEqual, 9, 10),
        binary_op_test_case!(">=", BinaryOpKind::GreaterThanOrEqual, 11, 12),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn lower_binary_ops_logical() {
    let test_cases = vec![
        binary_op_test_case!("&&", BinaryOpKind::And, 1, 1),
        binary_op_test_case!("||", BinaryOpKind::Or, 0, 1),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn lower_binary_ops_bitwise() {
    let test_cases = vec![
        binary_op_test_case!("&", BinaryOpKind::BitwiseAND, 1, 2),
        binary_op_test_case!("|", BinaryOpKind::BitwiseOR, 3, 4),
        binary_op_test_case!("^", BinaryOpKind::BitwiseXOR, 5, 6),
        binary_op_test_case!("<<", BinaryOpKind::ShiftLeft, 7, 8),
        binary_op_test_case!(">>", BinaryOpKind::ShiftRight, 9, 10),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn lower_unary_ops() {
    let test_cases = vec![
        (main!("-a;"), UnaryOpKind::Negate, "a"),
        (main!("!b;"), UnaryOpKind::Not, "b"),
        (main!("*c;"), UnaryOpKind::Dereference, "c"),
    ];

    for (source, expected_op, expected_rhs) in test_cases {
        let expr = get_first_expr(&parse!(hir, source)).clone();

        expect_match!(&expr.kind, ExprKind::UnaryOp(op, rhs) => {
            assert_eq!(*op, expected_op, "Failed for: {}", source);

            expect_path!(expr, get_expr(&parse!(hir, source), *rhs), expected_rhs);
        }, "Expected unary operation for: {}", source);
    }
}

#[test]
fn lower_path_expression() {
    let hlir = wrap_and_parse!(hir, "x;");
    let expr = get_first_expr(&hlir);

    expect_path!(expr, "x");
}

#[test]
fn lower_block_expression() {
    let hlir = wrap_and_parse!(hir, "{ 21 }");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Block(b) => {
        let block = expect_block!(id, &hlir, *b);
        assert_eq!(block.statements.len(), 1);

        let block_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, block.statements[0]));
        expect_literal!(block_expr, Literal::Integer(21));
    }, "Expected block expression");
}

#[test]
fn lower_if_expression() {
    let hlir = wrap_and_parse!(hir, "if true { 1 }");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::If(cond_id, then_id, else_opt) => {
        assert!(else_opt.is_none(), "Expected no else branch");
        
        let cond_node = get_hir_node(&hlir, *cond_id);
        
        expect_match!(cond_node, HirNode::Expr(cond_expr) => {
            expect_literal!(cond_expr, Literal::Boolean(true));
        }, "Expected condition to be an expression");

        let then_block = expect_block!(id, &hlir, *then_id);
        assert_eq!(then_block.statements.len(), 1);

        let block_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, then_block.statements[0]));
        expect_literal!(block_expr, Literal::Integer(1));
    }, "Expected if expression");
}

#[test]
fn lower_if_else_expression() {
    let hlir = wrap_and_parse!(hir, "if false { 1 } else { 2 }");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::If(cond_id, then_id, else_opt) => {
        let cond_node = get_hir_node(&hlir, *cond_id);
        expect_match!(cond_node, HirNode::Expr(cond_expr) => {
            expect_literal!(cond_expr, Literal::Boolean(false));
        }, "Expected condition to be an expression");

        let then_block = expect_block!(id, &hlir, *then_id);
        assert_eq!(then_block.statements.len(), 1);
        let then_block_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, then_block.statements[0]));
        expect_literal!(then_block_expr, Literal::Integer(1));

        assert!(else_opt.is_some(), "Expected else branch");

        let else_block = expect_block!(expr_id, &hlir, else_opt.unwrap());
        assert_eq!(else_block.statements.len(), 1);
        let else_block_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, else_block.statements[0]));
        expect_literal!(else_block_expr, Literal::Integer(2));
    }, "Expected if expression");
}

#[test]
fn lower_while_loop() {
    let hlir = wrap_and_parse!(hir, "while true { break; }");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::While(cond_id, block_id) => {
        let cond_node = get_hir_node(&hlir, *cond_id);
        
        expect_match!(cond_node, HirNode::Expr(cond_expr) => {
            expect_literal!(cond_expr, Literal::Boolean(true));
        }, "Expected condition to be an expression");

        let inner_block = expect_block!(id, &hlir, *block_id);
        assert_eq!(inner_block.statements.len(), 1);

        let block_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, inner_block.statements[0]));
        expect_match!(&block_expr.kind, ExprKind::Break => {}, "Expected break expression");
    }, "Expected while expression");
}

#[test]
fn lower_function_call() {
    let hlir = wrap_and_parse!(hir, "test();");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Call(target_id, args) => {
        assert_eq!(args.len(), 0, "Expected no arguments");
        
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "test");
        }, "Expected target to be an expression");
    }, "Expected call expression");
}

#[test]
fn lower_function_call_with_args() {
    let hlir = wrap_and_parse!(hir, r#"test(1, "hi");"#);
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Call(target_id, args) => {
        assert_eq!(args.len(), 2, "Expected 2 arguments");

        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "test");
        }, "Expected target to be an expression");

        expect_literal!(get_expr(&hlir, args[0]), Literal::Integer(1));
        expect_literal!(get_expr(&hlir, args[1]), Literal::String("hi".to_string()));
    }, "Expected call expression");
}

#[test]
fn lower_method_call() {
    let hlir = wrap_and_parse!(hir, "obj.method();");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::MethodCall(target_id, method_name, args) => {
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "obj");
        }, "Expected target to be an expression");

        assert_eq!(method_name.str(), "method");
        assert_eq!(args.len(), 0);
    }, "Expected method call expression");
}

#[test]
fn lower_method_call_with_args() {
    let hlir = wrap_and_parse!(hir, r#"obj.method(1, "hi");"#);
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::MethodCall(target_id, method_name, args) => {
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "obj");
        }, "Expected target to be an expression");

        assert_eq!(method_name.str(), "method");
        assert_eq!(args.len(), 2);

        expect_literal!(get_expr(&hlir, args[0]), Literal::Integer(1));
        expect_literal!(get_expr(&hlir, args[1]), Literal::String("hi".to_string()));
    }, "Expected method call expression");
}

#[test]
fn lower_field_access() {
    let hlir = wrap_and_parse!(hir, "obj.field;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::FieldAccess(target_id, field_name) => {
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "obj");
        }, "Expected target to be an expression");

        assert_eq!(field_name.str(), "field");
    }, "Expected field access expression");
}

#[test]
fn lower_index_expression() {
    let hlir = wrap_and_parse!(hir, "arr[0];");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Index(target_id, index_id) => {
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "arr");
        }, "Expected target to be an expression");

        expect_match!(get_hir_node(&hlir, *index_id), HirNode::Expr(index_expr) => {
            expect_literal!(index_expr, Literal::Integer(0));
        }, "Expected index to be an expression");
    }, "Expected index expression");
}

#[test]
fn lower_assignment() {
    let hlir = wrap_and_parse!(hir, "x = 5;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Assign(target_id, value_id) => {
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_path!(target_expr, "x");
        }, "Expected target to be an expression");

        expect_match!(get_hir_node(&hlir, *value_id), HirNode::Expr(value_expr) => {
            expect_literal!(value_expr, Literal::Integer(5));
        }, "Expected value to be an expression");
    }, "Expected assignment expression");
}

#[test]
fn lower_return_statement() {
    let hlir = wrap_and_parse!(hir, "return 11;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Return(Some(ret_id)) => {
        let ret_node = get_hir_node(&hlir, *ret_id);
        
        expect_match!(ret_node, HirNode::Expr(ret_expr) => {
            expect_literal!(ret_expr, Literal::Integer(11));
        }, "Expected return value to be an expression");
    }, "Expected return expression");
}

#[test]
fn lower_return_without_value() {
    let hlir = wrap_and_parse!(hir, "return;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Return(None) => {}, "Expected return without value");
}

#[test]
fn lower_break_statement() {
    let hlir = wrap_and_parse!(hir, "break;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Break => {}, "Expected break expression");
}

#[test]
fn lower_continue_statement() {
    let hlir = wrap_and_parse!(hir, "continue;");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Continue => {}, "Expected continue expression");
}

#[test]
fn lower_let_statement() {
    let hlir = wrap_and_parse!(hir, "let x = 13;");
    let stmt = get_first_statement(&hlir);

    expect_match!(stmt, HirNode::Statement(stmt) => {
        expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
            assert_eq!(let_stmt.ident.str(), "x");
            assert_eq!(let_stmt.mutable, Mutability::Immutable);
            assert!(let_stmt.initial_value.is_some());

            expect_match!(get_hir_node(&hlir, let_stmt.initial_value.unwrap()), HirNode::Expr(expr) => {
                expect_literal!(expr, Literal::Integer(13));
            }, "Expected initial value to be an expression");
        }, "Expected let statement");
    }, "Expected statement HIR node");
}

#[test]
fn lower_let_statement_mutable() {
    let hlir = wrap_and_parse!(hir, "let mut y = 10;");
    let stmt = get_first_statement(&hlir);

    expect_match!(stmt, HirNode::Statement(stmt) => {
        expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
            assert_eq!(let_stmt.ident.str(), "y");
            assert_eq!(let_stmt.mutable, Mutability::Mutable);
            assert!(let_stmt.initial_value.is_some());

            expect_match!(get_hir_node(&hlir, let_stmt.initial_value.unwrap()), HirNode::Expr(expr) => {
                expect_literal!(expr, Literal::Integer(10));
            }, "Expected initial value to be an expression");
        }, "Expected let statement");
    }, "Expected statement HIR node");
}

#[test]
fn lower_let_statement_with_type() {
    let hlir = wrap_and_parse!(hir, "let z: i32 = 5;");
    let stmt = get_first_statement(&hlir);

    expect_match!(stmt, HirNode::Statement(stmt) => {
        expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
            assert_eq!(let_stmt.ident.str(), "z");
            let Type::Resolved(a) = &let_stmt.ty else {
                panic!("Expected resolved type.");
            };
            assert_eq!(*a, KitTy::Int(KitInt::I32));
        }, "Expected let statement");
    }, "Expected statement HIR node");
}

#[test]
fn lower_struct_initialisation() {
    let hlir = wrap_and_parse!(hir, "Point { x: 1, y: 2 };");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::StructInit(struct_init) => {
        assert_eq!(struct_init.ty_path.spanned_ident_path().to_string(), "Point");
        assert_eq!(struct_init.fields.len(), 2);
        assert_eq!(struct_init.fields[0].ident.str(), "x");
        assert_eq!(struct_init.fields[1].ident.str(), "y");
    }, "Expected struct initialisation");
}

#[test]
fn lower_function_declaration() {
    let hlir = parse!(hir, "fn test(a: i32, b: i32) -> i32 { a + b }");

    // Function should be in owning_nodes
    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let func_node = get_owning_node(&hlir, 1);
    let func = func_node.hir_function_ref().expect("Should be a function");

    assert_eq!(func.ident.str(), "test");
    assert_eq!(func.sig.parameters.len(), 2);

    let func_body = func.body.as_ref().expect("Function should have body.");
    let func_block = expect_block!(id, &hlir, func_body.block);
    assert!(!func_block.statements.is_empty());

    let func_block_first_statement =
        expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, func_block.statements[0]));
    expect_match!(&func_block_first_statement.kind, ExprKind::BinaryOp(op, lhs, rhs) => {
        assert_eq!(*op, BinaryOpKind::Add);

        expect_path!(expr, get_expr(&hlir, *lhs), "a");
        expect_path!(expr, get_expr(&hlir, *rhs), "b");
    }, "Expected binary operation in function body");
}

#[test]
fn lower_function_with_visibility() {
    let hlir = parse!(hir, "pub fn test() {}");

    let func_node = get_owning_node(&hlir, 1);
    let func = func_node.hir_function_ref().expect("Should be a function");

    assert_eq!(func.vis, Visibility::Public);
}

#[test]
fn lower_struct_declaration() {
    let hlir = parse!(hir, "struct Point { x: i32, y: i32 }");

    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let struct_node = get_owning_node(&hlir, 1);
    let struct_def = struct_node.hir_struct_ref().expect("Should be a struct");

    assert_eq!(struct_def.ident.str(), "Point");
    assert_eq!(struct_def.fields.len(), 2);

    expect_match!(get_hir_node(&hlir, struct_def.fields[0]), HirNode::Field(field_info) => {
        assert_eq!(field_info.ident.str(), "x");
        assert_eq!(field_info.ty, Type::Resolved(KitTy::Int(KitInt::I32)));
    }, "Expected named field id");

    expect_match!(get_hir_node(&hlir, struct_def.fields[1]), HirNode::Field(field_info) => {
        assert_eq!(field_info.ident.str(), "y");
        assert_eq!(field_info.ty, Type::Resolved(KitTy::Int(KitInt::I32)));
    }, "Expected named field id");
}

#[test]
fn lower_struct_with_visibility() {
    let hlir = parse!(hir, "pub struct Visible { pub field: i32 }");

    let struct_node = get_owning_node(&hlir, 1);
    let struct_def = struct_node.hir_struct_ref().expect("Should be a struct");

    assert_eq!(struct_def.vis, Visibility::Public);
}

#[test]
fn lower_impl_block() {
    let hlir = parse!(
        hir,
        "impl Point { fn new() -> Point { Point { x: 1, y: 2 } } }"
    );

    // Impl block should create an owning node
    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let impl_node = get_owning_node(&hlir, 1);
    let impl_block = impl_node.hir_impl_ref().expect("Should be an impl block");

    assert_eq!(impl_block.self_ty.to_string(), "Point");
    assert_eq!(impl_block.items.len(), 1);

    let method_node = get_owning_node(&hlir, impl_block.items[0].0);
    let method = method_node
        .hir_function_ref()
        .expect("Should be a function");

    assert_eq!(method.ident.str(), "new");
    let method_body = method.body.as_ref().expect("Method should have body.");
    let method_block = expect_block!(id, &hlir, method_body.block);

    assert_eq!(method_block.statements.len(), 1);

    let method_block_first_statement =
        expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, method_block.statements[0]));
    expect_match!(&method_block_first_statement.kind, ExprKind::StructInit(struct_init) => {
        assert_eq!(struct_init.ty_path.spanned_ident_path().to_string(), "Point");
        assert_eq!(struct_init.fields.len(), 2);

        assert_eq!(struct_init.fields[0].ident.str(), "x");
        expect_literal!(get_expr(&hlir, struct_init.fields[0].expr), Literal::Integer(1));
        
        assert_eq!(struct_init.fields[1].ident.str(), "y");
        expect_literal!(get_expr(&hlir, struct_init.fields[1].expr), Literal::Integer(2));
    }, "Expected struct initialisation in method body");
}

#[test]
fn lower_const_declaration() {
    let hlir = parse!(hir, "const DATE: f32 = 7.12023;");

    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let const_node = get_owning_node(&hlir, 1);
    let const_def = const_node.hir_const_ref().expect("Should be a constant");

    assert_eq!(const_def.ident.str(), "DATE");
    assert_eq!(const_def.ty, Type::Resolved(KitTy::Float(KitFloat::F32)));
    expect_literal!(get_expr(&hlir, const_def.expr), Literal::Float(7.12023));
}

#[test]
fn lower_nested_blocks() {
    let hlir = wrap_and_parse!(hir, "{ { 23 } }");
    let expr = get_first_expr(&hlir);

    expect_match!(&expr.kind, ExprKind::Block(outer_block_id) => {
        let outer_block = expect_block!(id, &hlir, *outer_block_id);
        assert_eq!(outer_block.statements.len(), 1);

        let stmt_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, outer_block.statements[0]));
        let inner_block = expect_match!(&stmt_expr.kind, ExprKind::Block(inner_block_id) => {
            expect_block!(id, &hlir, *inner_block_id)
        }, "Expected inner block expression");
        assert_eq!(inner_block.statements.len(), 1);
        let inner_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, inner_block.statements[0]));
        expect_literal!(inner_expr, Literal::Integer(23));
    }, "Expected block expression");
}

#[test]
fn lower_complex_expression() {
    let hlir = wrap_and_parse!(hir, "(1 + 2) * (3 - 4) / 5;");
    let expr = get_first_expr(&hlir);

    // This should be:
    //      - lhs_1 / 5
    //      - lhs_1 = lhs_2 * lhs_3
    //      - lhs_2 = 1 + 2
    //      - lhs_3 = 3 - 4

    expect_match!(&expr.kind, ExprKind::BinaryOp(op, lhs_id, rhs_id) => {
        assert_eq!(*op, BinaryOpKind::Div);

        let lhs_expr = get_expr(&hlir, *lhs_id);
        let rhs_expr = get_expr(&hlir, *rhs_id);

        // lhs_1: (1 + 2) * (3 - 4)
        expect_match!(&lhs_expr.kind, ExprKind::BinaryOp(op2, lhs2_id, rhs2_id) => {
            assert_eq!(*op2, BinaryOpKind::Mul);

            let lhs2_expr = get_expr(&hlir, *lhs2_id);
            let rhs2_expr = get_expr(&hlir, *rhs2_id);

            // lhs_2: 1 + 2
            expect_match!(&lhs2_expr.kind, ExprKind::BinaryOp(op3, lhs3_id, rhs3_id) => {
                assert_eq!(*op3, BinaryOpKind::Add);

                expect_literal!(get_expr(&hlir, *lhs3_id), Literal::Integer(1));
                expect_literal!(get_expr(&hlir, *rhs3_id), Literal::Integer(2));
            }, "Expected addition expression as lhs of multiplication");

            // rhs_3: 3 - 4
            expect_match!(&rhs2_expr.kind, ExprKind::BinaryOp(op3, lhs3_id, rhs3_id) => {
                assert_eq!(*op3, BinaryOpKind::Sub);

                expect_literal!(get_expr(&hlir, *lhs3_id), Literal::Integer(3));
                expect_literal!(get_expr(&hlir, *rhs3_id) , Literal::Integer(4));
            }, "Expected addition expression as lhs of multiplication");
        }, "Expected multiplication expression as lhs of division");

        // rhs: 5
        expect_literal!(rhs_expr, Literal::Integer(5));
    }, "Expected binary expression");
}

#[test]
fn lower_module() {
    let hlir = parse!(hir, "mod test { fn test_func() {} }");

    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let module_node = get_owning_node(&hlir, 1);
    let module = module_node.hir_module_ref().expect("Should be a module");

    assert_eq!(module.ident.ident().str(), "test");
    assert_eq!(module.item_ids.len(), 1);
    assert_eq!(module.vis, Visibility::Private);

    let func_node = get_owning_node(&hlir, module.item_ids[0].0);
    let func = func_node.hir_function_ref().expect("Should be a function");
    assert_eq!(func.ident.str(), "test_func");
    assert_eq!(func.vis, Visibility::Private);
    assert!(func.sig.parameters.is_empty());
    assert!(func.sig.output.is_unit());
}

#[test]
fn lower_use_statement() {
    let hlir = parse!(hir, "use test::test1::TestStruct;");

    assert!(
        hlir.owner_node_count() >= 2,
        "Expected at least 2 owning nodes"
    );

    let use_node = get_owning_node(&hlir, 1);
    let use_stmt = use_node.hir_use_ref().expect("Should be a use statement");
    assert_eq!(use_stmt.vis, Visibility::Private);
    assert_eq!(use_stmt.imports.len(), 1);
    assert_eq!(use_stmt.imports[0].to_string(), "test::test1::TestStruct");
}

#[test]
fn lower_chained_method_calls() {
    let hlir = wrap_and_parse!(hir, "obj.method1().method2(1);");
    let expr = get_first_expr(&hlir);

    // Outer method call should be method2
    expect_match!(&expr.kind, ExprKind::MethodCall(target_id, method_name, args) => {
        assert_eq!(method_name.str(), "method2");
        assert_eq!(args.len(), 1);
        expect_literal!(get_expr(&hlir, args[0]), Literal::Integer(1));

        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_match!(&target_expr.kind, ExprKind::MethodCall(inner_target_id, inner_method_name, _inner_args) => {
                assert_eq!(inner_method_name.str(), "method1");

                expect_match!(get_hir_node(&hlir, *inner_target_id), HirNode::Expr(inner_target_expr) => {
                    expect_path!(inner_target_expr, "obj");
                }, "Expected target of method1 to be an expression");
            }, "Expected method1 call expression");
        }, "Expected target of method2 to be an expression");
    }, "Expected method call expression");
}

#[test]
fn lower_chained_field_access() {
    let hlir = wrap_and_parse!(hir, "obj.field1.field2;");
    let expr = get_first_expr(&hlir);

    // Outer field access should be field2
    expect_match!(&expr.kind, ExprKind::FieldAccess(target_id, field_name) => {
        assert_eq!(field_name.str(), "field2");
        expect_match!(get_hir_node(&hlir, *target_id), HirNode::Expr(target_expr) => {
            expect_match!(&target_expr.kind, ExprKind::FieldAccess(inner_target_id, inner_field_name) => {
                assert_eq!(inner_field_name.str(), "field1");

                expect_match!(get_hir_node(&hlir, *inner_target_id), HirNode::Expr(inner_target_expr) => {
                    expect_path!(inner_target_expr, "obj");
                }, "Expected target of field1 to be an expression");
            }, "Expected field1 access expression");
        }, "Expected target of field2 to be an expression");
    }, "Expected field access expression");
}

#[test]
fn lower_empty_function_body() {
    let hlir = parse!(hir, "fn empty() {}");

    let func_node = get_owning_node(&hlir, 1);
    let func = func_node.hir_function_ref().expect("Should be a function");

    let func_body = func.body.as_ref().expect("Function should have body.");
    let func_block = expect_block!(id, &hlir, func_body.block);

    assert!(
        func_block.statements.is_empty(),
        "Expected empty function body"
    );
}

#[test]
fn lower_function_with_self_param() {
    let hlir = parse!(hir, "impl Point { fn get_x(self) -> i32 { self.x } }");

    let impl_node = get_owning_node(&hlir, 1);
    let impl_block = impl_node.hir_impl_ref().expect("Should be an impl block");

    assert_eq!(impl_block.self_ty.to_string(), "Point");
    assert_eq!(impl_block.items.len(), 1);

    // Get the method
    let method_node = get_owning_node(&hlir, impl_block.items[0].0);
    let method = method_node
        .hir_function_ref()
        .expect("Should be a function");

    assert!(method.is_method);
    assert_eq!(method.sig.parameters.len(), 1);
    assert!(matches!(
        method.sig.parameters[0],
        Type::Unresolved(Ty::This(..))
    ));
    assert_eq!(method.sig.output, Type::Resolved(KitTy::Int(KitInt::I32)));
}

#[test]
fn lower_multiple_statements() {
    let hlir = wrap_and_parse!(hir, "let x = 1; let y = 2; x + y");
    let main_fn = get_main_function(&hlir);
    let func = main_fn.hir_function_ref().expect("Should be a function");
    let body = func.body.as_ref().expect("Function should have a body");
    let body = get_hir_node(&hlir, body.block);

    expect_match!(body, HirNode::Block(block) => {
        assert_eq!(block.statements.len(), 3, "Expected three statements");

        expect_match!(get_hir_node(&hlir, block.statements[0]), HirNode::Statement(stmt) => {
            expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
                assert_eq!(let_stmt.ident.str(), "x");
                expect_match!(get_hir_node(&hlir, let_stmt.initial_value.expect("Expected initial value")), HirNode::Expr(expr) => {
                    expect_literal!(expr, Literal::Integer(1));
                }, "Expected initial value to be an expression");
            }, "Expected let statement");
        }, "Expected first statement to be a let statement");

        expect_match!(get_hir_node(&hlir, block.statements[1]), HirNode::Statement(stmt) => {
            expect_match!(&stmt.kind, StatementKind::Let(let_stmt) => {
                assert_eq!(let_stmt.ident.str(), "y");
                expect_match!(get_hir_node(&hlir, let_stmt.initial_value.expect("Expected initial value")), HirNode::Expr(expr) => {
                    expect_literal!(expr, Literal::Integer(2));
                }, "Expected initial value to be an expression");
            }, "Expected let statement");
        }, "Expected first statement to be a let statement");

        let last_expr = expect_any_expr_stmt!(&hlir, get_hir_node(&hlir, block.statements[2]));
        expect_match!(&last_expr.kind, ExprKind::BinaryOp(op, lhs_id, rhs_id) => {
            assert_eq!(*op, BinaryOpKind::Add);

            expect_path!(expr, get_expr(&hlir, *lhs_id), "x");
            expect_path!(expr, get_expr(&hlir, *rhs_id), "y");
        }, "Expected addition expression as last statement");
    }, "Expected block HIR node");
}
