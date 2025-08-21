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
        let mutable = self.check_keyword_advance(Keyword::Mut);
        let var_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::Colon)?;
        let type_ident = self.expect_ident()?;

        Ok(VariablePattern {
            ident: Ident(var_ident),
            ty: Ty(type_ident),
            mutable: Mutability::from_is_mutable(mutable),
        })
    }

    pub fn parse_let_local(&mut self) -> PResult<Box<Local>> {
        self.expect_keyword(Keyword::Let)?;
        let mutable = self.check_keyword_advance(Keyword::Mut);
        let var_ident = self.expect_ident()?;
        let type_ident = if self.check_punctuation_advance(Punctuation::Colon) {
            self.expect_ident()?
        } else {
            String::new()
        };
        self.expect_punctuation(Punctuation::Eq)?;

        let expr = self.parse_expression()?;

        self.expect_punctuation(Punctuation::SemiColon)?;

        Ok(Box::new(Local::new(
            Ident(var_ident),
            Ty(type_ident),
            LocalKind::Initialise(expr),
            Mutability::from_is_mutable(mutable),
        )))
    }
}
