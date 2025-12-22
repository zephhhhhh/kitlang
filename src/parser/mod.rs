//! The parser module transforms a stream of tokens into an Abstract Syntax Tree.
//!
//! It provides recursive descent parsing for expressions, statements, and items.
//!
//! The parser maintains a [`TokenCursor`] to track position in the token stream and provides utilities
//! for parsing paths, blocks, and structured language constructs.
//!
//! Parse a Kitlang program from source code using the [`parse_from_tokens`] and [`parse_from_source`] which produce
//! an [`ASTRoot`] on success or a [`ParseError`] on failure.

mod errors;
mod expr;
mod item;
mod statement;

#[cfg(doc)]
use crate::ast::{Expression, Item, Statement};

use ::std::ops::Range;

use crate::ast::{
    ASTRoot, IdentPath, IdentPathSegments, Mutability, SourceSpan, SpannedIdent, SpannedIdentPath,
    Ty, Visibility,
};

use crate::lexer::tokenise_stripped;
use crate::parser::errors::PResult;
use crate::token::{Keyword, Punctuation, Token, TokenKind};

pub use crate::parser::errors::{ParseError, ParseErrorKind};

pub type TokenList = Vec<Token>;

/// A cursor into a list of tokens, provides functionality for peeking as well as consuming tokens
/// in a linear order.
#[derive(Debug)]
pub struct TokenCursor<'a> {
    pub tokens: &'a TokenList,
    pub position: u32,
}

impl<'a> TokenCursor<'a> {
    #[inline]
    #[must_use]
    pub const fn new(tokens: &'a TokenList) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }
}

// Accessors..
impl TokenCursor<'_> {
    /// Returns the total number tokens in the underlying [`TokenList`].
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.tokens.len() as u32
    }

    /// Returns if the total number of tokens in the underlying [`TokenList`] is `zero`.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Returns the current position of the [`TokenCursor`] as an index.
    #[inline]
    #[must_use]
    pub const fn position(&self) -> u32 {
        self.position
    }

    /// Returns the total number of tokens remaining until the end of the [`TokenList`].
    #[inline]
    #[must_use]
    pub const fn remaining(&self) -> u32 {
        self.len() - self.position
    }

    /// Returns true if the current cursor position is at the end of the [`TokenList`].
    #[inline]
    #[must_use]
    pub const fn is_end(&self) -> bool {
        self.position >= self.len()
    }

    /// Returns a reference to a [`Token`] at a specified absolute index into the [`TokenList`].
    #[inline]
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&Token> {
        self.tokens.get(index as usize)
    }

    /// Returns the end of the current token.
    #[inline]
    #[must_use]
    pub fn get_current_source_end(&self) -> u32 {
        self.get(self.position()).map_or_else(
            || self.tokens.last().map_or(0, |token| token.span.end),
            |token| token.span.end,
        )
    }

    /// Returns the start of the current token.
    #[inline]
    #[must_use]
    pub fn get_current_source_start(&self) -> u32 {
        self.get(self.position()).map_or_else(
            || self.tokens.last().map_or(0, |token| token.span.start),
            |token| token.span.start,
        )
    }

    /// Returns the start of the previous token.
    #[inline]
    #[must_use]
    pub fn get_previous_source_start(&self) -> u32 {
        self.get(self.position().saturating_sub(1))
            .map_or_else(|| self.get_current_source_start(), |token| token.span.start)
    }

    /// Returns the end of the previous token.
    #[inline]
    #[must_use]
    pub fn get_previous_source_end(&self) -> u32 {
        self.get(self.position().saturating_sub(1))
            .map_or_else(|| self.get_current_source_end(), |token| token.span.end)
    }

    /// Returns a range that represents the start token.
    #[inline]
    #[must_use]
    pub fn first_span(&self) -> Range<u32> {
        self.tokens
            .first()
            .map_or(0..0, |token| token.span.start..token.span.end)
    }

    /// Returns a range that represents the end of the file.
    #[inline]
    #[must_use]
    pub fn eof_span(&self) -> Range<u32> {
        self.tokens
            .last()
            .map_or(0..0, |token| token.span.start..token.span.end)
    }

    /// Returns a range that represents the current token index.
    #[inline]
    #[must_use]
    pub fn current_span(&self) -> Range<u32> {
        self.get(self.position())
            .map_or_else(|| self.eof_span(), |token| token.span.start..token.span.end)
    }

    /// Returns a range that represents the full file of tokens.
    /// From the first character of the first token, to the last character of the final token.
    #[inline]
    #[must_use]
    pub fn full_span(&self) -> Range<u32> {
        let Some(first_token) = self.tokens.first() else {
            return self.eof_span();
        };
        let Some(last_token) = self.tokens.last() else {
            return self.eof_span();
        };
        (first_token.span.start)..(last_token.span.end)
    }
}

