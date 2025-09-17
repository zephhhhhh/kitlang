// Implemented with a macro for flexibility and for it to be more declarative.
macro_rules! define_punctuation {
    (
        $(
            ($punct_name: ident, $punct_char: literal)
        ),*
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
        pub enum Punctuation {
            $($punct_name),*
        }

        impl Punctuation {
            /// Returns true if the provided character is a valid punctuation character.
            #[inline]
            #[must_use]
            pub const fn is_punctuation(c: char) -> bool {
                matches!(c, $($punct_char)|+)
            }

            /// Attempts to parse a `Punctuation` from a character.
            /// # Returns
            /// The `Punctuation` corresponding with the char `c`, or None if no corresponding
            /// `Punctuation` can be established.
            #[inline]
            #[must_use]
            pub fn from_char(c: char) -> Option<Self> {
                match c {
                    $($punct_char => Some(Self::$punct_name),)+
                    _ => None
                }
            }

            /// Convert a `Punctuation` into the corresponding `char` value.
            #[inline]
            #[must_use]
            pub fn to_char(self) -> char {
                match self {
                    $(Self::$punct_name => $punct_char,)+
                }
            }
        }
    };
}

macro_rules! define_keywords {
    (
        $(
            ($keyword_name: ident, $keyword_str: literal)
        ),+
    ) =>{
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
        pub enum Keyword {
            $($keyword_name),*
        }

        impl Keyword {
            /// Returns true if the provided string slice `s` is a valid keyword.
            #[inline]
            #[must_use]
            pub fn is_keyword(s: &str) -> bool {
                matches!(s, $($keyword_str)|+)
            }

            /// Attempts to parse a `Keyword` from a string slice.
            /// # Returns
            /// The `Keyword` corresponding with the input string slice `s`, or None if no corresponding
            /// `Keyword` can be established.
            #[inline]
            #[must_use]
            pub fn from_string(s: &str) -> Option<Self> {
                match s {
                    $($keyword_str => Some(Self::$keyword_name),)+
                    _ => None
                }
            }

            /// Convert a `Keyword` into the corresponding `string slice` value.
            #[inline]
            #[must_use]
            pub fn to_str(self) -> &'static str {
                match self {
                    $(Self::$keyword_name => $keyword_str,)+
                }
            }
        }
    };
}

define_punctuation!(
    (SemiColon, ';'),
    (Colon, ':'),
    (Dot, '.'),
    (Comma, ','),
    (Question, '?'),
    (Bang, '!'),
    (Eq, '='),
    (LessThan, '<'),
    (GreaterThan, '>'),
    (Plus, '+'),
    (Minus, '-'),
    (Star, '*'),
    (Slash, '/'),
    (Caret, '^'),
    (Ampersand, '&'),
    (Or, '|'),
    (OpenParen, '('),
    (CloseParen, ')'),
    (OpenBrace, '{'),
    (CloseBrace, '}'),
    (OpenBracket, '['),
    (CloseBracket, ']'),
    (At, '@'),
    (Hashtag, '#'),
    (Tilde, '~'),
    (Dollar, '$'),
    (Percent, '%')
);

define_keywords!(
    (Mod, "mod"),
    (Use, "use"),
    (Pub, "pub"),
    (Global, "global"),
    (Let, "let"),
    (Const, "const"),
    (If, "if"),
    (Else, "else"),
    (For, "for"),
    (In, "in"),
    (While, "while"),
    (Break, "break"),
    (Continue, "continue"),
    (Return, "return"),
    (Match, "match"),
    (Fn, "fn"),
    (Native, "native"),
    (Mut, "mut"),
    (As, "as"),
    (Enum, "enum"),
    (Struct, "struct"),
    (Impl, "impl"),
    (This, "self"),
    (True, "true"),
    (False, "false")
);

impl Keyword {
    /// Returns true if the [`Keyword`] is 'significant' (I.e. not a 'modifier' such as `pub`
    /// or `mut`, rather something that can start an `expression` or `item` such as `let`)
    #[inline]
    pub fn is_significant(self) -> bool {
        !matches!(self, Self::Pub | Self::Mut | Self::Native)
    }

