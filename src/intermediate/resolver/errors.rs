use crate::ast::{IdentPath, SourceSpan, SpannedIdentPath};
use crate::intermediate::hir::HirId;
use crate::intermediate::resolver::NamespaceKind;
use crate::spanned_error::SpannedErrorBuilder;
use std::fmt::{Debug, Display};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ResolverErrorKind {
    #[error("Error while resolving: {0:?}")]
    ResolverErrors(Vec<ResolverError>),

    #[error("Failed to resolve references: {0:?}")]
    UnresolvedReferences(UnresolvedReferences),

    #[error("Cannot find parent namespace while type checking for path: {0:?}")]
    CannotFindNamespaceForTypeCheck(IdentPath),
    #[error("Cannot find parent namespace for path: {0:?}")]
    CannotFindParentNamespace(IdentPath),
    #[error("Item '{0}' is already defined within the scope '{1}'!")]
    ItemAlreadyDefined(SpannedIdentPath, IdentPath),
    #[error("Cannot resolve struct fields on type: {0:?}")]
    CannotResolveStructFields(SpannedIdentPath),

    #[error("Variable '{0}' is already defined!")]
    VariableAlreadyDefined(String),

    #[error("Expected ADT definition at '{0}', but instead found: '{1:?}'!")]
    ExpectedADTDefinition(IdentPath, NamespaceKind),

    #[error("Could not resolve 'self' function argument for path: {0:?}")]
    CouldNotResolveSelfFunctionArg(IdentPath),
    #[error("Could not resolve function argument type '{0}' for path: {1:?}")]
    CouldNotResolveFunctionArgType(SpannedIdentPath, IdentPath),
    #[error("Could not resolve function return type '{0}' for path: {1:?}")]
    CouldNotResolveFunctionReturnType(SpannedIdentPath, IdentPath),

    #[error("Failed to find struct field '{0}' in type at path: {1:?}")]
    FailedToFindStructField(String, IdentPath),

    #[error("Failed to get type for impl at path: {0:?}")]
    FailedToGetTypeForImpl(IdentPath),
}

impl ResolverErrorKind {
    pub fn with_span(&self, span: impl Into<SourceSpan>) -> ResolverError {
        ResolverError::new(self.clone(), span)
    }

    pub fn with_no_span(&self) -> ResolverError {
        ResolverError::new(self.clone(), SourceSpan::null_span())
    }
}

/// Represents an error while parsing the [`ASTRoot`] into High-level IR.
#[derive(Debug, Error, Clone, PartialEq)]
pub struct ResolverError {
    #[source]
    pub error_kind: ResolverErrorKind,
    pub span: SourceSpan,
}

impl ResolverError {
    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<ResolverErrorKind>, span: impl Into<SourceSpan>) -> Self {
        Self {
            error_kind: kind.into(),
            span: span.into(),
        }
    }

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
            other => SpannedErrorBuilder::new(source_string, self.span)
                .print_header_line(format!("Resolver error: {:#?}", other))
                .generate_highlight()
                .generate_output(),
        }
    }
}

impl Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {:#?}",
            self.error_kind, //self.span.start, self.span.end
        )
    }
}

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
    pub fn error_message_for(&self, target_ident: &str) -> String {
        match self {
            ResolutionFailure::NotFound => {
                format!("The referenced item '{}' was not found.", target_ident)
            }
            ResolutionFailure::Inaccessible => format!(
                "The referenced item '{}' is not accessible from the current scope.",
                target_ident
            ),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct UnresolvedReference {
    pub path: SpannedIdentPath,
    pub id: HirId,
    pub failure: ResolutionFailure,
}

impl Debug for UnresolvedReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.path)
    }
}

#[derive(Clone, PartialEq)]
pub struct UnresolvedReferences {
    pub references: Vec<UnresolvedReference>,
}

impl Debug for UnresolvedReferences {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.references).finish()
    }
}
