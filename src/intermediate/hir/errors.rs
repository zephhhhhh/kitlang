use thiserror::Error;

use crate::intermediate::{hir, resolver::UnresolvedReferences};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum LoweringErrorKind {
    #[error("Expected a 'Parameter' node, found: {0:?}")]
    ExpectedParameterNode(hir::HirId),
    #[error("Expected to find '{0}' at '{1:?}'")]
    WrongNodeType(String, hir::HirId),
    #[error("Expected to find a HIR Node at '{0:?}' but found none!")]
    ExpectedNodeFoundNone(hir::HirId),

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
}

impl ::std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {:#?}",
            self.error_kind, //self.span.start, self.span.end
        )
    }
}

pub type LowerResult<T> = Result<T, LoweringError>;
