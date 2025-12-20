#![allow(dead_code)]

use kitlang::{KitlangError, ast::ASTRoot, intermediate::hir::HLIR, prelude::Compiler};

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
        parse_lower_hir_no_res_ok(main!($test_src))
    };
    (hir, err, $test_src:literal) => {
        parse_lower_hir_no_res_err(main!($test_src))
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
        parse_lower_hir_no_res_ok($test_src)
    };
    (hir, err, $test_src:expr) => {
        parse_lower_hir_no_res_err($test_src)
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
    let mut compiler = Compiler::new(source).no_stdlib(true);
    compiler.parse_ast().expect("Parse should succeed")
}
/// Parse the source code, expect it to fail, and return the error.
pub fn parse_err(source: &str) -> KitlangError {
    let mut compiler = Compiler::new(source).no_stdlib(true);
    compiler.parse_ast().expect_err("Parse should fail")
}

/// Parse the source code, expect it to succeed, and return the AST.
pub fn parse_lower_hir_no_res_ok(source: &str) -> HLIR {
    let mut compiler = Compiler::new(source).no_stdlib(true);
    compiler
        .compile_to_hir_no_resolve()
        .expect("HIR lowering should succeed")
}
/// Parse the source code, expect it to fail, and return the error.
pub fn parse_lower_hir_no_res_err(source: &str) -> KitlangError {
    let mut compiler = Compiler::new(source).no_stdlib(true);
    compiler
        .compile_to_hir_no_resolve()
        .expect_err("HIR lowering should fail")
}
