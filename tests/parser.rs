use kitlang::ast::{
    ASTRoot, BinaryOpKind, BindingPattern, Block, Expression, ExpressionKind, FunctionReturnTy,
    Item, ItemKind, Literal, LocalKind, Mutability, StatementKind, Ty, UnaryOpKind, Visibility,
};

mod common;
use common::*;

// Helper functions/macros..

/// Wraps the given source code in a `main` function.
///
/// Essentially generates:
/// ```ignore
/// fn main() {
///    // $test_src
/// }
/// ```
macro_rules! wrap_and_parse_expr {
    ($test_src:literal) => {
        get_first_expr(&wrap_and_parse!(parse, $test_src)).clone()
    };
}
macro_rules! wrap_and_parse_statement {
    ($test_src:literal) => {
        get_first_statement(&wrap_and_parse!(parse, $test_src)).clone()
    };
}

// Test verification macros..

macro_rules! expect_literal {
    ($expr:expr, $expected:expr) => {
        expect_match!(&$expr.kind, ExpressionKind::Literal(found) => {
            assert_eq!(*found, $expected, "Expected literal '{:?}' but found '{:?}'", $expected, found);
        }, "Expected literal but found '{:?}'", $expr.kind);
    };
}
macro_rules! expect_path {
    ($expr:expr, $expected:literal) => {
        expect_match!(&$expr.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), $expected, "Expected ident path '{}' but found '{}'", $expected, path);
        }, "Expected ident path expression but found '{:?}'", $expr.kind);
    };
}
macro_rules! expect_block {
    ($expr:expr) => {
        expect_match!(&$expr.kind, ExpressionKind::Block(block) => block,
            "Expected block expression but found '{:?}'", $expr.kind
        )
    };
}
macro_rules! expect_ty {
    (str, $expr:expr, $expected:literal) => {
        expect_match!(&$expr, Ty::Type(path) => {
            assert_eq!(path.to_string(), $expected, "Expected type '{}' but found '{}'", $expected, path);
        }, "Expected type path expression but found '{:?}'", $expr);
    };
    ($expr:expr, $expected:expr) => {
        assert_eq!($expr, $expected, "Expected type '{}' but found '{}'", $expected, e);
    };
}

// Helper getters..
fn first_item(root: &ASTRoot) -> &Item {
    &root.items[0]
}
fn first_as_function(root: &ASTRoot) -> &kitlang::ast::Function {
    if let ItemKind::Fn(func) = &first_item(root).kind {
        func
    } else {
        panic!("First item is not a function");
    }
}
fn statement_any_expr(body: &Block, index: usize) -> &Expression {
    match &body.statements[index].kind {
        StatementKind::Semi(expr) | StatementKind::Expr(expr) => expr,
        _ => panic!(
            "Statement at index {} is not an expression or a semi-expression",
            index
        ),
    }
}
fn get_nth_statement(root: &ASTRoot, index: usize) -> &kitlang::ast::Statement {
    let func = first_as_function(root);
    let body = func.body.as_ref().unwrap();
    &body.statements[index]
}
fn get_first_statement(root: &ASTRoot) -> &kitlang::ast::Statement {
    get_nth_statement(root, 0)
}
fn get_nth_expr(root: &ASTRoot, index: usize) -> &Expression {
    let func = first_as_function(root);
    let body = func.body.as_ref().unwrap();
    statement_any_expr(body, index)
}
fn get_first_expr(root: &ASTRoot) -> &Expression {
    get_nth_expr(root, 0)
}

macro_rules! check_ident_pattern {
    ($pattern:expr, $ident:expr, $muta:expr) => {{
        let BindingPattern::Variable(found_ident, is_mut) = $pattern else {
            panic!("Expected binding pattern to be a variable");
        };
        assert_eq!(
            found_ident.str(),
            $ident,
            "Expected identifier `{}` but found `{}`",
            $ident,
            found_ident.str()
        );
        assert_eq!(
            *is_mut, $muta,
            "Expected mutability `{:?}` but found `{:?}`",
            $muta, *is_mut
        );
    }};
}

// Let the testing begin...

#[test]
fn parse_literal_integers() {
    expect_literal!(&wrap_and_parse_expr!("42"), Literal::Integer(42));
}

