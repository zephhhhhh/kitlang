#![feature(iter_advance_by)]
#![feature(debug_closure_helpers)]
#![feature(f16)]

//! Kitlang is a language built for embedding/scripting purposes
//!
//! The language is inspired by rust and aims to be as similar to rust as possible with some
//! creative liberties taken.
//!
//! The structure of the project is also inspired by `rust` and `rustc`.

use thiserror::Error;

pub mod ast;
pub mod intermediate;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod profiling;
pub mod spanned_error;
pub mod token;

#[cfg(feature = "webasm")]
pub mod webasm;

#[cfg(feature = "logging")]
use log::*;

// Outward facing API..

/// Represents errors from different stages in the compilation process.
/// TODO: Expand this to include type checking and MIR errors.
#[derive(Debug, Error)]
pub enum KitlangError {
    #[error("Parser Error: {0}")]
    ParserError(#[from] crate::parser::ParseError),
    #[error("Lowering Error: {0}")]
    LoweringError(#[from] crate::intermediate::hir::LoweringError),
    #[error("Execution ended unexpectedly.")]
    ExecutionEndedUnexpectedly,
    #[error("Failed to find entry point 'main' function.")]
    FailedToFindEntryPoint,
}

impl KitlangError {
    /// Format the error as an error message, given the source code string.
    #[inline]
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match self {
            KitlangError::ParserError(err) => err.format_as_error_message(source_string),
            KitlangError::LoweringError(err) => err.format_as_error_message(source_string),
            KitlangError::ExecutionEndedUnexpectedly => {
                "Error: Execution ended unexpectedly.".to_string()
            }
            KitlangError::FailedToFindEntryPoint => {
                "Error: Failed to find entry point 'main' function.".to_string()
            }
        }
    }
}

/// Result type for Kitlang operations with error variants for different stages in the compilation process.
pub type KitlangResult<T> = Result<T, KitlangError>;

/// Parse a given kitlang source code string into MIR representations.
pub fn parse_source_string_to_mir(
    source: &str,
) -> KitlangResult<(intermediate::hir::ProgramMetaData, intermediate::mir::MIR)> {
    let ast = crate::parser::parse_from_source(source)?;
    let (meta_data, hir) = intermediate::hir::parse_ast_to_hir_processed(&ast)?;
    let mir = intermediate::mir::lower_hir_to_mir(&hir, &meta_data)?;
    Ok((meta_data, mir))
}

/// Execute a given kitlang source code string with the MIR interpreter, using the provided native functions.
/// If `time_execution` is `true`, the different stages of execution will be timed and printed
pub fn execute_source_string(
    source: &str,
    native_functions: crate::interpreter::mir_interpreter::RegisterNativeFns,
    time_execution: bool,
) -> KitlangResult<crate::interpreter::mir_interpreter::Value> {
    use crate::interpreter::mir_interpreter::execute_mir_with_native_functions;

    let (meta_data, mir) = if time_execution {
        crate::profiling::print_execution_named("Parse", || parse_source_string_to_mir(source))
    } else {
        parse_source_string_to_mir(source)
    }?;

    execute_mir_with_native_functions(mir, &meta_data, native_functions, time_execution)
}

/// Call this function to initialise logging, if the `logging` feature is enabled.
pub fn init_logging() {
    #[cfg(all(feature = "logging", not(feature = "webasm")))]
    fn setup_logging() {
        simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
            LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )])
        .expect("Failed to initialize logging");
    }
    #[cfg(all(feature = "logging", not(feature = "webasm")))]
    setup_logging();
}
