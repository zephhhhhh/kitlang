#![allow(dead_code)]

use kitlang::{
    KitlangError,
    ast::ASTRoot,
    intermediate::{
        hir::{HLIR, LoweringError, LoweringErrorKind, lower_ast_to_hir},
        resolver::errors::ResolverError,
    },
    parser::{ParseError, parse_from_source},
    prelude::{Compiler, ProgramMetaData},
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
    ($test_src:literal, $ret_ty:literal) => {
        concat!("fn main() -> ", $ret_ty, " { ", $test_src, " }")
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

/// Parse the source code, expect it to succeed, and return the [`HLIR`].
pub fn parse_lower_hir_ok(source: &str) -> HLIR {
    let ast = parse_ok(source);
    lower_ast_to_hir(&ast).expect("HIR lowering should succeed")
}
/// Parse the source code, expect it to fail, and return the parse error.
pub fn parse_ok_lower_hir_err(source: &str) -> LoweringError {
    let ast = parse_ok(source);
    lower_ast_to_hir(&ast).expect_err("HIR lowering should fail")
}

/// Parse the source code, resolve it, expect it to succeed, and return the [`HLIR`].
pub fn parse_lower_hir_full_ok(source: &str) -> (ProgramMetaData, HLIR) {
    let mut compiler = Compiler::new(source).no_stdlib(true);

    let hlir = compiler
        .compile_to_hir()
        .expect("HIR lowering should succeed");

    (compiler.context.meta, hlir)
}
/// Parse the source code, expect it to fail, and return the parse error.
pub fn parse_lower_hir_full_err(source: &str) -> ResolverError {
    let mut compiler = Compiler::new(source).no_stdlib(true);
    let error = compiler
        .compile_to_hir()
        .expect_err("HIR lowering should fail");
    match error {
        KitlangError::LoweringError(lower_err) => match lower_err.error_kind {
            LoweringErrorKind::ResolverError(resolver_err) => resolver_err,
            _ => panic!("Expected resolver error"),
        },
        _ => panic!("Expected resolver error"),
    }
}