#[test]
fn parse_literal_floats() {
    expect_literal!(&wrap_and_parse_expr!("7.12023"), Literal::Float(7.12023));
}

#[test]
fn parse_literal_strings() {
    expect_literal!(
        &wrap_and_parse_expr!(r#""hello world""#),
        Literal::String("hello world".to_string())
    );
}

#[test]
fn parse_literal_booleans() {
    let ast = wrap_and_parse!(parse, "true; false;");

    let true_expr = get_first_expr(&ast);
    let false_expr = get_nth_expr(&ast, 1);

    expect_literal!(true_expr, Literal::Boolean(true));
    expect_literal!(false_expr, Literal::Boolean(false));
}

fn run_binary_op_test_cases(test_cases: &[(&str, BinaryOpKind, i64, i64)]) {
    for (source, expected_op, expected_lhs, expected_rhs) in test_cases {
        let ast = parse!(parse, source);
        let func = first_as_function(&ast);
        let body = func.body.as_ref().unwrap();

        if body.statements.is_empty() {
            panic!("No statements found in function body ast: {:#?}", ast);
        }

        let expr = get_first_expr(&ast).clone();

        expect_match!(&expr.kind, ExpressionKind::BinaryOp(op, lhs, rhs) => {
            assert_eq!(*op, *expected_op, "Failed for: {}", source);

            expect_literal!(lhs, Literal::Integer(*expected_lhs));
            expect_literal!(rhs, Literal::Integer(*expected_rhs));
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
fn parse_binary_ops_arithmetic() {
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
fn parse_binary_ops_literal_signs_edge_case() {
    let test_cases = vec![
        ("fn main() { 1 +2; }", (BinaryOpKind::Add), 1, 2),
        ("fn main() { 3+ 4; }", (BinaryOpKind::Add), 3, 4),
        ("fn main() { 5 -6; }", (BinaryOpKind::Sub), 5, 6),
        ("fn main() { 7- 8; }", (BinaryOpKind::Sub), 7, 8),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_binary_ops_comparison() {
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
fn parse_binary_ops_logical() {
    let test_cases = vec![
        binary_op_test_case!("&&", BinaryOpKind::And, 1, 1),
        binary_op_test_case!("||", BinaryOpKind::Or, 0, 1),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_binary_ops_bitwise() {
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
fn parse_unary_ops() {
    let test_cases = vec![
        (main!("-a;"), UnaryOpKind::Negate),
        (main!("!true;"), UnaryOpKind::Not),
        (main!("*ptr;"), UnaryOpKind::Dereference),
    ];

    for (source, expected_op) in test_cases {
        let expr = get_first_expr(&parse!(parse, source)).clone();

        expect_match!(&expr.kind, ExpressionKind::UnaryOp(op, ..) => {
            assert_eq!(*op, expected_op, "Failed for: {}", source);
        }, "Expected unary operation for: {}", source);
    }
}

#[test]
fn parse_block_expression() {
    let expr = wrap_and_parse_expr!("{ 42 }");
    let block = expect_block!(&expr);

    expect_literal!(statement_any_expr(block, 0), Literal::Integer(42));
}

#[test]
fn parse_if_expression() {
    let expr = wrap_and_parse_expr!("if true { 1 }");

    expect_match!(&expr.kind, ExpressionKind::If(cond, block, else_block) => {
        expect_literal!(cond, Literal::Boolean(true));

        expect_literal!(statement_any_expr(block, 0), Literal::Integer(1));
        
        assert!(else_block.is_none(), "Expected no else block");
    }, "Expected if expression");
}

#[test]
fn parse_if_else_expression() {
    let expr = wrap_and_parse_expr!("if true { 1 } else { 2 }");

    expect_match!(&expr.kind, ExpressionKind::If(cond, block, else_block) => {
        expect_literal!(cond, Literal::Boolean(true));

        expect_literal!(statement_any_expr(block, 0), Literal::Integer(1));

        let else_block = expect_block!(else_block.as_ref().expect("Expected else block is Some."));
        expect_literal!(statement_any_expr(else_block, 0), Literal::Integer(2));
    }, "Expected if expression");
}

#[test]
fn parse_nested_if_statements() {
    let expr = wrap_and_parse_expr!(
        "if true {
        if a {
            1
        } else {
            2
        }
    }"
    );

    expect_match!(&expr.kind, ExpressionKind::If(cond, block, else_block) => {
        expect_literal!(cond, Literal::Boolean(true));

        expect_match!(&statement_any_expr(block, 0).kind, ExpressionKind::If(nested_cond, nested_block, nested_else_block) => {
            expect_path!(nested_cond, "a");

            expect_literal!(statement_any_expr(nested_block, 0), Literal::Integer(1));

            let else_block = expect_block!(nested_else_block.as_ref().expect("Expected else block is Some."));
            expect_literal!(statement_any_expr(else_block, 0), Literal::Integer(2));
        }, "Expected nested if expression");

        assert!(else_block.is_none(), "Expected no else block");
    }, "Expected if expression");
}

#[test]
fn parse_while_loop() {
    let expr = wrap_and_parse_expr!("while true { 1; }");

    expect_match!(&expr.kind, ExpressionKind::While(cond, body) => {
        expect_literal!(cond, Literal::Boolean(true));

        expect_literal!(statement_any_expr(body, 0), Literal::Integer(1));
    }, "Expected while loop");
}

#[test]
fn parse_return_expression() {
    let expr = wrap_and_parse_expr!("return 42;");

    expect_match!(&expr.kind, ExpressionKind::Return(ret_expr) => {
        expect_literal!(ret_expr.as_ref().expect("Expected return statement has expression."), Literal::Integer(42));
    }, "Expected return expression");
}

#[test]
fn parse_return_no_expression() {
    let expr = wrap_and_parse_expr!("return;");

    expect_match!(&expr.kind, ExpressionKind::Return(ret_expr) => {
        assert!(ret_expr.is_none(), "Expected return statement with no expression");
    }, "Expected return expression");
}

#[test]
fn parse_function_call() {
    let expr = wrap_and_parse_expr!("test();");

    expect_match!(&expr.kind, ExpressionKind::Call(func, args) => {
        assert_eq!(args.len(), 0);
        expect_path!(func, "test");
    }, "Expected function call");
}

#[test]
fn parse_function_call_with_args() {
    let expr = wrap_and_parse_expr!(r#"test(1, 2.2, "hello")"#);

    expect_match!(&expr.kind, ExpressionKind::Call(func, args) => {
        assert_eq!(args.len(), 3);

        expect_path!(func, "test");

        expect_literal!(&args[0], Literal::Integer(1));
        expect_literal!(&args[1], Literal::Float(2.2));
        expect_literal!(&args[2], Literal::String("hello".to_string()));
    }, "Expected function call");
}

#[test]
fn parse_method_call() {
    let expr = wrap_and_parse_expr!("obj.method()");

    expect_match!(&expr.kind, ExpressionKind::MethodCall(call_info) => {
        expect_path!(&call_info.target_expr, "obj");
        assert_eq!(call_info.method_ident.str(), "method");
        assert_eq!(call_info.args.len(), 0);
    }, "Expected method call");
}

#[test]
fn parse_method_call_with_args() {
    let expr = wrap_and_parse_expr!(r#"obj.method(1, "str")"#);

    expect_match!(&expr.kind, ExpressionKind::MethodCall(call_info) => {
        expect_path!(&call_info.target_expr, "obj");
        assert_eq!(call_info.method_ident.str(), "method");
        assert_eq!(call_info.args.len(), 2);

        expect_literal!(&call_info.args[0], Literal::Integer(1));
        expect_literal!(&call_info.args[1], Literal::String("str".to_string()));
    }, "Expected method call");
}

#[test]
fn parse_struct_initialisation() {
    let expr = wrap_and_parse_expr!("Point { x: 1, y: 2 }");

    expect_match!(&expr.kind, ExpressionKind::StructInit(init) => {
        assert_eq!(init.fields.len(), 2);

        assert_eq!(init.fields[0].ident.str(), "x");
        expect_literal!(&init.fields[0].expr, Literal::Integer(1));

        assert_eq!(init.fields[1].ident.str(), "y");
        expect_literal!(&init.fields[1].expr, Literal::Integer(2));
    }, "Expected struct initialisation");
}

#[test]
fn parse_member_access() {
    let expr = wrap_and_parse_expr!("obj.field");

    expect_match!(&expr.kind, ExpressionKind::FieldAccess(target, field_ident) => {
        expect_path!(&target, "obj");
        assert_eq!(field_ident.str(), "field");
    }, "Expected member access");
}

#[test]
fn parse_assignment() {
    let expr = wrap_and_parse_expr!("x = 5");

    expect_match!(&expr.kind, ExpressionKind::Assign(target, value) => {
        expect_path!(&target, "x");

        expect_literal!(value, Literal::Integer(5));
    }, "Expected assignment expression");
}

#[test]
fn parse_let_statement_basic() {
    let stmt = wrap_and_parse_statement!("let x: i32 = 5;");

    expect_match!(&stmt.kind, StatementKind::Let(local) => {
        check_ident_pattern!(&local.pattern, "x", Mutability::Immutable);

        expect_ty!(str, &local.ty, "i32");

        expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
            expect_literal!(init_expr, Literal::Integer(5));
        }, "Expected let statement to have an initialise expression");
    }, "Expected let statement");
}

#[test]
fn parse_let_statement_mutable() {
    let stmt = wrap_and_parse_statement!(
        r#"let mut y: string = "TESTING TESTING HAHAHAHAHAHAHHAHAHAHAHHA";"#
    );

    expect_match!(&stmt.kind, StatementKind::Let(local) => {
        check_ident_pattern!(&local.pattern, "y", Mutability::Mutable);

        expect_ty!(str, &local.ty, "string");

        expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
            expect_literal!(init_expr, Literal::String("TESTING TESTING HAHAHAHAHAHAHHAHAHAHAHHA".to_string()));
        }, "Expected let statement to have an initialise expression");
    }, "Expected let statement");
}

#[test]
fn parse_empty_statement() {
    let stmt = wrap_and_parse_statement!(";");
    assert!(matches!(stmt.kind, StatementKind::Empty));
}

#[test]
fn parse_function_no_params() {
    let ast = parse_ok("fn test() {}");
    let func = first_as_function(&ast);

    assert_eq!(func.ident.str(), "test");
    assert_eq!(func.sig.parameters.len(), 0);
    assert_eq!(func.sig.output, FunctionReturnTy::Default);
}

#[test]
fn parse_function_with_params() {
    let ast = parse_ok("fn test(a: i32, b: f32) {}");
    let func = first_as_function(&ast);

    assert_eq!(func.ident.str(), "test");
    assert_eq!(ast.items[0].vis, Visibility::Private);
    assert_eq!(func.sig.parameters.len(), 2);

    check_ident_pattern!(&func.sig.parameters[0].pattern, "a", Mutability::Immutable);
    expect_ty!(str, func.sig.parameters[0].ty, "i32");

    check_ident_pattern!(&func.sig.parameters[1].pattern, "b", Mutability::Immutable);
    expect_ty!(str, func.sig.parameters[1].ty, "f32");
}

#[test]
fn parse_function_with_return_type() {
    let ast = parse_ok("pub fn test() -> i32 {}");
    let func = first_as_function(&ast);

    assert_eq!(func.ident.str(), "test");
    assert_eq!(ast.items[0].vis, Visibility::Public);
    assert_eq!(func.sig.parameters.len(), 0);

    expect_match!(&func.sig.output, FunctionReturnTy::Ty(ret_ty) => {
        expect_ty!(str, **ret_ty, "i32");
    }, "Expected function to have a return type");
}

#[test]
fn parse_struct_basic() {
    let ast = parse_ok("struct Point {}");

    expect_match!(&first_item(&ast).kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Point");
        assert_eq!(s.fields.len(), 0);
    }, "Expected a struct");
}

#[test]
fn parse_struct_with_fields() {
    let ast = parse_ok("struct Point { x: i32, y: i32}");

    expect_match!(&first_item(&ast).kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Point");
        assert_eq!(s.fields.len(), 2);

        assert_eq!(s.fields[0].ident.str(), "x");
        expect_ty!(str, s.fields[0].ty, "i32");
        
        assert_eq!(s.fields[1].ident.str(), "y");
        expect_ty!(str, s.fields[1].ty, "i32");
    }, "Expected a struct");
}

#[test]
fn parse_struct_with_visibility() {
    let ast = parse_ok("pub struct Point { pub x: f32, y: f32 }");
    let item = first_item(&ast);

    expect_match!(&item.kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Point");
        assert_eq!(s.fields.len(), 2);

        assert_eq!(s.fields[0].ident.str(), "x");
        expect_ty!(str, s.fields[0].ty, "f32");
        
        assert_eq!(s.fields[1].ident.str(), "y");
        expect_ty!(str, s.fields[1].ty, "f32");

        assert_eq!(s.fields[0].vis, Visibility::Public);
        assert_eq!(s.fields[1].vis, Visibility::Private);

        assert_eq!(item.vis, Visibility::Public);
    }, "Expected a struct");
}

#[test]
fn parse_unit_struct() {
    let ast = parse_ok("struct Unit;");

    expect_match!(&first_item(&ast).kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Unit");
        assert_eq!(s.fields.len(), 0);
    }, "Expected a struct");
}

#[test]
fn parse_impl_block() {
    let ast = parse_ok("impl Point { }");

    expect_match!(&first_item(&ast).kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "Point");
        assert_eq!(impl_block.items.len(), 0);
    }, "Expected an impl block");
}

#[test]
fn parse_impl_block_with_method() {
    let ast = parse_ok("impl Point { fn new() -> Point { Point { x: 0, y: 0 } } }");

    expect_match!(&first_item(&ast).kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "Point");
        assert_eq!(impl_block.items.len(), 1);

        expect_match!(&impl_block.items[0].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "new");
            assert_eq!(func.sig.parameters.len(), 0);

            expect_match!(&func.sig.output, FunctionReturnTy::Ty(ret_ty) => {
                expect_ty!(str, **ret_ty, "Point");
            }, "Expected function to have a return type");

            // TODO: CHECK FUNCTION BODY CBA RN
        }, "Expected method");
    }, "Expected an impl block");
}

