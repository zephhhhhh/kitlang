use ::std::fmt::Display;

use thiserror::Error;

use crate::intermediate::hir::HirId;
use crate::intermediate::resolver::UnresolvedReferences;

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
                if let Some(first_error) = unresolved_references.references.first() {
                    SpannedErrorBuilder::new(source_string, first_error.path.span)
                        .print_header_line(format!(
                            "Failed to resolve reference '{}'",
                            first_error.path.path.to_ident().str()
                        ))
                        .generate_highlight()
                        .generate_output()
                } else {
                    format!("Unresolved references: {:?}", unresolved_references)
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
