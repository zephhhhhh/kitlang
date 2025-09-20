use thiserror::Error;

use crate::ast::SourceSpan;
use crate::spanned_error::SpannedErrorBuilder;
use crate::token::TokenKind;

/// Parse error result, has [`ParseError`] as the error variant.
pub type PResult<T> = Result<T, ParseError>;

#[derive(Error, Debug, Clone, PartialEq, PartialOrd)]
pub enum ParseErrorKind {
    #[error("Invalid string literal")]
    InvalidStringLiteral,
    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error("Invalid literal: {0}")]
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

    #[error("Parameter 'self' must be the first argument in the function parameters")]
    SelfMustBeFirstArgument,
    #[error(
        "Parameter 'self' cannot be used in free functions, it must be used within an associated 'impl' block"
    )]
    SelfMustBeUsedInAMethod,

    #[error("Expected Semi-colon, expressions can only be at the end of a block")]
    ExpectedSemiColon,

    #[error("Expected an item, found: {0:?}")]
    ExpectedItem(TokenKind),

    #[error("Unexpected token: {0:?}")]
    UnexpectedToken(TokenKind),

    #[error("Invalid path, path separator must be '::' and all segments must be identifiers")]
    InvalidPath,

    #[error("Found an invalid binary operator")]
    InvalidBinaryOperator,

    #[error("Tried to parse AST type, but there were no available tokens")]
    NoTokens,
    #[error("Unterminated string literal.")]
    UnterminatedStringLiteral,

    #[error("A function marked 'native' cannot define a function body")]
    NativeFunctionCannotDefineABody,
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

    /// Format the error as an error message, given the source code string.
    #[inline]
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        SpannedErrorBuilder::new(source_string, self.span)
            .generate_highlight()
            .print_header_line(format!("Error: {}", self.error_kind))
            .generate_output()
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
