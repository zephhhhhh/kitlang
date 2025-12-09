use kitlang::{
    ast::{
        ASTRoot, BinaryOpKind, Block, Expression, ExpressionKind, FunctionReturnTy, Item, ItemKind,
        Literal, LocalKind, Mutability, Statement, StatementKind, Ty, UnaryOpKind, Visibility,
    },
    parser::{ParseError, parse_from_source},
};

// Helper functions/macros..

/// Wraps the given source code in a `main` function.
///
/// Essentially generates:
/// ```ignore
/// fn main() {
///    // $test_src
/// }
/// ```
macro_rules! main_fn {
    ($test_src:literal) => {
        concat!("fn main() { ", $test_src, " }")
    };
}
macro_rules! wrap_expr_and_parse {
    ($test_src:literal) => {
        parse_first_expr(main_fn!($test_src))
    };
}
macro_rules! wrap_statement_and_parse {
    ($test_src:literal) => {
        parse_first_statement(main_fn!($test_src))
    };
}
macro_rules! expect_match {
    ($expr:expr, $pat:pat => $out:expr, $msg:expr) => {
        match $expr {
            $pat => $out,
            _ => panic!("{}", $msg),
        }
    };
}
macro_rules! expect_literal {
    ($expr:expr, $pat:pat => $out:expr, $msg:expr) => { expect_match!($expr, ExpressionKind::Literal($pat) => $out, $msg) };
}

/// Parse the source code, expect it to succeed, and return the AST.
fn parse_ok(source: &str) -> kitlang::ast::ASTRoot {
    parse_from_source(source).expect("Parse should succeed")
}
/// Parse the source code, expect it to fail, and return the parse error.
fn parse_err(source: &str) -> ParseError {
    parse_from_source(source).expect_err("Parse should fail")
}

/// Parse the source code, expect it to succeed, and return the first expression of the first function.
fn parse_first_expr(source: &str) -> Expression {
    let ast = parse_ok(source);
    get_first_expr(&ast).clone()
}