#[test]
fn parse_method_with_self() {
    let ast = parse_ok("impl Point { fn get_x(self) -> i32 { self.x } }");

    expect_match!(&first_item(&ast).kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "Point");
        assert_eq!(impl_block.items.len(), 1);

        expect_match!(&impl_block.items[0].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "get_x");
            assert_eq!(func.sig.parameters.len(), 1);

            expect_match!(&func.sig.output, FunctionReturnTy::Ty(ret_ty) => {
                expect_ty!(str, **ret_ty, "i32");
            }, "Expected function to have a return type");

            // TODO: CHECK FUNCTION BODY CBA RN
        }, "Expected method");
    }, "Expected an impl block");
}

#[test]
fn parse_enum_basic() {
    let ast = parse_ok("enum Test { A, B, C }");

    expect_match!(&first_item(&ast).kind, ItemKind::Enum(e) => {
        assert_eq!(e.ident.str(), "Test");
        assert_eq!(e.variants.len(), 3);

        assert_eq!(e.variants[0].ident.str(), "A");
        assert_eq!(e.variants[1].ident.str(), "B");
        assert_eq!(e.variants[2].ident.str(), "C");
    }, "Expected enum");
}

#[test]
fn parse_enum_with_struct_variant() {
    let ast = parse_ok("enum Test { Structure { a: i32, b: string } }");

    expect_match!(&first_item(&ast).kind, ItemKind::Enum(e) => {
        assert_eq!(e.ident.str(), "Test");
        assert_eq!(e.variants.len(), 1);

        assert_eq!(e.variants[0].ident.str(), "Structure");

        expect_match!(&e.variants[0].data, kitlang::ast::VariantData::Struct(fields) => {
            assert_eq!(fields.len(), 2);

            assert_eq!(fields[0].ident.str(), "a");
            expect_ty!(str, fields[0].ty, "i32");

            assert_eq!(fields[1].ident.str(), "b");
            expect_ty!(str, fields[1].ty, "string");
        }, "Expected struct variant data");
    }, "Expected enum with struct variant");
}