impl TokenCursor<'_> {
    /// Advance the token cursor by one position
    /// # Returns
    /// `true` if the cursor could advance, `false` otherwise.
    #[inline]
    pub const fn advance(&mut self) -> bool {
        if self.position < self.len() {
            self.position = self.position.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Advance the token cursor by `count` positions.
    /// # Returns
    /// The number of tokens the cursor was advanced by, may be less if reaching the end.
    #[inline]
    pub fn advance_by(&mut self, count: u32) -> u32 {
        let adjusted = count.min(self.remaining());
        self.position = self.position.saturating_add(adjusted);
        adjusted
    }

    /// Get the current token and advance the cursor by one position.
    #[inline]
    pub fn consume(&mut self) -> Option<&Token> {
        if self.position >= self.len() {
            None
        } else {
            let position = self.position;
            self.advance();
            self.get(position)
        }
    }

    /// Peek at the current token _without_ advancing the cursor position.
    #[inline]
    #[must_use]
    pub fn peek(&self) -> Option<&Token> {
        self.get(self.position)
    }

    /// Peek `ahead` number of tokens ahead of the current cursor position, _without_ advancing the
    /// cursor position.
    #[inline]
    #[must_use]
    pub fn peek_at(&self, ahead: u32) -> Option<&Token> {
        self.get(self.position.saturating_add(ahead))
    }
}

/// A sequence of [`Token`]'s
#[derive(Debug)]
pub struct TokenStream(pub Vec<Token>);

/// Context/Information about a parsing session.
/// Contains things such as Diagnostic settings, warning settings, etc..
#[derive(Debug)]
pub struct ParserContext {}

/// Contains the state needed to parse an input file, as well as methods to parse different [`Item`]s,
/// [`Expression`]s and [`Statement`]s.
#[derive(Debug)]
pub(crate) struct Parser<'a, 'b> {
    // TODO: Add functionality to this.
    #[allow(dead_code)]
    context: &'a ParserContext,

    cursor: TokenCursor<'b>,
}

impl<'a, 'b> Parser<'a, 'b> {
    #[inline]
    #[must_use]
    pub const fn from_cursor(token_cursor: TokenCursor<'b>, context: &'a ParserContext) -> Self {
        Self {
            context,
            cursor: token_cursor,
        }
    }
}

/// Describes a type of reference.
/// Contains the result of parsing `&` or `&mut` from the [`TokenStream`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefType {
    /// No specified reference.
    None,
    /// An immutable reference.
    Ref,
    /// A mutable reference.
    RefMut,
}

impl RefType {
    /// Return the [`Mutability`] of the [`RefType`].
    /// I.e. `RefMut` = `Mutability::Mutable`, `Ref` & `None` = `Mutability::Immutable`.
    #[inline]
    #[must_use]
    pub const fn mutability(self) -> Mutability {
        match self {
            Self::RefMut => Mutability::Mutable,
            _ => Mutability::Immutable,
        }
    }

    /// Returns `true` if `self` is a `reference`.
    #[inline]
    #[must_use]
    pub const fn is_ref(self) -> bool {
        matches!(self, Self::Ref | Self::RefMut)
    }
}

impl Parser<'_, '_> {
    /// Returns true if the next 2 tokens from the cursor position are [`Punctuation::Colon`].
    #[inline]
    #[must_use]
    fn is_double_colon(&self) -> bool {
        self.check_kind_at(0, Punctuation::Colon) && self.check_kind_at(1, Punctuation::Colon)
    }

