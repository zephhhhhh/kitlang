#![feature(iter_advance_by)]

pub mod lexer;
pub mod prelude;
pub mod token;

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
