use crate::{
    ast::{Ident, Local, LocalKind, Mutability, Parameter, Statement, StatementKind, Ty},
    token::{Keyword, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

#[derive(Debug, Clone)]
pub struct VariablePattern {
    pub ident: Ident,
    pub ty: Ty,
    pub mutable: Mutability,
}

impl From<VariablePattern> for Parameter {
    fn from(value: VariablePattern) -> Self {
        Self::new(value.ident, value.ty, value.mutable)
    }
}

impl VariablePattern {
    #[inline]
    pub fn into_param(self) -> Parameter {
        Parameter::new(self.ident, self.ty, self.mutable)
    }
}

impl Parser<'_, '_> {
    pub fn parse_statement(&mut self) -> PResult<Statement> {
        let start_span = self.cursor.position();
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
                if self.check_punctuation_advance(Punctuation::SemiColon) {
                    StatementKind::Semi(expr)
                } else {
                    StatementKind::Expr(expr)
                }
            }
        };
        Ok(Statement::new(kind, start_span..self.cursor.position()))
    }

    pub fn parse_variable_pattern(&mut self) -> PResult<VariablePattern> {
        let mutable = self.parse_mutability()?;
        let (var_ident, var_type) = if self.check_keyword_advance(Keyword::This) {
            let ident = "self".to_string();
            let ty = if self.check_punctuation_advance(Punctuation::Colon) {
                self.parse_ty()?
            } else {
                Ty::new("Self") // TODO: Make this a Ty.
            };
            (ident, ty)
        } else {
            let ident = self.expect_ident()?;
            self.expect_punctuation(Punctuation::Colon)?;
            let ty = self.parse_ty()?;
            (ident, ty)
        };

        Ok(VariablePattern {
            ident: var_ident.into(),
            ty: var_type,
            mutable,
        })
    }

    pub fn parse_let_local(&mut self) -> PResult<Box<Local>> {
        self.expect_keyword(Keyword::Let)?;
        let mutable = self.parse_mutability()?;
        let var_ident = self.expect_ident()?;
        let var_type = if self.check_punctuation_advance(Punctuation::Colon) {
            self.parse_ty()?
        } else {
            Ty::Infer
        };
        self.expect_punctuation(Punctuation::Eq)?;

        let expr = self.parse_expression()?;

        self.expect_punctuation(Punctuation::SemiColon)?;

        Ok(Local::new_boxed(
            var_ident.into(),
            var_type,
            LocalKind::Initialise(expr),
            mutable,
        ))
    }
}
