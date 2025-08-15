use std::str::Chars;

use crate::token::{Keyword, LiteralKind, Punctuation, Token, TokenKind};

/// Iterator over a character sequence, supports peeking and reading from the sequence.
#[derive(Debug)]
pub struct CodeCursor<'a> {
    /// Length of the input string in characters.
    length: usize,
    /// Iterator over `code-points` of a source string.
    chars: Chars<'a>,
    /// Previous character to be read.
    previous: char,
}

/// End of file.
pub(crate) const EOF_CHAR: char = '\0';

impl<'a> CodeCursor<'a> {
    /// Construct a new code character iterator with a provided input string.
    #[inline]
    #[must_use]
    pub fn new(input: &'a str) -> CodeCursor<'a> {
        Self {
            length: input.len(),
            chars: input.chars(),
            previous: EOF_CHAR,
        }
    }

    /// Returns the slice from the current cursor position to the end of the original underlying
    /// data of the `CodeCursor`.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }

    /// The current position of the cursor in the string.
    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.length - self.remaining()
    }

    /// The amount of characters remaining.
    #[inline]
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.as_str().len()
    }

    /// Get the last 'consumed' `symbol`.
    /// # Returns
    /// The previous `symbol` to be consumed by the stream, or `EOF` if there is no valid previous.
    #[inline]
    #[must_use]
    pub fn prev(&self) -> char {
        self.previous
    }

    /// Peek the next or 'current' character, without `consuming` it. (Without advancing the
    /// cursor).
    /// # Returns
    /// The character at the current cursor position, or the `EOF` char if the position is invalid.
    #[inline]
    pub fn peek(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peek the next or 'current' character, without `consuming` it. (Without advancing the
    /// cursor).
    #[inline]
    pub fn peek_opt(&self) -> Option<char> {
        self.chars.clone().next()
    }

    /// Peek at the character after the current character, without `consuming` (without advancing
    /// the cursor).
    #[inline]
    pub fn peek_second(&self) -> char {
        let mut iter = self.chars.clone();
        iter.next();
        iter.next().unwrap_or(EOF_CHAR)
    }

    /// Peek at the character two characters after the current character, without
    /// `consuming` (without advancing the cursor).
    #[inline]
    pub fn peek_third(&self) -> char {
        let mut iter = self.chars.clone();
        iter.next();
        iter.next();
        iter.next().unwrap_or(EOF_CHAR)
    }

    /// Peek forward a specified number of characters, this does _not_ advance the cursor.
    /// # Returns
    /// The character at the specified `offset` or `None` if the iterator could not be advanced to
    /// that point.
    #[inline]
    pub fn peek_at(&self, offset: usize) -> Option<char> {
        if offset == 0 {
            self.chars.clone().next()
        } else {
            let mut iter = self.chars.clone();
            iter.advance_by(offset).ok()?;
            iter.next()
        }
    }

    /// Compare the string representation of the cursor's remaining data to check if the next
    /// `len(expected)` characters are equal to `expected`.
    #[inline]
    #[must_use]
    pub fn compare_next(&self, expected: &str) -> bool {
        let cursor_str = self.as_str();
        if expected.len() > cursor_str.len() {
            false
        } else {
            &cursor_str[0..expected.len()] == expected
        }
    }

    /// Consume the current cursor position, advancing the cursor by one.
    /// # Returns the next character in the sequence if valid, `None` otherwise.
    #[inline]
    pub fn consume(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.previous = c;
        Some(c)
    }

    /// Advance the cursor over characters until `predicate` returns `false` or the `EOF` is
    /// reached.
    /// # Returns
    /// How many characters were consumed.
    #[inline]
    pub fn consume_while(&mut self, mut predicate: impl FnMut(char) -> bool) -> usize {
        let mut consumed: usize = 0;
        while let Some(c) = self.peek_opt() {
            if predicate(c) {
                self.consume()
                    .expect("Consume should never fail in comsume_while.");
                consumed = consumed.saturating_add(1);
            } else {
                break;
            }
        }
        consumed
    }

    /// Continue to consume characters up until a specified character sequence is found.
    /// This function will _not_ consume the start of the sequence, it peeks, then consumes if
    /// the sequence/pattern is not found.
    /// # Returns
    /// The number of characters consumed before the sequence was found or `eof` was encountered.
    #[inline]
    #[must_use]
    pub fn consume_until_str(&mut self, sequence: &str) -> Option<usize> {
        let mut consumed = 0;
        while !self.is_eof() {
            if self.compare_next(sequence) {
                break;
            }

            self.consume()?;
            consumed += 1;
        }
        Some(consumed)
    }

    #[inline]
    #[must_use]
    pub fn consume_until_sequence(&mut self, sequence: &[char]) -> Option<usize> {
        let mut consumed = 0;
        while !self.is_eof() {
            let mut matched = true;

            for (i, c) in sequence.iter().enumerate() {
                if self.peek_at(i)? != *c {
                    matched = false;
                    break;
                }
            }
            if matched {
                break;
            }

            self.consume()
                .expect("Consume should never fail in consume_until_sequence.");
            consumed += 1;
        }
        Some(consumed)
    }

    /// Advance the cursor over all `whitespace` characters until a `non-whitespace` character is
    /// found.
    /// # Returns
    /// How many characters were consumed.
    #[inline]
    pub fn consume_whitespace(&mut self) -> usize {
        self.consume_while(is_whitespace)
    }

    /// Consume the newline character sequence, this can either be `'\n'` or `"\r\n"`. This
    /// function will consume either 1 or 2 characters depending on the line ending of the input.
    /// The sequence `'\r'` with no newline after it is considered invalid.
    /// # Returns
    /// None if the cursor was not at a newline char, or the number of characters consumed if it
    /// was.
    #[inline]
    pub fn consume_newline(&mut self) -> Option<usize> {
        let c = self.peek();
        if c == '\r' && self.peek_second() != '\n' {
            None
        } else if c == '\r' {
            self.consume()?;
            self.consume()?;
            Some(2)
        } else if c == '\n' {
            self.consume()?;
            Some(1)
        } else {
            None
        }
    }

    /// Consume the current cursor position, advancing the cursor by one for each character in
    /// `expected_chars`, ensures the consumed character was equal to the expected character.
    /// # Returns
    /// The amount of characters consumed, or `None` if one of the expected chars was not found,
    /// or if `eof` was reached.
    #[inline]
    pub fn consume_expect(&mut self, expected_chars: &[char]) -> Option<usize> {
        for c in expected_chars {
            self.consume().filter(|s| *s == *c)?;
        }
        Some(expected_chars.len())
    }

    /// Consume the current cursor position, advancing the cursor by one for each character in
    /// `expected`, checking if the consumed character was equal to the corresponding char in
    /// `expected`.
    /// # Returns
    /// The amount of characters consumed, or `None` if one of the expected chars was not equal,
    /// or if `eof` was reached.
    #[inline]
    pub fn consume_expect_str(&mut self, expected: &str) -> Option<usize> {
        for c in expected.chars() {
            self.consume().filter(|s| *s == c)?;
        }
        Some(expected.len())
    }

    /// Returns true if the cursor as at the end of the file.
    #[inline]
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }
}

