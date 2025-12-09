#![allow(dead_code)]

use kitlang::{
    ast::ASTRoot,
    intermediate::hir::{HLIR, LoweringError, lower_ast_to_hir},
    parser::{ParseError, parse_from_source},
};

/// Wraps the given source code in a `main` function.
///
/// Essentially generates:
/// ```ignore
/// fn main() {
///    test_src
/// }
/// ```
#[macro_export]
macro_rules! main {
    ($test_src:literal) => {
        concat!("fn main() { ", $test_src, " }")
    };
}
#[macro_export]
macro_rules! wrap_and_parse {
    (parse, $test_src:literal) => {
        parse_ok(main!($test_src))
    };
    (parse, err, $test_src:literal) => {
        parse_err(main!($test_src))
    };
    (hir, $test_src:literal) => {
        parse_lower_hir_ok(main!($test_src))
    };
    (hir, err, $test_src:literal) => {
        parse_lower_hir_err(main!($test_src))
    };
}
#[macro_export]
macro_rules! parse {
    (parse, $test_src:expr) => {
        parse_ok($test_src)
    };
    (parse, err, $test_src:expr) => {
        parse_err($test_src)
    };
    (hir, $test_src:expr) => {
        parse_lower_hir_ok($test_src)
    };
    (hir, err, $test_src:expr) => {
        parse_lower_hir_err($test_src)
    };
}

#[macro_export]
macro_rules! expect_match {
    ($expr:expr, $pat:pat => $out:expr, $($fmta:tt)*) => {
        match $expr {
            $pat => $out,
            _ => panic!("{}", format!($($fmta)*)),
        }
    };
}

/// Parse the source code, expect it to succeed, and return the AST.
pub fn parse_ok(source: &str) -> ASTRoot {
    parse_from_source(source).expect("Parse should succeed")
}
/// Parse the source code, expect it to fail, and return the parse error.
pub fn parse_err(source: &str) -> ParseError {
    parse_from_source(source).expect_err("Parse should fail")
}

/// Parse the source code, expect it to succeed, and return the AST.
pub fn parse_lower_hir_ok(source: &str) -> HLIR {
    let ast = parse_ok(source);
    lower_ast_to_hir(&ast).expect("HIR lowering should succeed")
}
/// Parse the source code, expect it to fail, and return the parse error.
pub fn parse_ok_lower_hir_err(source: &str) -> LoweringError {
    let ast = parse_ok(source);
    lower_ast_to_hir(&ast).expect_err("HIR lowering should fail")
}
