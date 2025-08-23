use thiserror::Error;

use crate::token::{Token, TokenKind};

/// Parse error result, has [`ParseError`] as the error variant.
pub type PResult<T> = Result<T, ParseError>;

#[derive(Error, Debug, Clone, PartialEq, PartialOrd)]
pub enum ParseErrorKind {
    #[error("Invalid string literal")]
    InvalidStringLiteral,
    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error("Invalider literal: {0}")]
    InvalidLiteral(String),
    #[error("Unknown token found")]
    UnknownToken,

    #[error("Wrong token type found: {0:?}")]
    WrongTokenKind(TokenKind),
    #[error("Expected token: {0:?}, found: {1:?}")]
    ExpectedToken(TokenKind, TokenKind),
    #[error("Expected token: {0:?}, but none was found")]
    ExpectedTokenFoundNone(TokenKind),

    #[error("Invalid expression atom: {0:?}")]
    InvalidExpressionAtom(TokenKind),

    #[error("Expected identifier, found: {0:?}")]
    ExpectedIdentifier(TokenKind),
    #[error("Expected identifier, but none was found")]
    ExpectedIdentifierFoundNone,

    #[error("Expected Semi-colon, expressions can only be at the end of a block")]
    ExpectedSemiColon,

    #[error("Expected an item, found: {0:?}")]
    ExpectedItem(TokenKind),

    #[error("Unexpected token: {0:?}")]
    UnexpectedToken(TokenKind),

    #[error("Invalid path found: {0:?}")]
    InvalidPath(String),

    #[error("Found an invalid binary operator")]
    InvalidBinaryOperator,

    #[error("Tried to parse AST type, but there were no available tokens")]
    NoTokens,
    #[error("Unterminated string literal.")]
    UnterminatedStringLiteral,
}

/// Represents the span of bytes in the source string that the error originates from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

impl From<(u32, u32)> for SourceSpan {
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl<T: Into<u32>> From<::std::ops::Range<T>> for SourceSpan {
    fn from(value: ::std::ops::Range<T>) -> SourceSpan {
        Self::new(value.start.into(), value.end.into())
    }
}

impl From<Token> for SourceSpan {
    fn from(value: Token) -> Self {
        Self::new(value.start, value.end)
    }
}

impl From<&Token> for SourceSpan {
    fn from(value: &Token) -> Self {
        Self::new(value.start, value.end)
    }
}

/// Represents an error while parsing the [`Token`]s
#[derive(Debug, Error)]
pub struct ParseError {
    pub span: SourceSpan,
    #[source]
    pub error_kind: ParseErrorKind,
}

impl ParseError {
    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<ParseErrorKind>, span: impl Into<SourceSpan>) -> Self {
        Self {
            error_kind: kind.into(),
            span: span.into(),
        }
    }

    #[inline]
    #[must_use]
    pub fn no_tokens(span: impl Into<SourceSpan>) -> Self {
        Self::new(ParseErrorKind::NoTokens, span)
    }
}

impl ::std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error: {}, Location: [{}..{}]",
            self.error_kind, self.span.start, self.span.end
        )
    }
}