#[test]
fn parse_const_declaration() {
    let ast = parse_ok(r#"const GEEZER: f32 = "Gary";"#);

    expect_match!(&ast.items[0].kind, ItemKind::Const(c) => {
        assert_eq!(c.ident.str(), "GEEZER");
        
        expect_ty!(str, c.ty, "f32");
        expect_literal!(&c.expr, Literal::String("Gary".to_string()));
    }, "Expected const declaration");
}

#[test]
fn parse_use_statement() {
    let ast = parse_ok("use std::io;");

    expect_match!(&ast.items[0].kind, ItemKind::Use(_) => {
        // TODO: NO way im doing this rn
    }, "Expected use statement");
}

#[test]
fn parse_module_declaration() {
    let ast = parse_ok("mod test {}");

    expect_match!(&ast.items[0].kind, ItemKind::Mod(m) => {
        assert_eq!(m.ident.str(), "test");
    }, "Expected module declaration");
}

// TODO: Type parsing validation.

// Some more complex tests..

#[test]
fn parse_complete_program() {
    let ast = parse_ok(
        r#"
        pub struct Vec2 {
            x: f32,
            y: f32,
        }

        impl Vec2 {
            pub fn new(x: f32, y: f32) -> Vec2 {
                Vec2 { x: x, y: y }
            }

            pub fn distance(self, other: Vec2) -> f32 {
                let dx = other.x - self.x;
                let dy = other.y - self.y;
                dx * dx + dy * dy
            }
        }

        pub fn main() {
            let v1 = Vec2::new(0.0, 0.0);
            let v2 = Vec2::new(3.0, 4.0);
            let dist = v1.distance(v2);
        }
    "#,
    );

    assert_eq!(ast.items.len(), 3);

    let structure = &ast.items[0];
    let strcture_impl = &ast.items[1];
    let main_fn = &ast.items[2];

    expect_match!(&structure.kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Vec2");
        assert_eq!(s.fields.len(), 2);

        assert_eq!(s.fields[0].ident.str(), "x");
        expect_ty!(str, s.fields[0].ty, "f32");

        assert_eq!(s.fields[1].ident.str(), "y");
        expect_ty!(str, s.fields[1].ty, "f32");

        assert_eq!(structure.vis, Visibility::Public);
    }, "Expected first item to be struct Vec2");

    expect_match!(&strcture_impl.kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "Vec2");

        assert_eq!(impl_block.items.len(), 2);

        expect_match!(&impl_block.items[0].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "new");

            assert_eq!(func.sig.parameters.len(), 2);

            check_ident_pattern!(&func.sig.parameters[0].pattern, "x", Mutability::Immutable);
            expect_ty!(str, func.sig.parameters[0].ty, "f32");

            check_ident_pattern!(&func.sig.parameters[1].pattern, "y", Mutability::Immutable);
            expect_ty!(str, func.sig.parameters[1].ty, "f32");

            // TODO: Function body check.. rn? nahhh
        }, "Expected first impl item to be a function");

        expect_match!(&impl_block.items[1].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "distance");

            assert_eq!(func.sig.parameters.len(), 2);

            check_ident_pattern!(&func.sig.parameters[0].pattern, "self", Mutability::Immutable);
            assert!(matches!(func.sig.parameters[0].ty, Ty::This(..)));

            check_ident_pattern!(&func.sig.parameters[1].pattern, "other", Mutability::Immutable);
            expect_ty!(str, func.sig.parameters[1].ty, "Vec2");

            // TODO: Function body check
        }, "Expected second impl item to be a function");
    }, "Expected second item to be impl block");

    expect_match!(&main_fn.kind, ItemKind::Fn(func) => {
        assert_eq!(func.ident.str(), "main");
        assert_eq!(func.sig.parameters.len(), 0);
        assert_eq!(main_fn.vis, Visibility::Public);

        // TODO: Function body
    }, "Expected third item to be function");
}