/// Returns true if `c` is considered a 'whitespace' character.
#[inline]
#[must_use]
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'   // Tab ('\t')
        | '\u{000A}' // Newline ('\n')
        | '\u{000B}' // Vertical tab
        | '\u{000C}' // Form feed
        | '\u{000D}' // Carriage return (`\r`)
        | '\u{0020}' // Space

        | '\u{0085}' // Next line (latin1)

        | '\u{200E}' // Left-to-right
        | '\u{200F}' // Right-to-left

        | '\u{2028}' // Line separator
        | '\u{2029}' // Paragraph separator
    )
}

/// Returns true if the character sequence describes a comment or doc block.
fn is_comment_sequence(first_char: char, second_char: char) -> bool {
    first_char == '/' && (second_char == '/' || second_char == '*')
}

/// Returns true if the char is considered a newline char (`'\r'`` or `'\n'`)
fn is_newline_char(c: char) -> bool {
    c == '\r' || c == '\n'
}

/// Returns true if the char represents the sign of a number.
fn is_sign_char(c: char) -> bool {
    c == '-' || c == '+'
}

/// Returns true if the char sequence represents a numeric literal.
fn is_numeric_literal_sequence(first_char: char, second_char: char) -> bool {
    if is_sign_char(first_char) {
        second_char.is_numeric()
    } else {
        first_char.is_numeric()
    }
}

/// Holds the state of the lexing process, contains implementation of the lexer.
#[derive(Debug)]
struct Lexer<'a> {
    source_string: &'a str,
    cursor: CodeCursor<'a>,
}