/// Parse the source code, expect it to succeed, and return the first statement of the first function.
fn parse_first_statement(source: &str) -> Statement {
    let ast = parse_ok(source);
    get_first_statement(&ast).clone()
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


// Let the testing begin...

#[test]
fn parse_literal_integers() {
    let ast = parse_ok(main_fn!("42"));
    let expr = get_first_expr(&ast);

    expect_literal!(&expr.kind, Literal::Integer(val) => {
        assert_eq!(*val, 42);
    }, "Expected integer literal");
}

#[test]
fn parse_literal_floats() {
    let expr = parse_first_expr("fn main() { 7.12023; }");

    expect_literal!(&expr.kind, Literal::Float(val) => {
        assert_eq!(*val, 7.12023);
    }, "Expected float literal");
}

#[test]
fn parse_literal_strings() {
    let expr = parse_first_expr(r#"fn main() { "hello world"; }"#);

    expect_literal!(&expr.kind, Literal::String(val) => {
        assert_eq!(val, "hello world");
    }, "Expected string literal");
}
#[test]
fn parse_literal_booleans() {
    let ast = parse_ok("fn main() { true; false; }");
    let true_expr = get_first_expr(&ast);
    let false_expr = get_nth_expr(&ast, 1);

    expect_literal!(&true_expr.kind, Literal::Boolean(val) => {
        assert!(val);
    }, "Expected boolean literal");

    expect_literal!(&false_expr.kind, Literal::Boolean(val) => {
        assert!(!val);
    }, "Expected boolean literal");
}

fn run_binary_op_test_cases(test_cases: &[(&str, BinaryOpKind)]) {
    for (source, expected_op) in test_cases {
        let expr = parse_first_expr(source);

        expect_match!(&expr.kind, ExpressionKind::BinaryOp(op, _, _) => {
            assert_eq!(*op, *expected_op, "Failed for: {}", source);
        }, format!("Expected binary operation for: {}", source));
    }
}

#[test]
fn parse_binary_ops_arithmetic() {
    let test_cases = vec![
        ("fn main() { 1 + 2; }", BinaryOpKind::Add),
        ("fn main() { 3 - 4; }", BinaryOpKind::Sub),
        ("fn main() { 5 * 6; }", BinaryOpKind::Mul),
        ("fn main() { 7 / 8; }", BinaryOpKind::Div),
        ("fn main() { 9 % 10; }", BinaryOpKind::Mod),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_binary_ops_comparison() {
    let test_cases = vec![
        ("fn main() { 1 == 2; }", BinaryOpKind::Equal),
        ("fn main() { 1 != 2; }", BinaryOpKind::NotEqual),
        ("fn main() { 1 < 2; }", BinaryOpKind::LessThan),
        ("fn main() { 1 > 2; }", BinaryOpKind::GreaterThan),
        ("fn main() { 1 <= 2; }", BinaryOpKind::LessThanOrEqual),
        ("fn main() { 1 >= 2; }", BinaryOpKind::GreaterThanOrEqual),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_binary_ops_logical() {
    let test_cases = vec![
        ("fn main() { true && false; }", BinaryOpKind::And),
        ("fn main() { true || false; }", BinaryOpKind::Or),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_binary_ops_bitwise() {
    let test_cases = vec![
        ("fn main() { 5 & 3; }", BinaryOpKind::BitwiseAND),
        ("fn main() { 5 | 3; }", BinaryOpKind::BitwiseOR),
        ("fn main() { 5 ^ 3; }", BinaryOpKind::BitwiseXOR),
        ("fn main() { 5 << 2; }", BinaryOpKind::ShiftLeft),
        ("fn main() { 5 >> 2; }", BinaryOpKind::ShiftRight),
    ];

    run_binary_op_test_cases(&test_cases);
}

#[test]
fn parse_unary_ops() {
    let test_cases = vec![
        ("fn main() { -a; }", UnaryOpKind::Negate),
        ("fn main() { !true; }", UnaryOpKind::Not),
        ("fn main() { *ptr; }", UnaryOpKind::Dereference),
    ];

    for (source, expected_op) in test_cases {
        let expr = parse_first_expr(source);

        expect_match!(&expr.kind, ExpressionKind::UnaryOp(op, _) => {
            assert_eq!(*op, expected_op, "Failed for: {}", source);
        }, format!("Expected unary operation for: {}", source));
    }
}

#[test]
fn parse_block_expression() {
    let expr = parse_first_expr("fn main() { { 42 } }");

    expect_match!(&expr.kind, ExpressionKind::Block(b) => {
        let inside_block_expr = statement_any_expr(b, 0);
        expect_literal!(&inside_block_expr.kind, Literal::Integer(val) => {
            assert_eq!(*val, 42);
        }, "Expected integer literal inside block");
    }, "Expected block expression");
}

#[test]
fn parse_if_expression() {
    let expr = parse_first_expr("fn main() { if true { 1 } }");

    expect_match!(&expr.kind, ExpressionKind::If(cond, block, else_block) => {
        expect_literal!(&cond.kind, Literal::Boolean(val) => {
            assert!(val);
        }, "Expected boolean literal as if condition");

        let inside_block_expr = statement_any_expr(block, 0);
        expect_literal!(&inside_block_expr.kind, Literal::Integer(val) => {
            assert_eq!(*val, 1);
        }, "Expected integer literal inside if block");

        assert!(else_block.is_none(), "Expected no else block");
    }, "Expected if expression");
}

#[test]
fn parse_if_else_expression() {
    let expr = parse_first_expr("fn main() { if true { 1 } else { 2 } }");

    expect_match!(&expr.kind, ExpressionKind::If(cond, block, else_block) => {
        expect_literal!(&cond.kind, Literal::Boolean(val) => {
            assert!(val);
        }, "Expected boolean literal as if condition");

        expect_literal!(&statement_any_expr(block, 0).kind, Literal::Integer(val) => {
            assert_eq!(*val, 1);
        }, "Expected integer literal inside if block");

        expect_match!(&else_block.as_ref().expect("Expected else block").kind, ExpressionKind::Block(b) => {
            expect_literal!(&statement_any_expr(b, 0).kind, Literal::Integer(val) => {
                assert_eq!(*val, 2);
            }, "Expected integer literal inside else block");
        }, "Expected else block expression to be a block");
    }, "Expected if expression");
}

#[test]
fn parse_nested_if_statements() {
    todo!();
}

#[test]
fn parse_while_loop() {
    let expr = parse_first_expr("fn main() { while true { 1; } }");

    expect_match!(&expr.kind, ExpressionKind::While(cond, body) => {
        expect_literal!(&cond.kind, Literal::Boolean(val) => {
            assert!(val);
        }, "Expected boolean literal as while condition");

        let inside_body_expr = statement_any_expr(body, 0);
        expect_literal!(&inside_body_expr.kind, Literal::Integer(val) => {
            assert_eq!(*val, 1);
        }, "Expected integer literal inside while body");
    }, "Expected while loop");
}

#[test]
fn parse_return_expression() {
    let expr = parse_first_expr("fn main() { return 42; }");

    expect_match!(&expr.kind, ExpressionKind::Return(ret_expr) => {
        expect_literal!(&ret_expr.as_ref().expect("Expected return statement has expression.").kind, Literal::Integer(val) => {
            assert_eq!(*val, 42);
        }, "Expected integer literal in return expression");
    }, "Expected return expression");
}

#[test]
fn parse_return_no_expression() {
    let expr = parse_first_expr("fn main() { return; }");

    expect_match!(&expr.kind, ExpressionKind::Return(ret_expr) => {
        assert!(ret_expr.is_none(), "Expected return statement with no expression");
    }, "Expected return expression");
}

#[test]
fn parse_function_call() {
    let expr = parse_first_expr("fn main() { test(); }");

    expect_match!(&expr.kind, ExpressionKind::Call(func, args) => {
        assert_eq!(args.len(), 0);

        expect_match!(&func.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), "test");
        }, "Expected function call path");
    }, "Expected function call");
}

#[test]
fn parse_function_call_with_args() {
    let expr = wrap_expr_and_parse!(r#"test(1, 2.2, "hello")"#);

    expect_match!(&expr.kind, ExpressionKind::Call(func, args) => {
        assert_eq!(args.len(), 3);

        expect_match!(&func.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), "test");
        }, "Expected function call path");

        expect_literal!(&args[0].kind, Literal::Integer(val) => {
            assert_eq!(*val, 1);
        }, "Expected argument to be integer literal 1");
        expect_literal!(&args[1].kind, Literal::Float(val) => {
            assert_eq!(*val, 2.2);
        }, "Expected argument to be float literal 2.2");
        expect_literal!(&args[2].kind, Literal::String(val) => {
            assert_eq!(val, "hello");
        }, "Expected argument to be string literal `hello`");
    }, "Expected function call");
}

#[test]
fn parse_method_call() {
    let expr = wrap_expr_and_parse!("obj.method()");

    expect_match!(&expr.kind, ExpressionKind::MethodCall(call_info) => {
        expect_match!(&call_info.target_expr.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), "obj");
        }, "Expected method call target to be an identifier path");

        assert_eq!(call_info.method_ident.str(), "method");

        assert_eq!(call_info.args.len(), 0);
    }, "Expected method call");
}