    /// Parses a double colon `"::"` or the "path" separator from the cursor position.
    ///
    /// This function advances the cursor position by the length of the path separator (2) if the
    /// double colon is found.
    /// # Returns
    /// *   `Ok(true)` if a double colon was found and the cursor was advanced.
    /// *   `Ok(false)` if a double colon wasn't found, but the token at the current cursor position
    ///     _was not_ a single colon `':'`, and the cursor was not advanced.
    /// *   `Err(ParseErrorKind::InvalidPath)` if the double colon was not found, but a single
    ///     colon was, indicating a malformed path.
    fn parse_double_colon(&mut self, path_start: u32) -> PResult<bool> {
        if self.is_double_colon() {
            self.cursor.advance_by(2);
            Ok(true)
        } else {
            if self.check_kind(Punctuation::Colon) {
                return Err(ParseError::new(
                    ParseErrorKind::InvalidPath,
                    path_start..self.cursor.get_current_source_end(),
                ));
            }
            Ok(false)
        }
    }

    pub fn parse_path_segment(&mut self) -> PResult<String> {
        let token = self.peek()?.clone();
        match token.kind {
            TokenKind::Ident(ref ident_value) => {
                self.cursor.advance();
                Ok(ident_value.clone())
            }
            TokenKind::Keyword(Keyword::ThisTy) => {
                self.cursor.advance();
                Ok("Self".to_string())
            }
            k => Err(ParseError::new(
                ParseErrorKind::ExpectedIdentifier(k),
                self.cursor.current_span(),
            )),
        }
    }

    /// Parse a 'path' (I.e. `std::math::powf`) from the current cursor position,
    /// into a path segment representation.
    /// # Notes
    /// Always expects atleast one `identifier`, can optionally start with '::', to force the path
    /// to reference absolutely from the 'base'.
    pub fn parse_path(&mut self) -> PResult<IdentPath> {
        let path_start = self.cursor.get_current_source_start();
        let mut path_segments = IdentPathSegments::new();
        let root_relative = self.parse_double_colon(path_start)?;

        path_segments.push(self.parse_path_segment()?);

        while !self.cursor.is_end() {
            if !self.parse_double_colon(path_start)? {
                break;
            }

            // Testing
            if self.check_kind(Punctuation::OpenBrace) {
                break;
            }

            path_segments.push(self.parse_path_segment()?);
        }

        Ok(IdentPath::from_segments(path_segments, root_relative))
    }

    /// Parse a 'path' (I.e. `std::math::powf`) from the current cursor position,
    /// into a path segment representation, also returns the span of the parsed path.
    /// # Notes
    /// Always expects atleast one `identifier`, can optionally start with '::', to force the path
    /// to reference absolutely from the 'base'.
    pub fn parse_spanned_path(&mut self) -> PResult<SpannedIdentPath> {
        let path_span_start = self.begin_span();
        let path = self.parse_path()?;
        Ok(SpannedIdentPath {
            path,
            span: self.finish_span(path_span_start),
        })
    }

    /// Parses a `&` and `&mut`
    pub fn parse_ref_and_refmut(&mut self) -> PResult<RefType> {
        if self.check_kind_advance_err(Punctuation::Ampersand)? {
            if self.check_kind_advance_err(Keyword::Mut)? {
                Ok(RefType::RefMut)
            } else {
                Ok(RefType::Ref)
            }
        } else {
            // TODO: Check if next token is 'mut', if so, return an error.
            Ok(RefType::None)
        }
    }

    /// Parse a tuple type from the current cursor position.
    /// (Ty, Ty, ...)
    /// # Notes
    /// Must be verified the type being parsed is _NOT_ a unit type `()` by the caller.
    pub fn parse_tuple_ty(&mut self) -> PResult<Ty> {
        let span_start = self.begin_span();
        self.expect_kind(Punctuation::OpenParen)?;

        let mut tuple_types = vec![Box::new(self.parse_ty()?)];

        while self.check_kind_advance(Punctuation::Comma) {
            if self.check_kind(Punctuation::CloseParen) {
                break;
            }
            tuple_types.push(Box::new(self.parse_ty()?));
        }

        self.expect_kind(Punctuation::CloseParen)?;
        Ok(Ty::Tuple(tuple_types, self.finish_span(span_start)))
    }