impl<'a> Lexer<'a> {
    /// Construct a new lexer state from an input string.
    pub fn new(source_string: &'a str) -> Lexer<'a> {
        Self {
            source_string,
            cursor: CodeCursor::new(source_string),
        }
    }

    /// Read a token from the current cursor position.
    pub fn read_token(&mut self) -> Option<Token> {
        self.cursor.consume_whitespace();

        let Some(first_char) = self.cursor.peek_opt() else {
            let pos = self.cursor.position();
            self.cursor.consume();
            return Some(Token::new(TokenKind::Eof, pos, pos));
        };

        match first_char {
            c if is_comment_sequence(c, self.cursor.peek_second()) => {
                match self.cursor.peek_second() {
                    '/' => match self.cursor.peek_third() {
                        '/' => self.documentation(),
                        _ => self.line_comment(),
                    },
                    _ => self.block_comment(),
                }
            }
            c if unicode_xid::UnicodeXID::is_xid_start(c) => self.identifier(),
            c if is_numeric_literal_sequence(c, self.cursor.peek_second()) => {
                self.numeric_literal()
            }
            c if Punctuation::is_punctuation(c) => self.punctuation(),
            '"' => self.string_literal(),
            _ => Some(Token::from_kind(TokenKind::Unknown)),
        }
    }
}

// Parsing implementations..
impl Lexer<'_> {
    /// Parses a documentation block from the cursor position.
    /// # Details
    /// Parses every line that starts with '///' until the next line with a non-documentation
    /// or comment token is found, as a documentation token.
    #[inline]
    fn documentation(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        let mut last_line_end_pos = pos;
        let mut total_doc_str = String::new();

        while !self.cursor.is_eof() {
            self.cursor.consume_whitespace();
            if is_newline_char(self.cursor.peek()) {
                // Empty line.. just continue..
                self.cursor.consume_newline()?;
                last_line_end_pos = self.cursor.position();
                continue;
            } else if self.cursor.peek() == '/' {
                if self.cursor.consume_expect_str("///").is_none() {
                    return Some(Token::new(
                        TokenKind::InvalidDocumentation(total_doc_str),
                        pos,
                        self.cursor.position(),
                    ));
                }
            } else {
                break;
            }
            let val_pos = self.cursor.position();
            self.cursor.consume_until_sequence(&['\n'])?;
            let val_end = self.cursor.position();

            let comment_value = &self.source_string[val_pos..val_end].trim();
            // If the doc line is empty we add a new line to the total documentation string.
            if comment_value.is_empty() {
                total_doc_str += "\n";
            } else {
                total_doc_str += comment_value;
                total_doc_str += " ";
            }
            self.cursor.consume_newline()?;
            last_line_end_pos = self.cursor.position();
        }

        Some(Token::new(
            TokenKind::Documentation(total_doc_str.trim().to_string()),
            pos,
            last_line_end_pos,
        ))
    }

    /// Parses a line comment from the cursor.
    /// # Details
    /// Parse everything after `'//'` until a newline as a comment.
    #[inline]
    fn line_comment(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        self.cursor.consume_expect_str("//")?;
        let val_pos = self.cursor.position();

        self.cursor.consume_until_sequence(&['\n'])?;
        let val_end = self.cursor.position();
        self.cursor.consume_expect(&['\n'])?;
        let end = self.cursor.position();

        let comment_value = &self.source_string[val_pos..val_end];

        Some(Token::new(
            TokenKind::Comment(comment_value.trim().to_string()),
            pos,
            end,
        ))
    }

    /// Parses a block comment from the cursor.
    /// # Details
    /// Parses everything between `'/*'` and `'*/` as a comment.
    #[inline]
    fn block_comment(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        self.cursor.consume_expect_str("/*")?;
        let value_pos = self.cursor.position();

        self.cursor.consume_until_str("*/")?;
        let value_end = self.cursor.position();
        self.cursor.consume_expect_str("*/")?;
        let end = self.cursor.position();

        let comment_value = &self.source_string[value_pos..value_end];

        let kind = TokenKind::BlockComment(comment_value.trim().to_string());

        Some(Token::new(kind, pos, end))
    }

    /// Parses a punctuation symbol from the cursor.
    #[inline]
    #[must_use]
    fn punctuation(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        let punct = Punctuation::from_char(self.cursor.consume()?)?;
        Some(Token::new(TokenKind::Punctuation(punct), pos, pos + 1))
    }