#[test]
fn parse_method_call_with_args() {
    todo!();
}

#[test]
fn parse_struct_initialisation() {
    let expr = wrap_expr_and_parse!("Point { x: 1, y: 2 }");

    expect_match!(&expr.kind, ExpressionKind::StructInit(init) => {
        assert_eq!(init.fields.len(), 2);

        assert_eq!(init.fields[0].ident.str(), "x");
        expect_literal!(&init.fields[0].expr.kind, Literal::Integer(val) => {
            assert_eq!(*val, 1);
        }, "Expected field `x` to have integer literal value 1");

        assert_eq!(init.fields[1].ident.str(), "y");
        expect_literal!(&init.fields[1].expr.kind, Literal::Integer(val) => {
            assert_eq!(*val, 2);
        }, "Expected field `y` to have integer literal value 2");
    }, "Expected struct initialisation");
}

#[test]
fn parse_member_access() {
    let expr = wrap_expr_and_parse!("obj.field");

    expect_match!(&expr.kind, ExpressionKind::FieldAccess(target, field_ident) => {
        expect_match!(&target.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), "obj");
        }, "Expected member access target to be an identifier path");

        assert_eq!(field_ident.str(), "field");
    }, "Expected member access");
}

#[test]
fn parse_assignment() {
    let expr = wrap_expr_and_parse!("x = 5");

    expect_match!(&expr.kind, ExpressionKind::Assign(target, value) => {
        expect_match!(&target.kind, ExpressionKind::IdentPath(path) => {
            assert_eq!(path.to_string(), "x");
        }, "Expected assignment target to be an identifier path");

        expect_literal!(&value.kind, Literal::Integer(val) => {
            assert_eq!(*val, 5);
        }, "Expected assignment value to be integer literal 5");
    }, "Expected assignment expression");
}

