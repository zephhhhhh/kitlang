use thiserror::Error;

use crate::ast::SourceSpan;

#[derive(Error, Debug, Clone, PartialEq, PartialOrd)]
pub enum LoweringErrorKind {}

/// Represents an error while parsing the [`ASTRoot`] into High-level IR.
#[derive(Debug, Error)]
pub struct LoweringError {
    pub span: SourceSpan,
    #[source]
    pub error_kind: LoweringErrorKind,
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
}

impl ::std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {}, Location: [{}..{}]",
            self.error_kind, self.span.start, self.span.end
        )
    }
}

pub type LowerResult<T> = Result<T, LoweringError>;
