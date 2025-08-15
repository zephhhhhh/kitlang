#![feature(iter_advance_by)]

pub mod lexer;
pub mod prelude;
pub mod token;

#[cfg(test)]
mod tests {
    use crate::lexer::CodeCursor;

    #[test]
    fn test_cursor() {
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
}