    /// Returns true if the [`Keyword`] is a valid keyword that can start an `Item` expression,
    /// I.e. Function, Struct, etc..
    #[inline]
    pub fn can_start_item(self) -> bool {
        matches!(
            self,
            Self::Mod | Self::Use | Self::Fn | Self::Struct | Self::Const | Self::Impl | Self::Enum
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum LiteralKind {
    Float(f64),
    Integer(i64),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum TokenKind {
    Comment(String),
    BlockComment(String),
    Documentation(String),

    Ident(String),
    Keyword(Keyword),
    StringLiteral(String),
    Literal(LiteralKind),
    Punctuation(Punctuation),

    InvalidIdent(String),
    InvalidLiteral(String),
    InvalidDocumentation(String),

    Unknown,

    Eof,
}

impl From<Keyword> for TokenKind {
    fn from(value: Keyword) -> Self {
        Self::Keyword(value)
    }
}

impl From<&Keyword> for TokenKind {
    fn from(value: &Keyword) -> Self {
        Self::Keyword(*value)
    }
}

impl From<Punctuation> for TokenKind {
    fn from(value: Punctuation) -> Self {
        Self::Punctuation(value)
    }
}

impl From<&Punctuation> for TokenKind {
    fn from(value: &Punctuation) -> Self {
        Self::Punctuation(*value)
    }
}

impl From<LiteralKind> for TokenKind {
    fn from(value: LiteralKind) -> Self {
        Self::Literal(value)
    }
}

impl From<&LiteralKind> for TokenKind {
    fn from(value: &LiteralKind) -> Self {
        Self::Literal(*value)
    }
}

// Accessors..
impl TokenKind {
    /// Returns true if the `kind` of token is _not_ an invalid variant.
    #[inline]
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !matches!(
            self,
            Self::InvalidIdent(_) | Self::InvalidLiteral(_) | Self::InvalidDocumentation(_)
        )
    }
}

// Parser convenience..
impl TokenKind {
    /// Returns true if the [`TokenKind`] is a valid token that can start an `Item` expression,
    /// I.e. Function, Struct, etc..
    #[inline]
    pub fn can_start_item(&self) -> bool {
        match self {
            TokenKind::Keyword(kw) => kw.can_start_item(),
            _ => false,
        }
    }

    /// Returns `true`` if the [`TokenKind`] is a valid `identifier`.
    #[inline]
    pub fn is_ident(&self) -> bool {
        matches!(self, TokenKind::Ident(_))
    }

    /// Returns `Some(ident)` if the [`TokenKind`] is a valid `identifier`, `None` otherwise.
    #[inline]
    pub fn get_ident(&self) -> Option<String> {
        match self {
            TokenKind::Ident(i) => Some(i.clone()),
            _ => None,
        }
    }

    /// Returns `true` if the [`TokenKind`] is 'significant', (I.e. not a 'modifier' such as `pub`
    /// or `mut`, rather something that can start an `expression` or `item` such as `let`)
    #[inline]
    pub fn is_significant(&self) -> bool {
        match self {
            TokenKind::Ident(_) => true,
            TokenKind::Keyword(keyword) => keyword.is_significant(),
            TokenKind::Punctuation(punctuation) => !matches!(punctuation, Punctuation::Ampersand),
            TokenKind::StringLiteral(_) | TokenKind::Literal(_) | TokenKind::Eof => true,
            _ => false,
        }
    }
}

/// A lexical token produced by the lexer.
///
/// A `Token` represents a classified span of source text, including its
/// [`TokenKind`] and the byte offsets denoting it's position in the input.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Token {
    pub kind: TokenKind,
    pub start: u32,
    pub end: u32,
}

impl Token {
    #[inline]
    #[must_use]
    pub fn from_kind(kind: TokenKind) -> Self {
        Self::new(kind, 0, 0)
    }

    #[inline]
    #[must_use]
    pub fn new_raw(kind: TokenKind, start: u32, end: u32) -> Self {
        Self { kind, start, end }
    }

    #[inline]
    #[must_use]
    pub fn new(kind: impl Into<TokenKind>, start: u32, end: u32) -> Self {
        Self {
            kind: kind.into(),
            start,
            end,
        }
    }
}

impl ::std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Token{{ {}..{} | {:?} }}",
            self.start, self.end, self.kind
        )
    }
}

impl ::std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}