    /// Parse a [`Ty`] from the current cursor position.
    /// # Notes
    /// Always expects atleast one `Identifier`.
    pub fn parse_ty(&mut self) -> PResult<Ty> {
        let span_start = self.begin_span();
        let ref_type = self.parse_ref_and_refmut()?;

        let root_type = match self.peek()?.kind {
            TokenKind::Punctuation(Punctuation::OpenParen) => {
                if self.check_kind_at(1, Punctuation::CloseParen) {
                    let unit_start_span = self.begin_span();
                    self.cursor.advance_by(2);
                    Ty::Unit(self.finish_span(unit_start_span))
                } else {
                    self.parse_tuple_ty()?
                }
            }
            TokenKind::Keyword(Keyword::ThisTy) => {
                let self_span_start = self.begin_span();
                self.cursor.advance();
                Ty::This(self.finish_span(self_span_start))
            }
            _ => Ty::new(self.parse_spanned_path()?),
        };

        if ref_type.is_ref() {
            Ok(Ty::Ref(
                Box::new(root_type),
                ref_type.mutability(),
                self.finish_span(span_start),
            ))
        } else {
            Ok(root_type)
        }
    }

    /// Parse the type identifier for an `impl` block.
    pub fn parse_impl_ty_ident(&mut self) -> PResult<SpannedIdentPath> {
        // Case for unit type "()".
        if self.check_kind(Punctuation::OpenParen) && self.check_kind_at(1, Punctuation::CloseParen)
        {
            let unit_span_start = self.begin_span();
            self.cursor.advance_by(2);
            let unit_path = IdentPath::from_segments_slice(&["()".to_string()], false);

            return Ok(SpannedIdentPath::new(
                unit_path,
                self.finish_span(unit_span_start),
            ));
        }

        self.parse_spanned_path()
    }

    /// Checks if the current keyword is `pub`, if it is, consume it and return `Ok(Visibility::Public)`,
    /// otherwise `Ok(Visibility::Private)`, If there are no tokens, return `Err`.
    pub fn parse_visibility(&mut self) -> PResult<Visibility> {
        Ok(Visibility::from_is_public(
            self.check_kind_advance_err(Keyword::Pub)?,
        ))
    }

    /// Checks if the current keyword is `mut`, if it is, consume it and return `Ok(Mutability::Mutable)`,
    /// otherwise `Ok(Mutability::Immutable)`, If there are no tokens, return `Err`.
    pub fn parse_mutability(&mut self) -> PResult<Mutability> {
        let mutable = matches!(self.peek()?.kind, TokenKind::Keyword(Keyword::Mut));
        if mutable {
            self.cursor.advance();
        }
        Ok(Mutability::from_is_mutable(mutable))
    }

    /// Parse the 'root' AST, starting from the beginning of the file.
    /// This is the function you want to call to fully parse a token stream.
    pub fn parse_root(&mut self) -> PResult<ASTRoot> {
        let mut root_ast = ASTRoot::new_with_span(self.cursor.full_span().into());

        while !self.cursor.is_end() {
            let Some((_offset, token)) = self.find_next_significant_token()? else {
                break;
            };

            if matches!(token.kind, TokenKind::Eof) {
                self.cursor.advance();
            } else if token.kind.can_start_item() {
                root_ast.push_item(self.parse_item()?);
            } else {
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedItem(token.kind.clone()),
                    token,
                ));
            }
        }

        Ok(root_ast)
    }
}

// Checking..
impl Parser<'_, '_> {
    /// Peek at the current token, return `true` if the [`TokenKind`] of the [`Token`] matches the
    /// `expected_kind`
    #[inline]
    fn check_kind<T: Into<TokenKind>>(&self, expected_kind: T) -> bool {
        let expected = expected_kind.into();
        self.cursor.peek().is_some_and(|t| t.kind == expected)
    }

    /// Peek at the specified `offset`, return `true` if the [`TokenKind`] of the [`Token`] matches the
    /// `expected_kind`
    #[inline]
    #[allow(dead_code)]
    fn check_kind_at_err<T: Into<TokenKind>>(
        &self,
        offset: u32,
        expected_kind: T,
    ) -> PResult<bool> {
        let expected = expected_kind.into();
        Ok(self.peek_at(offset)?.kind == expected)
    }

    /// Peek at the current token, return `true` if the [`TokenKind`] of the [`Token`] matches the
    /// `expected_kind`
    #[inline]
    fn check_kind_err<T: Into<TokenKind>>(&self, expected_kind: T) -> PResult<bool> {
        let expected = expected_kind.into();
        Ok(self.peek()?.kind == expected)
    }

