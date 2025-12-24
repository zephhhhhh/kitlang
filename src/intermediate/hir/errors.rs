use ::std::fmt::Display;

use thiserror::Error;

use crate::ast::SourceSpan;
use crate::intermediate::resolver::errors::ResolverError;

use crate::intermediate::type_check::TypeCheckFail;
use crate::spanned_error::SpannedErrorBuilder;

/// Represents errors that occur during the lowering phase from AST to High-level IR.
/// Each kind shows which stage the error occurred in.
/// This error can contain multiple errors that occurred during lowering.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum LoweringErrorKind {
    /// Multiple lowering errors occurred.
    #[error("Lowering errors: {0:?}")]
    LoweringErrors(Vec<LoweringError>),
    /// Represents errors that occurred during the name resolution stage.
    #[error("Resolver error: {0:?}")]
    ResolverError(#[from] ResolverError),
    /// Represents type checking failures that occurred during lowering.
    #[error("Failed to validate types: {0:?}")]
    TypeCheckFail(Vec<TypeCheckFail>),
    /// An individual error that happened during lowering itself.
    #[error("Diagnostic: {0:?}")]
    Diagnostic(String),
}

impl LoweringErrorKind {
    /// Attach a source span to the error kind, producing a [`LoweringError`].
    #[inline]
    #[must_use]
    pub fn with_span(&self, span: impl Into<SourceSpan>) -> LoweringError {
        LoweringError::new(self.clone(), span)
    }

    /// Produce a [`LoweringError`] with no associated source span.
    #[inline]
    #[must_use]
    pub fn with_no_span(&self) -> LoweringError {
        LoweringError::new(self.clone(), SourceSpan::null_span())
    }
}

/// Represents an error while parsing the AST into High-level IR.
#[derive(Debug, Error, Clone, PartialEq)]
pub struct LoweringError {
    #[source]
    pub error_kind: LoweringErrorKind,
    pub span: SourceSpan,
}

impl LoweringError {
    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<LoweringErrorKind>, span: impl Into<SourceSpan>) -> Self {
        Self {
            error_kind: kind.into(),
            span: span.into(),
        }
    }

    /// Format the lowering error as an error message, given the source code string.
    /// This function can process multiple errors if there are any contained within.
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match &self.error_kind {
            LoweringErrorKind::LoweringErrors(errs) => {
                let mut final_output = String::new();
                for error in errs {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    final_output += &error.format_as_error_message(source_string);
                }
                final_output
            }
            LoweringErrorKind::TypeCheckFail(fails) => {
                let mut final_output = String::new();
                for fail in fails {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    final_output += &SpannedErrorBuilder::new(source_string, fail.span)
                        .print_header_line(&fail.reason)
                        .generate_highlight()
                        .generate_output();
                }
                final_output
            }
            LoweringErrorKind::ResolverError(e) => e.format_as_error_message(source_string),
            LoweringErrorKind::Diagnostic(d) => SpannedErrorBuilder::new(source_string, self.span)
                .print_header_line(d)
                .generate_highlight()
                .generate_output(),
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error: {:#?}", self.error_kind,)
    }
}

macro_rules! push_lower_err {
    ($self: expr, no_span, $($arg:tt)*) => {
        $self.errors.push(lowering_err!(no_span, $($arg)*));
    };
    ($self: expr, on_span, $span: expr, $($arg:tt)*) => {
        $self.errors.push(lowering_err!(on_span, $span, $($arg)*));
    };
    ($self: expr, $hlir: expr, $id: expr, $($arg:tt)*) => {
        $self.errors.push(lowering_err!($hlir, $id, $($arg)*));
    };
}

macro_rules! lowering_err {
    (no_span, $($arg:tt)*) => {
        $crate::intermediate::hir::errors::LoweringError::new(
            $crate::intermediate::hir::errors::LoweringErrorKind::Diagnostic(format!($($arg)*)),
            $crate::ast::SourceSpan::null_span()
        )
    };
    (on_span, $span: expr, $($arg:tt)*) => {
        $crate::intermediate::hir::errors::LoweringError::new(
            $crate::intermediate::hir::errors::LoweringErrorKind::Diagnostic(format!($($arg)*)),
            $span
        )
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        $crate::intermediate::hir::errors::LoweringError::new(
            $crate::intermediate::hir::errors::LoweringErrorKind::Diagnostic(format!($($arg)*)),
            $crate::intermediate::hir::get_span_by_id($hlir.as_ref(), $id)
        )
    };
}
pub(crate) use lowering_err;
pub(crate) use push_lower_err;

pub type LowerResult<T> = Result<T, LoweringError>;
