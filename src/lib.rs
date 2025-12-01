#![feature(iter_advance_by)]
#![feature(debug_closure_helpers)]

//! Kitlang is a language built for embedding/scripting purposes
//!
//! The language is inspired by rust and aims to be as similar to rust as possible with some
//! creative liberties taken.
//!
//! The structure of the project is also inspired by `rust` and `rustc`.

use thiserror::Error;

pub mod ast;
pub mod intermediate;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod profiling;
pub mod spanned_error;
pub mod token;

// Outward facing API..

/// Represents errors from different stages in the compilation process.
/// TODO: Expand this to include type checking and MIR errors.
#[derive(Debug, Error)]
pub enum KitlangError {
    #[error("Parser Error: {0}")]
    ParserError(#[from] crate::parser::ParseError),
    #[error("Lowering Error: {0}")]
    LoweringError(#[from] crate::intermediate::hir::LoweringError),
    #[error("Execution ended unexpectedly.")]
    ExecutionEndedUnexpectedly,
    #[error("Failed to find entry point 'main' function.")]
    FailedToFindEntryPoint,
}

impl KitlangError {
    /// Format the error as an error message, given the source code string.
    #[inline]
    #[must_use]
    pub fn format_as_error_message(&self, source_string: &str) -> String {
        match self {
            KitlangError::ParserError(err) => err.format_as_error_message(source_string),
            KitlangError::LoweringError(err) => err.format_as_error_message(source_string),
            KitlangError::ExecutionEndedUnexpectedly => {
                "Error: Execution ended unexpectedly.".to_string()
            }
            KitlangError::FailedToFindEntryPoint => {
                "Error: Failed to find entry point 'main' function.".to_string()
            }
        }
    }
}

/// Result type for Kitlang operations with error variants for different stages in the compilation process.
pub type KitlangResult<T> = Result<T, KitlangError>;

/// Parse a given kitlang source code string into MIR representations.
pub fn parse_source_string_to_mir(
    source: &str,
) -> KitlangResult<(intermediate::hir::ProgramMetaData, intermediate::mir::MIR)> {
    let ast = crate::parser::parse_from_source(source)?;
    let (meta_data, hir) = intermediate::hir::parse_ast_to_hir_processed(&ast)?;
    let mir = intermediate::mir::lower_hir_to_mir(&hir, &meta_data)?;
    Ok((meta_data, mir))
}

/// Execute a given kitlang source code string with the MIR interpreter, using the provided native functions.
/// If `time_execution` is `true`, the different stages of execution will be timed and printed
pub fn execute_source_string(
    source: &str,
    native_functions: crate::interpreter::mir_interpreter::RegisterNativeFns,
    time_execution: bool,
) -> KitlangResult<crate::interpreter::mir_interpreter::Value> {
    use crate::interpreter::mir_interpreter::execute_mir_with_native_functions;

    let (meta_data, mir) = if time_execution {
        crate::profiling::print_execution_named("Parse", || parse_source_string_to_mir(source))
    } else {
        parse_source_string_to_mir(source)
    }?;

    execute_mir_with_native_functions(mir, &meta_data, native_functions, time_execution)
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::{CodeCursor, tokenise_stripped},
        token::{Keyword, LiteralKind, Punctuation, Token, TokenKind},
    };

    // Helper macros..
    macro_rules! int_literal {
        ($val: literal) => {
            TokenKind::Literal(LiteralKind::Integer($val))
        };
    }
    macro_rules! float_literal {
        ($val: literal) => {
            TokenKind::Literal(LiteralKind::Float($val))
        };
    }
    macro_rules! identifier {
        ($identifier_name: literal) => {
            TokenKind::Ident($identifier_name.to_string())
        };
    }
    macro_rules! tok_kind {
        ($tokens: ident) => {
            $tokens.next().expect("Token should exist.").kind
        };
    }

    #[test]
    fn cursor() {
        let source_string = "abc     def";
        let mut cursor = CodeCursor::new(source_string);

        assert_eq!(cursor.as_str(), source_string);

        assert_eq!(cursor.position(), 0);
        assert_eq!(cursor.remaining(), 11);

        assert_eq!(cursor.peek(), 'a');
        assert_eq!(cursor.peek_second(), 'b');
        assert_eq!(cursor.peek_third(), 'c');

        assert_eq!(cursor.consume(), Some('a'));

        assert_eq!(cursor.peek(), 'b');
        assert_eq!(cursor.peek_second(), 'c');
        assert_eq!(cursor.peek_third(), ' ');

        assert_eq!(cursor.consume_whitespace(), 0);
        assert_eq!(cursor.peek(), 'b');

        assert_eq!(cursor.peek_at(0), Some('b'));
        assert_eq!(cursor.peek_at(1), Some('c'));
        assert_eq!(cursor.peek_at(2), Some(' '));

        assert_eq!(cursor.consume(), Some('b'));
        assert_eq!(cursor.consume(), Some('c'));
        assert_eq!(cursor.peek(), ' ');

        assert_eq!(cursor.consume_whitespace(), 5);
        assert_eq!(cursor.as_str(), &source_string[8..]);

        assert_eq!(cursor.consume_expect(&['d', 'e', 'f']), Some(3));
        assert_eq!(cursor.consume(), None);

        assert!(cursor.is_eof());
    }

    fn verify_token_kinds(iter: &mut impl Iterator<Item = Token>, expected_kinds: &[TokenKind]) {
        for expected_kind in expected_kinds {
            assert_eq!(
                iter.next().expect("Token should exist.").kind,
                *expected_kind
            );
        }
    }

    #[test]
    fn literals() {
        let mut tokens = tokenise_stripped(include_str!("../tests/lexer/literals.purr"));

        // This also makes sure comments are indeed being stripped..
        let expected_tokens = [
            // First block, testing integer, negative integer, float, negative float, and
            // differentiation between negative float and '-' symbol.
            int_literal!(10),
            int_literal!(-10),
            float_literal!(0.2),
            float_literal!(-0.212421),
            float_literal!(0.213),
            TokenKind::Punctuation(Punctuation::Minus),
            float_literal!(-0.1),
            // Second block, testing different number formats and radixes.
            int_literal!(15),
            int_literal!(65535),
            int_literal!(9),
            float_literal!(0.12),
            // Third block, testing using '_' formatting characters for easier readability with
            // different number formats.
            int_literal!(65535),
            int_literal!(240),
            int_literal!(1170),
            int_literal!(10001000),
            float_literal!(10201230.10001),
        ];

        verify_token_kinds(&mut tokens, &expected_tokens);

        // Test we get the Eof token, and then the iterator halts.
        assert_eq!(tok_kind!(tokens), TokenKind::Eof);
        assert_eq!(tokens.next(), None);
    }

    #[test]
    fn keywords_idents() {
        let mut tokens = tokenise_stripped(include_str!("../tests/lexer/keywords_idents.purr"));

        let expected_tokens = [
            Keyword::Use.into(),
            identifier!("std"),
            Punctuation::Colon.into(),
            Punctuation::Colon.into(),
            identifier!("println"),
            Punctuation::SemiColon.into(),
            Keyword::Pub.into(),
            Keyword::Fn.into(),
            identifier!("function_definition"),
            Punctuation::OpenParen.into(),
            identifier!("age"),
            Punctuation::Colon.into(),
            identifier!("i32"),
            Punctuation::CloseParen.into(),
            Punctuation::Minus.into(),
            Punctuation::GreaterThan.into(),
            identifier!("bool"),
            Punctuation::OpenBrace.into(),
            Keyword::Let.into(),
            identifier!("age_times_2"),
            Punctuation::Eq.into(),
            identifier!("age"),
            Punctuation::Star.into(),
            int_literal!(2),
            Punctuation::SemiColon.into(),
            Keyword::If.into(),
            identifier!("age_times_2"),
            Punctuation::Eq.into(),
            Punctuation::Eq.into(),
            int_literal!(42),
            Punctuation::OpenBrace.into(),
            Keyword::True.into(),
            Punctuation::CloseBrace.into(),
            Keyword::Else.into(),
            Punctuation::OpenBrace.into(),
            Keyword::False.into(),
            Punctuation::CloseBrace.into(),
            Punctuation::CloseBrace.into(),
            Keyword::Pub.into(),
            Keyword::Struct.into(),
            identifier!("SomeData"),
            Punctuation::OpenBrace.into(),
            Keyword::Pub.into(),
            identifier!("element"),
            Punctuation::Colon.into(),
            identifier!("ElementTy"),
            Punctuation::Comma.into(),
            Punctuation::CloseBrace.into(),
        ];

        verify_token_kinds(&mut tokens, &expected_tokens);

        // Test we get the Eof token, and then the iterator halts.
        assert_eq!(tok_kind!(tokens), TokenKind::Eof);
        assert_eq!(tokens.next(), None);
    }
}