    /// Peek at the specified `offset`, return `true` if the [`TokenKind`] of the [`Token`] matches the
    /// `expected_kind`
    #[inline]
    fn check_kind_at<T: Into<TokenKind>>(&self, offset: u32, expected_kind: T) -> bool {
        let expected = expected_kind.into();
        self.cursor
            .peek_at(offset)
            .is_some_and(|t| t.kind == expected)
    }

    /// Peek at the current token, return `true` and advance the cursor if the [`TokenKind`] of the [`Token`]
    /// matches the `expected_kind`
    #[inline]
    fn check_kind_advance<T: Into<TokenKind>>(&mut self, expected_kind: T) -> bool {
        if self.check_kind(expected_kind) {
            self.cursor.advance();
            true
        } else {
            false
        }
    }

    /// Peek at the current token, return `true` and advance the cursor if the [`TokenKind`] of the [`Token`]
    /// matches the `expected_kind`
    #[inline]
    fn check_kind_advance_err<T: Into<TokenKind>>(&mut self, expected_kind: T) -> PResult<bool> {
        if self.check_kind_err(expected_kind)? {
            self.cursor.advance();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Peek at the current token, return `true` if the [`TokenKind`] of the [`Token`]
    /// is an `Identifier`.
    #[inline]
    fn check_ident(&self) -> bool {
        self.cursor
            .peek()
            .is_some_and(|t| matches!(t.kind, TokenKind::Ident(_)))
    }

    /// Peek at an offset from the current token, return `true` if the [`TokenKind`] of the [`Token`]
    /// is an `Identifier`.
    #[inline]
    fn check_ident_at(&self, offset: u32) -> bool {
        self.cursor
            .peek_at(offset)
            .is_some_and(|t| matches!(t.kind, TokenKind::Ident(_)))
    }

    /// Peek at the current token, check if the [`TokenKind`] of the [`Token`]
    /// is an `Identifier`.
    /// # Returns
    /// `Some(ident_value)` and advance the cursor if it is an `Identifier`,
    /// else `None`.
    #[inline]
    fn check_ident_advance(&mut self) -> Option<String> {
        if self.check_ident() {
            self.cursor.consume().and_then(|t| {
                if let TokenKind::Ident(ident_value) = &t.kind {
                    Some(ident_value.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a sequence of [`Punctuation`] tokens of length `N` from a specified offset from the current cursor
    /// position.
    /// # Returns
    /// *   `Some(arr)` if the next `N` tokens from the specified offset are all [`Punctuation`] token kinds.
    /// *   `None` if any of the next `N` tokens from the specified offset are _not_ of kind [`Punctuation`].
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn get_punctuation_sequence<const N: usize>(&self, offset: u32) -> Option<[Punctuation; N]> {
        let mut result = [Punctuation::Tilde; N];
        for (i, out_p) in result.iter_mut().enumerate() {
            let token = self.cursor.peek_at(offset + i as u32)?;

            if let TokenKind::Punctuation(p) = token.kind {
                *out_p = p;
            } else {
                return None;
            }
        }
        Some(result)
    }
}

// Expect..
impl Parser<'_, '_> {
    /// Expect that the token at the current cursor position is of a specified [`TokenKind`].
    ///
    /// This function will advance the cursor by one position if the `expected_kind` is found.
    /// # Returns
    /// *   `Ok(())` if the token at the cursor position is of the `expected_kind`.
    /// *   `Err(ParseErrorKind::ExpectedToken)` if the token found at the current cursor position
    ///     does not match the `expected_kind`.
    /// *   `Err(ParseErrorKind::ExpectedTokenFoundNone)` if there are no more tokens in the
    ///     stream, but it was expected that there were.
    #[inline]
    fn expect_kind<T: Into<TokenKind>>(&mut self, expected_kind: T) -> PResult<()> {
        let expected = expected_kind.into();
        if self.check_kind_advance(expected.clone()) {
            Ok(())
        } else if let Some(token) = self.cursor.peek() {
            Err(ParseError::new(
                ParseErrorKind::ExpectedToken(expected, token.kind.clone()),
                token,
            ))
        } else {
            Err(ParseError::new(
                ParseErrorKind::ExpectedTokenFoundNone(expected),
                self.cursor.eof_span(),
            ))
        }
    }

    /// Expect that the token at the current cursor position is an `Identifier`.
    ///
    /// This function will advance the cursor by one position if an `Identifier` is found.
    /// # Returns
    /// *   `Ok(ident_str)` if the token at the cursor position is an `Identifier`.
    /// *   `Err(ParseErrorKind::ExpectedIdentifier)` if the token found at the current cursor position
    ///     was not an `Identifier`.
    /// *   `Err(ParseErrorKind::ExpectedIdentifierFoundNone)` if there are no more tokens in the
    ///     stream, but it was expected that there were.
    #[inline]
    fn expect_ident(&mut self) -> PResult<String> {
        self.check_ident_advance().ok_or_else(|| {
            if let Some(token) = self.cursor.peek() {
                ParseError::new(
                    ParseErrorKind::ExpectedIdentifier(token.kind.clone()),
                    token,
                )
            } else {
                ParseError::new(
                    ParseErrorKind::ExpectedIdentifierFoundNone,
                    self.cursor.eof_span(),
                )
            }
        })
    }

    /// Expect that the token at the current cursor position is an `Identifier`.
    ///
    /// This function will advance the cursor by one position if an `Identifier` is found.
    /// # Returns
    /// *   `Ok((ident_str, span))` if the token at the cursor position is an `Identifier`.
    /// *   `Err(ParseErrorKind::ExpectedIdentifier)` if the token found at the current cursor position
    ///     was not an `Identifier`.
    /// *   `Err(ParseErrorKind::ExpectedIdentifierFoundNone)` if there are no more tokens in the
    ///     stream, but it was expected that there were.
    #[inline]
    fn expect_ident_spanned(&mut self) -> PResult<SpannedIdent> {
        let span_start = self.begin_span();
        let ident = self.expect_ident()?;
        Ok(SpannedIdent::new(ident, self.finish_span(span_start)))
    }
}

// Spans..
impl Parser<'_, '_> {
    /// Start a span at the beginning of the current token.
    /// Shorthand for `self.cursor.get_current_source_start`.
    #[inline]
    #[must_use]
    pub fn begin_span(&self) -> u32 {
        self.cursor.get_current_source_start()
    }

    /// Construct a span by using a supplied beginning position, and the end of the previous token
    /// as the end position.
    /// Shorthand for `self.cursor.get_current_source_start`.
    #[inline]
    #[must_use]
    pub fn finish_span(&self, begin: u32) -> SourceSpan {
        (begin..self.cursor.get_previous_source_end()).into()
    }
}

// Implementation details..
impl Parser<'_, '_> {
    /// Checks if the token is valid, returning an `Err` if it isn't.
    /// # Returns
    /// *   `Ok(())` if the token is valid,
    /// *   `Err(ParseError)` if the token is invalid.
    fn check_token_invalid(token: &Token) -> PResult<()> {
        match &token.kind {
            TokenKind::InvalidIdent(ident) => Err(ParseError::new(
                ParseErrorKind::InvalidIdentifier(ident.clone()),
                token,
            )),
            TokenKind::InvalidLiteral(source) => Err(ParseError::new(
                ParseErrorKind::InvalidLiteral(source.clone()),
                token,
            )),
            TokenKind::Unknown => Err(ParseError::new(ParseErrorKind::UnknownToken, token)),
            _ => Ok(()),
        }
    }

    /// Find the next significant token, I.e. A token that isn't `mut` or something similar, and
    /// that may identify which variant we should try to parse.
    /// # Returns
    /// *   `Ok(Some(offset_from_current_token, significant_token))` if there is no error found while
    ///     checking tokens.
    /// *   `Ok(None)` if no significant token was found, but there were no errors either.
    /// *   `Err(ParseError)` if an invalid token was found while checking next tokens.
    fn find_next_significant_token(&self) -> PResult<Option<(u32, &Token)>> {
        self.find_next_significant_token_with_offset(0)
    }

    /// Find the next significant token, I.e. A token that isn't `mut` or something similar, and
    /// that may identify which variant we should try to parse, starting from a beginning offset
    /// from the current cursor position `offset`.
    /// # Returns
    /// *   `Ok(Some(offset_from_current_token, significant_token))` if there is no error found while
    ///     checking tokens.
    /// *   `Ok(None)` if no significant token was found, but there were no errors either.
    /// *   `Err(ParseError)` if an invalid token was found while checking next tokens.
    fn find_next_significant_token_with_offset(
        &self,
        offset: u32,
    ) -> PResult<Option<(u32, &Token)>> {
        let remaining = self.cursor.remaining().saturating_sub(offset);
        for o in 0..remaining {
            if let Some(t) = self.cursor.peek_at(offset + o) {
                Self::check_token_invalid(t)?;
                if t.kind.is_significant() {
                    return Ok(Some((offset + o, t)));
                }
            } else {
                break;
            }
        }
        Ok(None)
    }

    /// Find the next significant token, I.e. A token that isn't `mut` or something similar, and
    /// that may identify which variant we should try to parse, starting from a beginning offset
    /// from the current cursor position `offset`.
    /// # Returns
    /// *   `Ok(offset_from_current_token, significant_token)` if there is no error found while
    ///     checking tokens.
    /// *   `Err(ParseError)` if an invalid token was found while checking next tokens, or if there
    ///     were no more tokens available.
    #[inline]
    fn expect_next_significant_token_with_offset(&self, offset: u32) -> PResult<(u32, &Token)> {
        self.find_next_significant_token_with_offset(offset)?
            .ok_or_else(|| self.no_token_error())
    }

    /// Find the next significant token, I.e. A token that isn't `mut` or something similar, and
    /// that may identify which variant we should try to parse.
    /// # Returns
    /// *   `Ok(offset_from_current_token, significant_token)` if there is no error found while
    ///     checking tokens.
    /// *   `Err(ParseError)` if an invalid token was found while checking next tokens, or if there
    ///     were no more tokens available.
    #[inline]
    fn expect_next_significant_token(&self) -> PResult<(u32, &Token)> {
        self.expect_next_significant_token_with_offset(0)
    }
}

// Parsing 'block' like structures.
impl Parser<'_, '_> {
    /// Parses a 'block'-like pattern, where the expected token stream to parse resembles:
    /// ```ignore
    /// <OpenPunctuation>
    ///     <Element>+
    /// <ClosePunctuation>
    /// ````
    /// # Notes
    /// This function expects that the `<OpenPunctuation>` has already been consumed and that the
    /// cursor is at the start of the first element in the block.
    ///
    /// This function allows for empty blocks.
    /// # Generic Parameters
    /// `T` is the type of the element the block will parse.
    /// `F` is a function that will actually do the parsing of each element, and the function is
    /// passed `self`.
    fn parse_block_like_no_delimiter<T, F: FnMut(&mut Self) -> PResult<T>>(
        &mut self,
        close: Punctuation,
        mut f: F,
    ) -> PResult<Vec<T>> {
        let mut elements = Vec::new();

        if !self.check_kind_advance(close) {
            elements.push(f(self)?);

            while !self.cursor.is_end() {
                if self.check_kind_advance(close) {
                    break;
                }
                elements.push(f(self)?);
            }
        }

        Ok(elements)
    }

    /// Parses a 'block'-like pattern, where the expected token stream to parse resembles:
    /// ```ignore
    /// <OpenPunctuation>
    ///     <Element><Delimiter>
    ///     <Element><Delimiter>?
    /// <ClosePunctuation>
    /// ````
    /// # Notes
    /// This function expects that the `<OpenPunctuation>` has already been consumed and that the
    /// cursor is at the start of the first element in the block.
    ///
    /// This function allows for empty blocks.
    /// # Generic Parameters
    /// If `TRAIL_DELIM` is set `true`, the algorithm will allow a trailing delimiter after the
    /// last element but before the closing [`Punctuation`], otherwise this will cause an error.
    /// `T` is the type of the element the block will parse.
    /// `F` is a function that will actually do the parsing of each element, and the function is
    /// passed `self`.
    fn parse_block_like_impl<const TRAIL_DELIM: bool, T, F: FnMut(&mut Self) -> PResult<T>>(
        &mut self,
        delimiter: Punctuation,
        close: Punctuation,
        mut f: F,
    ) -> PResult<Vec<T>> {
        let mut elements = Vec::new();

        if !self.check_kind_advance(close) {
            elements.push(f(self)?);

            while !self.cursor.is_end() {
                if self.check_kind_advance(close) {
                    break;
                }
                if TRAIL_DELIM && self.check_kind(delimiter) && self.check_kind_at(1, close) {
                    self.cursor.advance_by(2);
                    break;
                }
                self.expect_kind(delimiter)?;

                elements.push(f(self)?);
            }
        }

        Ok(elements)
    }

    /// Parses a 'block'-like pattern, where the expected token stream to parse resembles:
    /// ```ignore
    /// <OpenPunctuation>
    ///     <Element><Delimiter>
    ///     <Element><Delimiter>?
    /// <ClosePunctuation>
    /// ````
    /// # Notes
    /// This function expects that the `<OpenPunctuation>` has already been consumed and that the
    /// cursor is at the start of the first element in the block.
    ///
    /// This function allows for empty blocks.
    ///
    /// This function allows for trailing delimiters on the last element before the end of the
    /// `block`.
    /// # Generic Parameters
    /// `T` is the type of the element the block will parse.
    /// `F` is a function that will actually do the parsing of each element, and the function is
    /// passed `self`.
    #[inline]
    fn parse_block_like<T, F: FnMut(&mut Self) -> PResult<T>>(
        &mut self,
        delimiter: Punctuation,
        close: Punctuation,
        f: F,
    ) -> PResult<Vec<T>> {
        self.parse_block_like_impl::<true, T, F>(delimiter, close, f)
    }

    /// Parses a 'block'-like pattern, where the expected token stream to parse resembles:
    /// ```ignore
    /// <OpenPunctuation>
    ///     <Element><Delimiter>
    ///     <Element><Delimiter>?
    /// <ClosePunctuation>
    /// ````
    /// # Notes
    /// This function expects that the `<OpenPunctuation>` has already been consumed and that the
    /// cursor is at the start of the first element in the block.
    ///
    /// This function allows for empty blocks.
    ///
    /// This function does not allow for trailing delimiters on the last element before the end of the
    /// `block`.
    /// # Generic Parameters
    /// `T` is the type of the element the block will parse.
    /// `F` is a function that will actually do the parsing of each element, and the function is
    /// passed `self`.
    #[inline]
    #[allow(dead_code)]
    fn parse_block_like_no_trail<T, F: FnMut(&mut Self) -> PResult<T>>(
        &mut self,
        delimiter: Punctuation,
        close: Punctuation,
        f: F,
    ) -> PResult<Vec<T>> {
        self.parse_block_like_impl::<false, T, F>(delimiter, close, f)
    }
}

impl Parser<'_, '_> {
    /// Returns an error with the specified error `kind`, that is for the `token` currently pointed
    /// to by the cursor.
    #[inline]
    #[must_use]
    fn make_error(&self, kind: ParseErrorKind) -> ParseError {
        ParseError::new(kind, self.cursor.current_span())
    }

    /// Returns an error representing that the token stream ended unexpectedly.
    #[inline]
    #[must_use]
    fn no_token_error(&self) -> ParseError {
        ParseError::no_tokens(self.cursor.eof_span())
    }

    /// Returns the token at the offset, or [`ParseErrorKind::NoTokens`] otherwise.
    #[inline]
    fn peek_at(&self, offset: u32) -> PResult<&Token> {
        self.cursor
            .peek_at(offset)
            .ok_or_else(|| self.no_token_error())
    }

    /// Returns the token at the current cursor position, or [`ParseErrorKind::NoTokens`] otherwise.
    #[inline]
    fn peek(&self) -> PResult<&Token> {
        self.cursor.peek().ok_or_else(|| self.no_token_error())
    }
}

/// Parse AST from a [`Token`] iterator.
/// # Note
/// This function will collect the iterator into a [`Vec<Token>`].
/// # Errors
/// This function will return an error if any part of parsing the token stream fails.
/// The returned error will contain information about where in the source code the error occurred,
/// and a message about the nature of the error.
pub fn parse_from_tokens(tokens: impl Iterator<Item = Token>) -> PResult<ASTRoot> {
    // TODO: Probably try a method to not collect the iterator in the future, this is easier for
    // now though.
    let token_list = tokens.collect::<Vec<Token>>();
    let context = ParserContext {};

    let mut parser = Parser::from_cursor(TokenCursor::new(&token_list), &context);

    parser.parse_root()
}

/// Parse AST from a source code string.
/// # Note
/// This function will collect the iterator into a [`Vec<Token>`].
///
/// This function also strips all comments and documentation from the code.
/// # Errors
/// This function will return an error if any part of parsing the token stream fails.
/// The returned error will contain information about where in the source code the error occurred,
/// and a message about the nature of the error.
#[inline]
pub fn parse_from_source(source: &str) -> PResult<ASTRoot> {
    let token_iter = tokenise_stripped(source);
    parse_from_tokens(token_iter)
}