#[test]
fn parse_nested_blocks() {
    let expr = wrap_and_parse_expr!(r#"{ { let x = 1; } }"#);

    let block = expect_block!(&expr);
    let inner_block = expect_block!(statement_any_expr(block, 0));
    expect_match!(&inner_block.statements[0].kind, StatementKind::Let(local) => {
        check_ident_pattern!(&local.pattern, "x", Mutability::Immutable);

        expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
            expect_literal!(init_expr, Literal::Integer(1));
        }, "Expected let statement to have an initialise expression");
    }, "Expected let statement inside inner block");
}

#[test]
fn parse_complex_expression() {
    let expr = wrap_and_parse_expr!("(1 + 2) * (3 - 4) / 5");

    // This should be:
    //      - lhs_1 / 5
    //      - lhs_1 = lhs_2 * lhs_3
    //      - lhs_2 = 1 + 2
    //      - lhs_3 = 3 - 4

    expect_match!(&expr.kind, ExpressionKind::BinaryOp(op, lhs, rhs) => {
        assert_eq!(*op, BinaryOpKind::Div);

        // lhs_1: (1 + 2) * (3 - 4)
        expect_match!(&lhs.kind, ExpressionKind::BinaryOp(op2, lhs2, rhs2) => {
            assert_eq!(*op2, BinaryOpKind::Mul);

            // lhs_2: 1 + 2
            expect_match!(&lhs2.kind, ExpressionKind::BinaryOp(op3, lhs3, rhs3) => {
                assert_eq!(*op3, BinaryOpKind::Add);

                expect_literal!(lhs3, Literal::Integer(1));
                expect_literal!(rhs3, Literal::Integer(2));
            }, "Expected addition expression as lhs of multiplication");

            // rhs_3: 3 - 4
            expect_match!(&rhs2.kind, ExpressionKind::BinaryOp(op3, lhs3, rhs3) => {
                assert_eq!(*op3, BinaryOpKind::Sub);

                expect_literal!(lhs3, Literal::Integer(3));
                expect_literal!(rhs3, Literal::Integer(4));
            }, "Expected addition expression as lhs of multiplication");
        }, "Expected multiplication expression as lhs of division");

        // rhs: 5
        expect_literal!(rhs, Literal::Integer(5));
    }, "Expected binary expression");
}