#[test]
fn parse_let_statement_basic() {
    let stmt = wrap_statement_and_parse!("let x: i32 = 5;");

    expect_match!(&stmt.kind, StatementKind::Let(local) => {
        assert_eq!(local.ident.str(), "x");
        assert!(local.mutable == Mutability::Immutable);

        expect_match!(&local.ty, kitlang::ast::Ty::Type(path) => {
            assert_eq!(path.to_string(), "i32");
        }, "Expected let statement type to be i32");

        expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
            expect_literal!(&init_expr.kind, Literal::Integer(val) => {
                assert_eq!(*val, 5);
            }, "Expected let statement init expression to be integer literal 5");
        }, "Expected let statement to have an initialise expression");
    }, "Expected let statement");
}

#[test]
fn parse_let_statement_mutable() {
    let stmt = wrap_statement_and_parse!(
        r#"let mut y: string = "TESTING TESTING HAHAHAHAHAHAHHAHAHAHAHHA";"#
    );

    expect_match!(&stmt.kind, StatementKind::Let(local) => {
        assert_eq!(local.ident.str(), "y");
        assert!(local.mutable == Mutability::Mutable);

        expect_match!(&local.ty, Ty::Type(path) => {
            assert_eq!(path.to_string(), "string");
        }, "Expected let statement type to be string");

        expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
            expect_literal!(&init_expr.kind, Literal::String(val) => {
                assert_eq!(val, "TESTING TESTING HAHAHAHAHAHAHHAHAHAHAHHA");
            }, "Expected let statement init expression to be string literal");
        }, "Expected let statement to have an initialise expression");
    }, "Expected let statement");
}

#[test]
fn parse_empty_statement() {
    let stmt = wrap_statement_and_parse!(";");
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

    assert_eq!(func.sig.parameters[0].ident.str(), "a");
    assert_eq!(func.sig.parameters[0].ty.get_type_ident().expect("First param has no type ident"), "i32");
    
    assert_eq!(func.sig.parameters[1].ident.str(), "b");
    assert_eq!(func.sig.parameters[1].ty.get_type_ident().expect("Second param has no type ident"), "f32");
}

