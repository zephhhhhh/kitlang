use crate::ast::{
    BindingPattern, Local, LocalKind, Parameter, SourceSpan, SpannedIdent, Statement,
    StatementKind, Ty,
};

use crate::parser::Parser;
use crate::parser::errors::{PResult, ParseError, ParseErrorKind};

use crate::token::{Keyword, Punctuation, TokenKind};

#[cfg(doc)]
use crate::parser::TokenStream;

/// Output from parsing a variable pattern from the [`TokenStream`].
/// Describes the declaration of a variable, as a parameter or similar.
#[derive(Debug, Clone)]
pub struct VariablePattern {
    /// Variable name.
    pub pattern: BindingPattern,
    /// Variable type.
    pub ty: Ty,
    /// The span of bytes in the source string the declaration occupies.
    pub span: SourceSpan,
}

impl From<VariablePattern> for Parameter {
    fn from(value: VariablePattern) -> Self {
        Self::new(value.pattern, value.ty, value.span)
    }
}

impl VariablePattern {
    #[inline]
    pub fn into_param(self) -> Parameter {
        Parameter::new(self.pattern, self.ty, self.span)
    }
}

impl Parser<'_, '_> {
    pub fn parse_statement(&mut self) -> PResult<Statement> {
        let start_span = self.begin_span();

        let (_, token) = self.expect_next_significant_token()?;
        let kind = match &token.kind {
            c if c.can_start_item() => StatementKind::Item(Box::new(self.parse_item()?)),
            TokenKind::Punctuation(Punctuation::SemiColon) => {
                self.cursor.consume();
                StatementKind::Empty
            }
            TokenKind::Keyword(Keyword::Let) => StatementKind::Let(self.parse_let_local()?),
            // Anything else we attempt to parse as an expression, and if this fails, parsing the
            // statement fails.
            _ => {
                let expr = self.parse_expression()?;
                if self.check_kind_advance(Punctuation::SemiColon) {
                    StatementKind::Semi(expr)
                } else {
                    StatementKind::Expr(expr)
                }
            }
        };

        Ok(Statement::new(kind, self.finish_span(start_span)))
    }

    pub fn parse_variable_pattern(&mut self) -> PResult<VariablePattern> {
        let start_span = self.begin_span();

        let mutable = self.parse_mutability()?;
        let ref_type = self.parse_ref_and_refmut()?;

        let var_ident_span = self.begin_span();
        let (var_pat, var_type) = if self.check_kind_advance(Keyword::This) {
            let ident_span = self.finish_span(var_ident_span);
            let ty = if self.check_kind_advance(Punctuation::Colon) {
                if ref_type.is_ref() {
                    let token = self.peek()?;
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedToken(token.kind.clone()),
                        token,
                    ));
                }

                self.parse_ty()?
            } else if ref_type.is_ref() {
                Ty::Ref(
                    Box::new(Ty::This(ident_span)),
                    ref_type.mutability(),
                    self.finish_span(start_span),
                )
            } else {
                Ty::This(ident_span)
            };

            let self_spanned = SpannedIdent::new("self", ident_span);
            (BindingPattern::Variable(self_spanned, mutable), ty)
        } else {
            if ref_type.is_ref() {
                let token = self.peek()?;
                return Err(ParseError::new(
                    ParseErrorKind::ExpectedToken(
                        TokenKind::Keyword(Keyword::This),
                        token.kind.clone(),
                    ),
                    token,
                ));
            }

            let pat = self.parse_binding()?;
            self.expect_kind(Punctuation::Colon)?;
            let ty = self.parse_ty()?;

            (pat, ty)
        };

        Ok(VariablePattern {
            pattern: var_pat,
            ty: var_type,
            span: self.finish_span(start_span),
        })
    }

    pub fn parse_binding(&mut self) -> PResult<BindingPattern> {
        let start_span = self.begin_span();

        if self.check_kind_advance(Punctuation::OpenParen) {
            // Tuple destructuring..
            let mut patterns = Vec::new();
            loop {
                if self.check_kind_advance(Punctuation::CloseParen) {
                    break;
                }

                let pattern = self.parse_binding()?;
                patterns.push(pattern);

                if self.check_kind_advance(Punctuation::Comma) {
                    continue;
                }

                self.expect_kind(Punctuation::CloseParen)?;
                break;
            }

            Ok(BindingPattern::Tuple(
                patterns,
                self.finish_span(start_span),
            ))
        } else {
            let mutable = self.parse_mutability()?;
            let ident = self.expect_ident_spanned()?;

            Ok(BindingPattern::Variable(ident, mutable))
        }
    }

    pub fn parse_let_local(&mut self) -> PResult<Box<Local>> {
        self.expect_kind(Keyword::Let)?;
        let pattern = self.parse_binding()?;
        let var_type = if self.check_kind_advance(Punctuation::Colon) {
            self.parse_ty()?
        } else {
            Ty::Infer
        };
        self.expect_kind(Punctuation::Eq)?;

        let expr = self.parse_expression()?;

        self.expect_kind(Punctuation::SemiColon)?;

        Ok(Local::new_boxed(
            pattern,
            var_type,
            LocalKind::Initialise(expr),
        ))
    }
}
