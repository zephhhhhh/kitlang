use ::std::fmt::Display;

use thiserror::Error;

use crate::spanned_error::SpannedErrorBuilder;

/// Represents errors that occur during the interpretation of MIR.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum InterpreterErrorKind {
    /// Multiple interpreter errors occurred.
    #[error("Interpreter errors: {0:?}")]
    InterpreterErrors(Vec<InterpreterError>),
    #[error("Diagnostic: {0:?}")]
    Diagnostic(String),
}

impl From<InterpreterErrorKind> for InterpreterError {
    fn from(kind: InterpreterErrorKind) -> Self {
        InterpreterError::new(kind)
    }
}

/// Represents an error while parsing the AST into High-level IR.
#[derive(Debug, Error, Clone, PartialEq)]
pub struct InterpreterError {
    #[source]
    pub error_kind: InterpreterErrorKind,
}

impl InterpreterError {
    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<InterpreterErrorKind>) -> Self {
        Self {
            error_kind: kind.into(),
        }
    }

    /// Format the lowering error as an error message, given the source code string.
    /// This function can process multiple errors if there are any contained within.
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match &self.error_kind {
            InterpreterErrorKind::InterpreterErrors(errs) => {
                let mut final_output = String::new();
                for error in errs {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    final_output += &error.format_as_error_message(source_string);
                }
                final_output
            }
            InterpreterErrorKind::Diagnostic(d) => {
                SpannedErrorBuilder::new(source_string, crate::ast::SourceSpan::null_span())
                    .error_prefix("Interpreter Error:")
                    .print_header_line(d)
                    .generate_highlight()
                    .generate_output()
            }
        }
    }
}

impl Display for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Execution error: {:#?}", self.error_kind,)
    }
}

// macro_rules! push_interp_err {
//     ($self: expr, no_span, $($arg:tt)*) => {
//         $self.errors.push($crate::interpreter::errors::interp_err!(no_span, $($arg)*));
//     };
//     ($self: expr, on_span, $span: expr, $($arg:tt)*) => {
//         $self.errors.push($crate::interpreter::errors::interp_err!(on_span, $span, $($arg)*));
//     };
//     ($self: expr, $hlir: expr, $id: expr, $($arg:tt)*) => {
//         $self.errors.push($crate::interpreter::errors::interp_err!($hlir, $id, $($arg)*));
//     };
// }

macro_rules! interp_err {
    ($($arg:tt)*) => {
        $crate::interpreter::errors::InterpreterError::new($crate::interpreter::errors::InterpreterErrorKind::Diagnostic(format!($($arg)*)))
    };
}
pub(crate) use interp_err;
//pub(super) use push_interp_err;

pub(crate) type InterpResult<T> = Result<T, InterpreterError>;
