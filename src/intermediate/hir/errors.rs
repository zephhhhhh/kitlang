use ::std::fmt::Display;

use thiserror::Error;

use crate::ast::SourceSpan;
use crate::intermediate::hir::HirId;
use crate::intermediate::resolver::UnresolvedReferences;

use crate::intermediate::type_check::TypeCheckFail;
use crate::spanned_error::SpannedErrorBuilder;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LoweringErrorKind {
    #[error("Expected a 'Parameter' node, found: {0:?}")]
    ExpectedParameterNode(HirId),
    #[error("Expected to find '{0}' at '{1:?}'")]
    WrongNodeType(String, HirId),
    #[error("Expected to find a HIR Node at '{0:?}' but found none!")]
    ExpectedNodeFoundNone(HirId),

    #[error("Variable '{0}' is already defined!")]
    VariableAlreadyDefined(String),
    #[error("Failed to find value '{0}'")]
    CannotFindValue(String),

    #[error("Failed to resolve references: {0:?}")]
    UnresolvedReferences(UnresolvedReferences),

    #[error("Failed to validate types: {0:?}")]
    TypeCheckFail(Vec<TypeCheckFail>),

    #[error("Cannot assign to an immutable variable.")]
    CannotAssignToImmutableVariable(SourceSpan),

    #[error("Item '{1}' is already defined within the scope '{2}'!")]
    ItemAlreadyDefined(SourceSpan, String, String),

    // TODO: REMOVE THIS LATER.
    #[error("{0}")]
    RemoveMeMessage(String, Option<SourceSpan>),
}

/// Represents an error while parsing the [`ASTRoot`] into High-level IR.
#[derive(Debug, Error, Clone)]
pub struct LoweringError {
    //pub span: SourceSpan,
    #[source]
    pub error_kind: LoweringErrorKind,
}

impl LoweringError {
    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<LoweringErrorKind> /*span: impl Into<SourceSpan>*/) -> Self {
        Self {
            error_kind: kind.into(),
            //span: span.into(),
        }
    }

    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match &self.error_kind {
            LoweringErrorKind::UnresolvedReferences(unresolved_references) => {
                let mut final_output = String::new();
                for unresolved in &unresolved_references.references {
                    if !final_output.is_empty() {
                        final_output += "\n";
                    }
                    let header_line = match unresolved.failure {
                        crate::intermediate::resolver::ResolutionFailure::NotFound => {
                            format!(
                                "Failed to resolve reference '{}'",
                                unresolved.path.path.to_ident().str()
                            )
                        }
                        crate::intermediate::resolver::ResolutionFailure::Inaccessible => {
                            format!(
                                "Cannot access referenced item '{}'",
                                unresolved.path.path.to_ident().str()
                            )
                        }
                    };
                    final_output += &SpannedErrorBuilder::new(source_string, unresolved.path.span)
                        .print_header_line(&header_line)
                        .generate_highlight()
                        .generate_output();
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
            LoweringErrorKind::CannotAssignToImmutableVariable(span) => {
                SpannedErrorBuilder::new(source_string, *span)
                    .print_header_line("Cannot assign to an immutable variable.")
                    .generate_highlight()
                    .generate_output()
            }
            LoweringErrorKind::ItemAlreadyDefined(span, item, scope) => {
                SpannedErrorBuilder::new(source_string, *span)
                    .print_header_line(format!(
                        "Item '{}' is already defined within the scope '{}'.",
                        item, scope
                    ))
                    .generate_highlight()
                    .generate_output()
            }
            LoweringErrorKind::RemoveMeMessage(msg, span) => {
                if let Some(span) = span {
                    SpannedErrorBuilder::new(source_string, *span)
                        .print_header_line(msg)
                        .generate_highlight()
                        .generate_output()
                } else {
                    msg.clone()
                }
            }
            e => format!("Failed to lower to HIR: {:?}", e),
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {:#?}",
            self.error_kind, //self.span.start, self.span.end
        )
    }
}

pub type LowerResult<T> = Result<T, LoweringError>;