#[test]
fn parse_function_with_return_type() {
    let ast = parse_ok("pub fn test() -> i32 {}");
    let func = first_as_function(&ast);

    assert_eq!(func.ident.str(), "test");
    assert_eq!(ast.items[0].vis, Visibility::Public);
    assert_eq!(func.sig.parameters.len(), 0);

    expect_match!(&func.sig.output, FunctionReturnTy::Ty(ret_ty) => {
        assert_eq!(ret_ty.get_type_ident().expect("Return type has no type ident"), "i32");
    }, "Expected function return type to be i32");
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
        assert_eq!(s.fields[0].ty.get_type_ident().expect("First field has no type ident"), "i32");
        
        assert_eq!(s.fields[1].ident.str(), "y");
        assert_eq!(s.fields[1].ty.get_type_ident().expect("Second field has no type ident"), "i32");
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
        assert_eq!(s.fields[0].ty.get_type_ident().expect("First field has no type ident"), "f32");
        
        assert_eq!(s.fields[1].ident.str(), "y");
        assert_eq!(s.fields[1].ty.get_type_ident().expect("Second field has no type ident"), "f32");

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
                assert_eq!(ret_ty.get_type_ident().expect("Return type has no type ident"), "Point");
            }, "Expected function return type to be Point");

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
                assert_eq!(ret_ty.get_type_ident().expect("Return type has no type ident"), "i32");
            }, "Expected function return type");

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
    let ast = parse_ok("enum Test { Structre { a: i32, b: String } }");

    expect_match!(&first_item(&ast).kind, ItemKind::Enum(e) => {
        assert_eq!(e.ident.str(), "Test");
        assert_eq!(e.variants.len(), 1);

        assert_eq!(e.variants[0].ident.str(), "Structre");

        expect_match!(&e.variants[0].data, kitlang::ast::VariantData::Struct(fields) => {
            assert_eq!(fields.len(), 2);

            assert_eq!(fields[0].ident.str(), "a");
            assert_eq!(fields[0].ty.get_type_ident().expect("First field has no type ident"), "i32");

            assert_eq!(fields[1].ident.str(), "b");
            assert_eq!(fields[1].ty.get_type_ident().expect("Second field has no type ident"), "String");
        }, "Expected struct variant data");
    }, "Expected enum with struct variant");
}

