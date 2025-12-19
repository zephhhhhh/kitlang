//! The purpose of this module is to define error types for the resolution phase of the compiler.
//! Provides structured error representations for various resolution failures, including unresolved references,
//! inaccessible items, and general diagnostic messages.

use crate::ast::{SourceSpan, SpannedIdentPath};
use crate::intermediate::hir::HirId;
use crate::spanned_error::SpannedErrorBuilder;
use std::fmt::{Debug, Display};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ResolverErrorKind {
    #[error("Error while resolving: {0:?}")]
    ResolverErrors(Vec<ResolverError>),
    #[error("Unresolved References: {0:?}")]
    UnresolvedReferences(UnresolvedReferences),
    #[error("Diagnostic: {0}")]
    Diagnostic(String),
}

impl ResolverErrorKind {
    #[inline]
    #[must_use]
    pub fn with_span(&self, span: impl Into<SourceSpan>) -> ResolverError {
        ResolverError::new(self.clone(), span)
    }

    #[inline]
    #[must_use]
    pub fn with_no_span(&self) -> ResolverError {
        ResolverError::new(self.clone(), SourceSpan::null_span())
    }
}

/// Represents an error while resolving references/symbols after in the intermediate representation.
#[derive(Debug, Error, Clone, PartialEq)]
pub struct ResolverError {
    pub error_kind: ResolverErrorKind,
    pub span: SourceSpan,
}

impl ResolverError {
    #[inline]
    #[must_use]
    pub fn new(error_kind: ResolverErrorKind, span: impl Into<SourceSpan>) -> Self {
        Self {
            error_kind,
            span: span.into(),
        }
    }

    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match &self.error_kind {
            ResolverErrorKind::UnresolvedReferences(unresolved_references) => {
                let mut final_output = String::new();
                for unresolved in &unresolved_references.references {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    let header_line = unresolved
                        .failure
                        .error_message_for(unresolved.path.path.to_ident().str());
                    final_output += &SpannedErrorBuilder::new(source_string, unresolved.path.span)
                        .print_header_line(&header_line)
                        .generate_highlight()
                        .generate_output();
                }
                final_output
            }
            ResolverErrorKind::ResolverErrors(errors) => {
                let mut final_output = String::new();
                for error in errors {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    final_output += &error.format_as_error_message(source_string);
                }
                final_output
            }
            ResolverErrorKind::Diagnostic(msg) => {
                SpannedErrorBuilder::new(source_string, self.span)
                    .print_header_line(msg)
                    .generate_highlight()
                    .generate_output()
            }
        }
    }
}

impl Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.error_kind {
            ResolverErrorKind::ResolverErrors(ref errors) => {
                write!(f, "Resolver Errors: {errors:#?}")
            }
            ResolverErrorKind::Diagnostic(ref msg) => {
                write!(f, "Diagnostic: {msg}")
            }
            ResolverErrorKind::UnresolvedReferences(ref refs) => {
                write!(f, "Unresolved References: {refs:#?}")
            }
        }
    }
}

macro_rules! push_resolve_err {
    ($self: expr, no_span, $($arg:tt)*) => {
        $self.errors.push(resolve_err!(no_span, $($arg)*));
    };
    ($self: expr, on_span, $span: expr, $($arg:tt)*) => {
        $self.errors.push(resolve_err!(on_span, $span, $($arg)*));
    };
    ($self: expr, $hlir: expr, $id: expr, $($arg:tt)*) => {
        $self.errors.push(resolve_err!($hlir, $id, $($arg)*));
    };
}

macro_rules! resolve_err {
    (no_span, $($arg:tt)*) => {
        ResolverError::new(ResolverErrorKind::Diagnostic(format!($($arg)*)), $crate::ast::SourceSpan::null_span())
    };
    (on_span, $span: expr, $($arg:tt)*) => {
        ResolverError::new(ResolverErrorKind::Diagnostic(format!($($arg)*)), $span)
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        ResolverError::new(ResolverErrorKind::Diagnostic(format!($($arg)*)), $crate::intermediate::hir::get_span_by_id($hlir.as_ref(), $id))
    };
}
pub(crate) use push_resolve_err;
pub(crate) use resolve_err;

pub type ResolveResult<T> = Result<T, ResolverError>;

/// Represents the reason for a resolution failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolutionFailure {
    /// The referenced item was not found. (I.e., It isn't defined)
    NotFound,
    /// The referenced item is not accessible from the current scope due to visibility rules.
    Inaccessible,
}

impl ResolutionFailure {
    /// Returns a human-readable description of the resolution failure with a specific target ident.
    #[inline]
    #[must_use]
    pub fn error_message_for(&self, target_ident: &str) -> String {
        match self {
            Self::NotFound => {
                format!("The referenced item '{target_ident}' was not found.")
            }
            Self::Inaccessible => format!(
                "The referenced item '{target_ident}' is not accessible from the current scope."
            ),
        }
    }
}

/// Represents a single unresolved reference in the intermediate representation.
/// It contains the path of the reference, its associated [`HirId`], and the reason for the failure.
#[derive(Clone, PartialEq, Eq)]
pub struct UnresolvedReference {
    /// The spanned identifier path of the unresolved reference.
    pub path: SpannedIdentPath,
    /// The [`HirId`] associated with the unresolved reference.
    pub id: HirId,
    /// The reason for the resolution failure.
    pub failure: ResolutionFailure,
}

impl Debug for UnresolvedReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.path)
    }
}

/// Represents a collection of unresolved references in the intermediate representation.
/// Used for error handling, to bundle up multiple resolution failures into one error value.
#[derive(Clone, PartialEq, Eq)]
pub struct UnresolvedReferences {
    pub references: Vec<UnresolvedReference>,
}

impl Debug for UnresolvedReferences {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.references).finish()
    }
}
