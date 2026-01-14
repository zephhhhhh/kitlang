//! Kitlang is a language built for embedding/scripting purposes
//!
//! The language is inspired by rust and aims to be as similar to rust as possible with some
//! creative liberties taken.
//!
//! The structure of the project is also inspired by `rust` and `rustc`.

#![feature(iter_advance_by)]
#![feature(debug_closure_helpers)]
// Linting rules..
#![warn(clippy::pedantic)]
// Lint allows..
// Docs.. Temporary..
#![allow(clippy::too_long_first_doc_paragraph)]

use thiserror::Error;

pub mod ast;
pub mod compiler;
pub mod intermediate;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod profiling;
pub mod spanned_error;
pub mod token;

// Re-export macros for use in downstream crates.
pub use kitlang_macros as macros;

/// A small vector type used throughout Kitlang for performance where a small vector is expected.
pub type KitSmallVec<T> = smolvec::SmolVec<T>;

#[cfg(feature = "webasm")]
pub mod webasm;

// #[cfg(feature = "logging")]
// use log::*;

// Outward facing API..

/// Represents errors from different stages in the compilation process.
#[derive(Debug, Error)]
pub enum KitlangError {
    /// Errors originating from the parser stage.
    #[error("Parser Error: {0}")]
    ParserError(#[from] crate::parser::ParseError),
    /// Errors originating from the lowering stage.
    #[error("Lowering Error: {0}")]
    LoweringError(#[from] crate::intermediate::hir::LoweringError),
    #[error("Interpreter Error: {0}")]
    InterpreterError(#[from] crate::interpreter::errors::InterpreterError),
}

impl KitlangError {
    /// Format the error as an error message, given the source code string.
    #[inline]
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match self {
            Self::ParserError(err) => err.format_as_error_message(source_string),
            Self::LoweringError(err) => err.format_as_error_message(source_string),
            Self::InterpreterError(err) => err.format_as_error_message(source_string),
        }
    }
}

/// Result type for Kitlang operations with error variants for different stages in the compilation process.
pub type KitlangResult<T> = Result<T, KitlangError>;

/// Parse a given kitlang source code string into MIR representations.
/// This function injects the standard library, if you do not want that, use `parse_source_string_to_mir_no_std` instead.
/// # Errors
/// This function will return an error if any part of the parsing or lowering process fails.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
/// as which stage they originate from.
///
/// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
/// method on the `KitlangError` on the `Err` variant.
pub fn parse_source_string_to_mir(
    source: &str,
) -> KitlangResult<(intermediate::hir::ProgramMetaData, intermediate::mir::MIR)> {
    let final_source = inject_standard_library(source);

    let ast = crate::parser::parse_from_source(&final_source)?;
    let (meta_data, hir) = intermediate::hir::parse_ast_to_hir_processed(&ast)?;
    let mir = intermediate::mir::lower_hir_to_mir(&hir, &meta_data)?;
    Ok((meta_data, mir))
}

/// Parse a given kitlang source code string into MIR representations.
/// # Errors
/// This function will return an error if any part of the parsing or lowering process fails.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
/// as which stage they originate from.
///
/// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
/// method on the `KitlangError` on the `Err` variant.
pub fn parse_source_string_to_mir_no_std(
    source: &str,
) -> KitlangResult<(intermediate::hir::ProgramMetaData, intermediate::mir::MIR)> {
    let ast = crate::parser::parse_from_source(source)?;
    let (meta_data, hir) = intermediate::hir::parse_ast_to_hir_processed(&ast)?;
    let mir = intermediate::mir::lower_hir_to_mir(&hir, &meta_data)?;
    Ok((meta_data, mir))
}

/// Inject the standard library code into a provided source code string.
#[inline]
#[must_use]
pub fn inject_standard_library(source: &str) -> String {
    const CORE_LIB: &str = include_str!("../kit-std/core.purr");
    const INTRINSICS_LIB: &str = include_str!("../kit-std/intrinsics.purr");

    let mut source_with_std =
        String::with_capacity(source.len() + CORE_LIB.len() + INTRINSICS_LIB.len() + 2);
    source_with_std.push_str(source);
    source_with_std.push('\n');
    source_with_std.push_str(CORE_LIB);
    source_with_std.push('\n');
    source_with_std.push_str(INTRINSICS_LIB);

    source_with_std
}

/// Execute a given kitlang source code string with the MIR interpreter, using the provided native functions.
/// The standard library is injected into the program with this function, if you do not want this for some reason,
/// use `execute_source_string_no_std` instead.
/// If `time_execution` is `true`, the different stages of execution will be timed and printed
/// # Errors
/// This function will return an error if any part of the parsing, lowering or execution process fails.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
/// as which stage they originate from.
///
/// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
/// method on the `KitlangError` on the `Err` variant.
pub fn execute_source_string(
    source: &str,
    native_functions: crate::interpreter::mir_interpreter::RegisterNativeFns,
    time_execution: bool,
) -> KitlangResult<crate::interpreter::mir_interpreter::Value> {
    use crate::compiler::Compiler;
    use crate::interpreter::mir_interpreter::execute_mir;

    let mut compiler = Compiler::new(source).profile_stages(time_execution);
    let mir = compiler.compile_to_mir()?;

    execute_mir(
        mir,
        compiler.context.meta(),
        native_functions,
        time_execution,
    )
}

/// Execute a given kitlang source code string with the MIR interpreter, using the provided native functions.
/// The standard library is injected into the program with this function, if you do not want this for some reason,
/// use `execute_source_string_no_std` instead.
/// If `time_execution` is `true`, the different stages of execution will be timed and printed
/// `no_io` indicates that the standard library will be injected, by the input/output functions are not registered,
/// and should be registered manually in `native_functions`.
/// # Errors
/// This function will return an error if any part of the parsing, lowering or execution process fails.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
/// as which stage they originate from.
///
/// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
/// method on the `KitlangError` on the `Err` variant.
pub fn execute_source_string_no_io(
    source: &str,
    native_functions: crate::interpreter::mir_interpreter::RegisterNativeFns,
    time_execution: bool,
) -> KitlangResult<crate::interpreter::mir_interpreter::Value> {
    use crate::compiler::Compiler;
    use crate::interpreter::mir_interpreter::execute_mir_no_io;

    let mut compiler = Compiler::new(source).profile_stages(time_execution);
    let mir = compiler.compile_to_mir()?;

    execute_mir_no_io(
        mir,
        compiler.context.meta(),
        native_functions,
        time_execution,
    )
}

/// Execute a given kitlang source code string with the MIR interpreter, using the provided native functions.
/// If `time_execution` is `true`, the different stages of execution will be timed and printed.
/// # Errors
/// This function will return an error if any part of the parsing, lowering or execution process fails.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures, as well
/// as which stage they originate from.
///
/// These errors can be formatted into user-friendly error messages using the `format_as_error_message`
/// method on the `KitlangError` on the `Err` variant.
pub fn execute_source_string_no_std(
    source: &str,
    native_functions: crate::interpreter::mir_interpreter::RegisterNativeFns,
    time_execution: bool,
) -> KitlangResult<crate::interpreter::mir_interpreter::Value> {
    use crate::compiler::Compiler;
    use crate::interpreter::mir_interpreter::execute_mir_no_intrinsics;

    let mut compiler = Compiler::new(source)
        .profile_stages(time_execution)
        .no_stdlib(true);
    let mir = compiler.compile_to_mir()?;

    execute_mir_no_intrinsics(
        mir,
        compiler.context.meta(),
        native_functions,
        time_execution,
    )
}

/// Call this function to initialise logging, if the `logging` feature is enabled.
pub fn init_logging() {
    #[cfg(all(feature = "logging", not(feature = "webasm")))]
    fn setup_logging() {
        simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
            log::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )])
        .expect("Failed to initialize logging");
    }
    #[cfg(all(feature = "logging", not(feature = "webasm")))]
    setup_logging();
}