#[test]
fn parse_const_declaration() {
    let ast = parse_ok(r#"const GEEZER: f32 = "Gary";"#);

    expect_match!(&ast.items[0].kind, ItemKind::Const(c) => {
        assert_eq!(c.ident.str(), "GEEZER");
        
        expect_match!(&c.ty, Ty::Type(path) => {
            assert_eq!(path.to_string(), "f32");
        }, "Expected const type to be f32");

        expect_literal!(&c.expr.kind, Literal::String(s) => {
            assert_eq!(*s, "Gary");
        }, "Expected const value to be string");
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
    let ast = parse_ok(r#"
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
    "#);

    assert_eq!(ast.items.len(), 3);

    let structure = &ast.items[0];
    let strcture_impl = &ast.items[1];
    let main_fn = &ast.items[2];
    
    expect_match!(&structure.kind, ItemKind::Struct(s) => {
        assert_eq!(s.ident.str(), "Vec2");

        assert_eq!(s.fields.len(), 2);

        assert_eq!(s.fields[0].ident.str(), "x");
        assert_eq!(s.fields[0].ty.get_type_ident().expect("First field has no type ident"), "f32");

        assert_eq!(s.fields[1].ident.str(), "y");
        assert_eq!(s.fields[1].ty.get_type_ident().expect("Second field has no type ident"), "f32");

        assert_eq!(structure.vis, Visibility::Public);
    }, "Expected first item to be struct Vec2");

    expect_match!(&strcture_impl.kind, ItemKind::Impl(impl_block) => {
        assert_eq!(impl_block.target_path.to_string(), "Vec2");

        assert_eq!(impl_block.items.len(), 2);

        expect_match!(&impl_block.items[0].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "new");

            assert_eq!(func.sig.parameters.len(), 2);

            assert_eq!(func.sig.parameters[0].ident.str(), "x");
            assert_eq!(func.sig.parameters[0].ty.get_type_ident().expect("First param has no type ident"), "f32");

            assert_eq!(func.sig.parameters[1].ident.str(), "y");
            assert_eq!(func.sig.parameters[1].ty.get_type_ident().expect("Second param has no type ident"), "f32");

            // TODO: Function body check.. rn? nahhh
        }, "Expected first impl item to be a function");

        expect_match!(&impl_block.items[1].kind, ItemKind::Fn(func) => {
            assert_eq!(func.ident.str(), "distance");

            assert_eq!(func.sig.parameters.len(), 2);

            assert_eq!(func.sig.parameters[0].ident.str(), "self");
            assert!(matches!(func.sig.parameters[0].ty, Ty::This(..)));

            assert_eq!(func.sig.parameters[1].ident.str(), "other");
            assert_eq!(func.sig.parameters[1].ty.get_type_ident().expect("Second param has no type ident"), "Vec2");

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
    let expr = wrap_expr_and_parse!(r#"
            {
                {
                    let x = 1;
                }
            }
    "#);

    expect_match!(&expr.kind, ExpressionKind::Block(outer_block) => {
        expect_match!(&statement_any_expr(outer_block, 0).kind, ExpressionKind::Block(inner_block) => {
            expect_match!(&inner_block.statements[0].kind, StatementKind::Let(local) => {
                assert_eq!(local.ident.str(), "x");

                expect_match!(&local.kind, LocalKind::Initialise(init_expr) => {
                    expect_literal!(&init_expr.kind, Literal::Integer(val) => {
                        assert_eq!(*val, 1);
                    }, "Expected let statement init expression to be integer literal 1");
                }, "Expected let statement to have an initialise expression");
            }, "Expected let statement inside inner block");
        }, "Expected block inside outer block");
    }, "Expected outer block");
}

#[test]
fn parse_complex_expression() {
    let expr = wrap_expr_and_parse!("(1 + 2) * (3 - 4) / 5");

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

                expect_literal!(&lhs3.kind, Literal::Integer(val) => {
                    assert_eq!(*val, 1);
                }, "Expected integer literal 1 as lhs of addition");

                expect_literal!(&rhs3.kind, Literal::Integer(val) => {
                    assert_eq!(*val, 2);
                }, "Expected integer literal 2 as rhs of addition");
            }, "Expected addition expression as lhs of multiplication");

            // rhs_3: 3 - 4
            expect_match!(&rhs2.kind, ExpressionKind::BinaryOp(op3, lhs3, rhs3) => {
                assert_eq!(*op3, BinaryOpKind::Sub);

                expect_literal!(&lhs3.kind, Literal::Integer(val) => {
                    assert_eq!(*val, 3);
                }, "Expected integer literal 1 as lhs of addition");

                expect_literal!(&rhs3.kind, Literal::Integer(val) => {
                    assert_eq!(*val, 4);
                }, "Expected integer literal 2 as rhs of addition");
            }, "Expected addition expression as lhs of multiplication");
        }, "Expected multiplication expression as lhs of division");

        // rhs: 5
        expect_literal!(&rhs.kind, Literal::Integer(val) => {
            assert_eq!(*val, 5);
        }, "Expected integer literal 5 as rhs of division");
    }, "Expected binary expression");
}

#[test]
fn parse_chained_method_calls() {
    let expr = wrap_expr_and_parse!("obj.method1().method2().method3()");

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
    let ast = parse_ok(r#"
        fn a() {}
        fn b() {}
        struct C {}
        impl C {}
    "#);
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
    let _err = parse_err("fn main() { let x = 5 }");
}

#[test]
fn parse_error_unclosed_brace() {
    let _err = parse_err("fn main() { ");
}

#[test]
fn parse_error_invalid_token() {
    let _err = parse_err("fn main() { @ }");
}

#[test]
fn parse_error_self_outside_impl() {
    let _err = parse_err("fn foo(self) {}");
}

// TODO: I can think of more test cases, but this is good enough for now.
// TODO: Need to add more expected fail tests.
// TODO: Need to add more type parsing tests.
// TODO: Need to add more nested expression parsing tests.