#[test]
fn parse_chained_method_calls() {
    let expr = wrap_and_parse_expr!("obj.method1().method2().method3()");

    expect_match!(&expr.kind, ExpressionKind::MethodCall(call3) => {
        assert_eq!(call3.method_ident.str(), "method3");
        assert_eq!(call3.args.len(), 0);

        expect_match!(&call3.target_expr.kind, ExpressionKind::MethodCall(call2) => {
            assert_eq!(call2.method_ident.str(), "method2");
            assert_eq!(call2.args.len(), 0);

            expect_match!(&call2.target_expr.kind, ExpressionKind::MethodCall(call1) => {
                assert_eq!(call1.method_ident.str(), "method1");
                assert_eq!(call1.args.len(), 0);

                expect_match!(&call1.target_expr.kind, ExpressionKind::IdentPath(path) => {
                    assert_eq!(path.to_string(), "obj");
                }, "Expected method1 target to be identifier");
            }, "Expected method call to method1");
        }, "Expected method call to method2");
    }, "Expected method call to method3");
}

#[test]
fn parse_multiple_items() {
    let ast = parse_ok(
        r#"
        fn a() {}
        fn b() {}
        struct C {}
        impl C {}
    "#,
    );
    assert_eq!(ast.items.len(), 4);

    expect_match!(&ast.items[0].kind, ItemKind::Fn(func) => {
        assert_eq!(func.ident.str(), "a");
    }, "Expected item to be function");

    expect_match!(&ast.items[1].kind, ItemKind::Fn(func) => {
        assert_eq!(func.ident.str(), "b");
    }, "Expected item to be function");

    expect_match!(&ast.items[2].kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "C");
    }, "Expected item to be struct");

    expect_match!(&ast.items[3].kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "C");
    }, "Expected item to be impl block");
}

// Errors..

// TODO: Validate they return the correct error. This is okay for now.

#[test]
fn parse_error_missing_semicolon() {
    let _err = wrap_and_parse!(parse, err, "let x = 5");
}

#[test]
fn parse_error_unclosed_brace() {
    let _err = parse!(parse, err, "fn main() { ");
}

#[test]
fn parse_error_invalid_token() {
    let _err = wrap_and_parse!(parse, err, "@");
}

#[test]
fn parse_error_self_outside_impl() {
    let _err = parse!(parse, err, "fn foo(self) {}");
}

// TODO: I can think of more test cases, but this is good enough for now.
// TODO: Need to add more expected fail tests.
// TODO: Need to add more type parsing tests.
// TODO: Need to add more nested expression parsing tests.