    /// Parses an identifier from the cursor.
    #[inline]
    fn identifier(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        let end_pos = pos
            + self
                .cursor
                .consume_while(unicode_xid::UnicodeXID::is_xid_continue);
        let ident_value = &self.source_string[pos..end_pos];

        let kind = if let Some(keyword) = Keyword::from_string(ident_value) {
            TokenKind::Keyword(keyword)
        } else {
            TokenKind::Ident(ident_value.to_string())
        };

        Some(Token::new(kind, pos, end_pos))
    }

    /// Parses a numeric literal from the cursor.
    /// # Details
    /// All support negative sign ('-'), positive sign ('+') and underscores ('_') for delimiting, underscores are to be
    /// read as non-influential characters.
    /// Can be of the form:
    /// *   Binary        ('0b01010101')
    /// *   Hexadecimal   ('0xFA')
    /// *   Octal         ('0o80')
    /// *   Decimal       ('820') and (2.1023)
    /// *   E-notation    ('2.1e6')
    #[inline]
    fn numeric_literal(&mut self) -> Option<Token> {
        // FIXME: Does not allow for whitespace between sign and number.
        let pos = self.cursor.position();
        let (negative, c1, c2) = {
            let negative = self.cursor.peek() == '-';
            if is_sign_char(self.cursor.peek()) {
                self.cursor
                    .consume()
                    .expect("Consuming numeric sign can't fail.");
            }
            (negative, self.cursor.peek(), self.cursor.peek_second())
        };

        let base = if c1 == '0' {
            match c2 {
                'b' => 2,
                'o' => 8,
                'x' => 16,
                _ => 10,
            }
        } else {
            10
        };

        // Consume prefix..
        if base != 10 {
            self.cursor.consume()?;
            self.cursor.consume()?;
        }

        let val_pos = self.cursor.position();
        self.cursor.consume_while(|c| {
            c.is_digit(base) || c == '_' || c == '.' || c == 'e' || c == '-' || c == '+'
        });
        let val_end = self.cursor.position();

        let value_string = self.source_string[val_pos..val_end].replace('_', "");

        let literal = if value_string.contains(['e', '.']) {
            if let Ok(v) = value_string.parse::<f64>() {
                if negative {
                    LiteralKind::Float(-v)
                } else {
                    LiteralKind::Float(v)
                }
            } else {
                return None;
            }
        } else if let Ok(v) = i64::from_str_radix(&value_string, base) {
            if negative {
                LiteralKind::Integer(-v)
            } else {
                LiteralKind::Integer(v)
            }
        } else {
            return None;
        };

        Some(Token::new(TokenKind::Literal(literal), pos, val_end))
    }

    /// Parses a string literal from the cursor.
    #[inline]
    fn string_literal(&mut self) -> Option<Token> {
        let pos = self.cursor.position();
        self.cursor.consume_expect(&['"'])?;

        let val_pos = self.cursor.position();
        self.cursor.consume_while(|c| c != '"');
        let val_end = self.cursor.position();

        if self.cursor.consume_expect(&['"']).is_none() {
            return Some(Token::new(
                TokenKind::InvalidLiteral(self.source_string[val_pos..val_end].to_string()),
                pos,
                val_end,
            ));
        }

        let end_pos = self.cursor.position();
        let string_value = &self.source_string[val_pos..val_end];

        Some(Token::new(
            TokenKind::StringLiteral(string_value.to_string()),
            pos,
            end_pos,
        ))
    }
}

/// Returns `true` if the token kind is a comment, or documentation token.
fn is_comment_token_kind(t: &TokenKind) -> bool {
    matches!(
        t,
        TokenKind::Comment(_)
            | TokenKind::BlockComment(_)
            | TokenKind::Documentation(_)
            | TokenKind::InvalidDocumentation(_)
    )
}

/// Returns an iterator that lazily parses tokens from the source string.
pub fn tokenise(source_string: &str) -> impl Iterator<Item = Token> {
    let mut lexer = Lexer::new(source_string);
    let mut eof: bool = false;
    ::std::iter::from_fn(move || {
        if eof {
            return None;
        }
        let c = lexer.read_token()?;
        eof |= c.kind == TokenKind::Eof;
        Some(c)
    })
}

/// Returns an iterator that lazily parses tokens from the source string, ignoring 'unneccesary'
/// tokens such as comments and documentation.
pub fn tokenise_stripped(source_string: &str) -> impl Iterator<Item = Token> {
    let mut iter = tokenise(source_string);
    ::std::iter::from_fn(move || iter.find(|t| !is_comment_token_kind(&t.kind)))
}
