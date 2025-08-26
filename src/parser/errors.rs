use ::std::ops::Range;

use thiserror::Error;

use crate::{ast::SourceSpan, token::TokenKind};

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
}

/// Counts the number of newline (`\n`) characters in a string slice.
fn count_new_lines(str: &str) -> usize {
    str.chars().filter(|c| *c == '\n').count()
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

    /// Get the 3 segments from the source string and `self`.
    /// # Returns
    /// `(before, the_error_span, after)`
    pub fn get_segments_from_source<'a>(
        &self,
        source_string: &'a str,
    ) -> (&'a str, &'a str, &'a str) {
        let error_span_start = self.span.start as usize;
        let error_span_end = self.span.end as usize;

        let before_span = &source_string[0..error_span_start];
        let error_span = &source_string[error_span_start..error_span_end];
        let after_span = &source_string[error_span_end..];

        (before_span, error_span, after_span)
    }

    /// Format the source line into the string to be printed, along with where in the printed
    /// string the "error" span is.
    fn format_source_line(
        prefix: &str,
        before: &str,
        error: &str,
        after: &str,
    ) -> (Range<usize>, String) {
        let mut err_line = String::from(prefix);
        err_line += before;

        let err_span_start = err_line.len();
        err_line += error;
        let err_span_end = err_line.len();

        err_line += after;

        (err_span_start..err_span_end, err_line)
    }

    fn generate_highlight_line_str(
        prefix: &str,
        err_span: Range<usize>,
        highlight: &str,
    ) -> String {
        let mut highlight_str = prefix.to_string();
        highlight_str += &" ".repeat(err_span.start.saturating_sub(prefix.len()));
        highlight_str += &highlight.repeat(err_span.len());
        highlight_str
    }

    /// Format the error as an error message, given the source code string.
    #[inline]
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        let (before, error, after) = self.get_segments_from_source(source_string);
        let line_number = count_new_lines(before).saturating_add(1);
        let previous_newline_index = before.rfind('\n').map(|i| i.saturating_add(1)).unwrap_or(0);
        let next_newline_index = after.find('\n').unwrap_or(source_string.len());
        let error_line_char_index = before[previous_newline_index..]
            .chars()
            .count()
            .saturating_add(1);
        let _error_section_line_count = count_new_lines(error);

        let line_number_str = line_number.to_string();
        let line_num_prefix = format!("{line_number_str} | ");
        let blank_prefix = format!("{} | ", " ".repeat(line_number_str.len()));

        let before_error_line_str = &before[(previous_newline_index.min(before.len()))..];

        let (err_span, err_line) = Self::format_source_line(
            &line_num_prefix,
            before_error_line_str,
            error,
            &after[..next_newline_index],
        );
        let highlight_str = Self::generate_highlight_line_str(&blank_prefix, err_span, "^");

        format!(
            "{0}\n\
            {line_number}:{error_line_char_index}\n\
            {blank_prefix}\n\
            {err_line}\n\
            {highlight_str}\n",
            self.error_kind
        )
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
