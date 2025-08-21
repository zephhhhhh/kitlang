use crate::{
    ast::{
        BinaryOpKind, Block, Expression, ExpressionAssociation, ExpressionKind, ExpressionOrder,
        Ident, Literal, Statement, StatementKind, UnaryOpKind,
    },
    parser::errors::{ParseError, ParseErrorKind},
    token::{Keyword, LiteralKind, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

// Block,               Done
// Literal,             Done
// BinaryOp,            Done (Rework?)
// UnaryOp,             Done
// If,                  Done
// While,               Done
// Assign,              Done
// Call,                Done
// Continue,            Todo
// Return,              Todo

#[derive(Debug)]
enum ExpressionOrderBound {
    Included(ExpressionOrder),
    Excluded(ExpressionOrder),
    Unbounded,
}

impl Parser<'_, '_> {
    fn is_binary_op(&self, offset: u32) -> PResult<Option<BinaryOpKind>> {
        let next_token = self.peek_at(offset)?;

        if let Some(puncts) = self.get_punctuation_sequence::<2>(offset) {
            if !matches!(
                puncts[1],
                Punctuation::Bang | Punctuation::Minus | Punctuation::Star | Punctuation::OpenParen
            ) {
                return Ok(match puncts {
                    [Punctuation::Eq, Punctuation::Eq] => Some(BinaryOpKind::Equal),
                    [Punctuation::GreaterThan, Punctuation::Eq] => {
                        Some(BinaryOpKind::GreaterThanOrEqual)
                    }
                    [Punctuation::LessThan, Punctuation::Eq] => Some(BinaryOpKind::LessThanOrEqual),
                    [Punctuation::Bang, Punctuation::Eq] => Some(BinaryOpKind::NotEqual),
                    [Punctuation::LessThan, Punctuation::LessThan] => Some(BinaryOpKind::ShiftLeft),
                    [Punctuation::GreaterThan, Punctuation::GreaterThan] => {
                        Some(BinaryOpKind::ShiftRight)
                    }
                    [Punctuation::Ampersand, Punctuation::Ampersand] => Some(BinaryOpKind::And),
                    [Punctuation::Or, Punctuation::Or] => Some(BinaryOpKind::Or),
                    _ => None,
                });
            }
        }

        if let TokenKind::Punctuation(p) = next_token.kind {
            Ok(match p {
                Punctuation::LessThan => Some(BinaryOpKind::LessThan),
                Punctuation::GreaterThan => Some(BinaryOpKind::GreaterThan),
                Punctuation::Percent => Some(BinaryOpKind::Mod),
                Punctuation::Plus => Some(BinaryOpKind::Add),
                Punctuation::Minus => Some(BinaryOpKind::Sub),
                Punctuation::Slash => Some(BinaryOpKind::Div),
                Punctuation::Star => Some(BinaryOpKind::Mul),
                Punctuation::Ampersand => Some(BinaryOpKind::BitwiseAND),
                Punctuation::Or => Some(BinaryOpKind::BitwiseOR),
                Punctuation::Caret => Some(BinaryOpKind::BitwiseXOR),
                _ => None,
            })
        } else {
            Ok(None)
        }
    }

    pub fn parse_semi(&mut self) -> PResult<Box<Expression>> {
        let expr = self.parse_expression()?;
        self.expect_punctuation(Punctuation::SemiColon)?;
        Ok(expr)
    }

    fn is_double_eq(&self) -> PResult<bool> {
        let (next, after) = (self.peek_at(0)?, self.peek_at(1)?);
        Ok(next.kind == TokenKind::Punctuation(Punctuation::Eq)
            && after.kind == TokenKind::Punctuation(Punctuation::Eq))
    }

    pub fn parse_expr_atom(&mut self) -> PResult<Box<Expression>> {
        let token = self.peek_at(0)?;
        let atom = match &token.kind {
            TokenKind::Keyword(Keyword::If) => self.parse_if(),
            TokenKind::Keyword(Keyword::While) => self.parse_while(),
            TokenKind::StringLiteral(_)
            | TokenKind::Literal(_)
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False) => self.parse_literal(),
            TokenKind::Ident(_) => self.parse_ident_expr(),
            TokenKind::Punctuation(Punctuation::OpenBrace) => self.parse_block_as_expression(),
            TokenKind::Punctuation(Punctuation::OpenParen) => self.parse_parens_expr(),
            TokenKind::Punctuation(Punctuation::Bang)
            | TokenKind::Punctuation(Punctuation::Minus)
            | TokenKind::Punctuation(Punctuation::Star) => self.parse_unary_op(),
            _ => {
                println!("Kind: {:?}", token.kind);
                todo!()
            }
        }?;
        match self.peek_at(0)?.kind {
            TokenKind::Punctuation(Punctuation::OpenParen) => self.parse_call_continued(atom),
            TokenKind::Punctuation(Punctuation::Dot) => todo!(), // Field access..
            TokenKind::Punctuation(Punctuation::OpenBracket) => todo!(), // Indexing..
            ref c if !self.is_double_eq()? && c == &TokenKind::Punctuation(Punctuation::Eq) => {
                self.parse_assign_continued(atom)
            }
            _ => Ok(atom),
        }
    }

    fn parse_parens_expr(&mut self) -> PResult<Box<Expression>> {
        self.expect_punctuation(Punctuation::OpenParen)?;
        let expr = self.parse_expression()?;
        self.expect_punctuation(Punctuation::CloseParen)?;
        Ok(expr)
    }

    pub fn parse_expression(&mut self) -> PResult<Box<Expression>> {
        let expr = self.parse_expr_atom()?;

        if self.is_binary_op(0)?.is_some() {
            self.parse_binary_op_continued(expr)
        } else {
            Ok(expr)
        }
    }

    pub fn parse_block_as_expression(&mut self) -> PResult<Box<Expression>> {
        Ok(Expression::new_boxed(ExpressionKind::Block(
            self.parse_block_expression()?,
        )))
    }

    pub fn parse_block_expression(&mut self) -> PResult<Box<Block>> {
        self.expect_punctuation(Punctuation::OpenBrace)?;

        let mut statements: Vec<Statement> = Vec::new();

        while !self.cursor.is_end() {
            if self.check_punctuation_advance(Punctuation::CloseBrace) {
                break;
            }

            let new_statement = self.parse_statement()?;
            if let Some(last) = statements.last() {
                if let StatementKind::Expr(e) = &last.kind {
                    // If the last statement was an expression (no trailing semi-colon) and the
                    // expression was not an expression which can omit the semi-colon (such as if,
                    // while, etc..), we throw an error..
                    if !e.kind.can_be_non_semi() {
                        return Err(ParseError::new(
                            ParseErrorKind::ExpectedSemiColon,
                            last.source_span.clone(),
                        ));
                    }
                }
            }
            statements.push(new_statement);
        }

        Ok(Box::new(Block::new(statements)))
    }

    pub fn parse_ident_expr(&mut self) -> PResult<Box<Expression>> {
        let ident = self.expect_ident()?;
        Ok(Expression::new_boxed(ExpressionKind::Ident(ident.into())))
    }

    pub fn parse_literal(&mut self) -> PResult<Box<Expression>> {
        let token = self.peek_at(0)?;
        let kind = match &token.kind {
            TokenKind::Keyword(Keyword::True) => {
                self.cursor.advance();
                Literal::Boolean(true)
            }
            TokenKind::Keyword(Keyword::False) => {
                self.cursor.advance();
                Literal::Boolean(false)
            }
            TokenKind::StringLiteral(s) => {
                let kind = Literal::String(s.clone());
                self.cursor.advance();
                kind
            }
            TokenKind::Literal(l) => {
                let kind = match l {
                    LiteralKind::Float(f) => Literal::Float(*f),
                    LiteralKind::Integer(i) => Literal::Integer(*i),
                };
                self.cursor.advance();
                kind
            }
            _ => todo!(),
        };

        Ok(Expression::new_boxed(ExpressionKind::Literal(kind)))
    }

    pub fn parse_assign_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_punctuation(Punctuation::Eq)?;

        let value_expr = self.parse_expression()?;

        Ok(Expression::new_boxed(ExpressionKind::Assign(
            lhs, value_expr,
        )))
    }

    pub fn parse_unary_op(&mut self) -> PResult<Box<Expression>> {
        let token = self.peek_at(0)?;
        let TokenKind::Punctuation(punct) = token.kind else {
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedToken(token.kind.clone()),
                token,
            ));
        };
        let unary = match punct {
            Punctuation::Minus => UnaryOpKind::Negate,
            Punctuation::Bang => UnaryOpKind::Not,
            Punctuation::Star => UnaryOpKind::Dereference,
            _ => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken(token.kind.clone()),
                    token,
                ));
            }
        };
        self.cursor.advance();

        let target_expr = self.parse_expr_atom()?;

        Ok(Expression::new_boxed(ExpressionKind::Unary(
            unary,
            target_expr,
        )))
    }

    fn parse_binary_op_continued_ordered(
        &mut self,
        lhs: Box<Expression>,
        min_order: ExpressionOrderBound,
    ) -> PResult<Box<Expression>> {
        let Some(binary_op) = self.is_binary_op(0)? else {
            return Err(ParseError::new(
                ParseErrorKind::InvalidBinaryOperator,
                self.cursor.pos_span(),
            ));
        };
        let binary_op_order = binary_op.get_order();

        if match min_order {
            ExpressionOrderBound::Included(expression_order) => binary_op_order < expression_order,
            ExpressionOrderBound::Excluded(expression_order) => binary_op_order <= expression_order,
            ExpressionOrderBound::Unbounded => false,
        } {
            return Ok(lhs);
        }

        self.cursor.advance_by(binary_op.token_count());

        let mut rhs = self.parse_expr_atom()?;

        if let Some(next_bin_op) = self.is_binary_op(0)? {
            if next_bin_op.get_order() > binary_op_order {
                let new_min = if binary_op.get_association() == ExpressionAssociation::Right {
                    ExpressionOrderBound::Excluded(binary_op_order)
                } else {
                    ExpressionOrderBound::Included(binary_op_order)
                };

                rhs = self.parse_binary_op_continued_ordered(rhs, new_min)?;
            } else {
                let result = Expression::new_boxed(ExpressionKind::BinaryOp(binary_op, lhs, rhs));

                return self
                    .parse_binary_op_continued_ordered(result, ExpressionOrderBound::Unbounded);
            }
        }

        Ok(Expression::new_boxed(ExpressionKind::BinaryOp(
            binary_op, lhs, rhs,
        )))
    }

    pub fn parse_binary_op_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.parse_binary_op_continued_ordered(lhs, ExpressionOrderBound::Unbounded)
    }

    pub fn parse_call_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_punctuation(Punctuation::OpenParen)?;

        let mut args = Vec::new();

        if !self.check_punctuation_advance(Punctuation::CloseParen) {
            // There are arguments.. Parse the first one..
            args.push(self.parse_expression()?);

            // Parse a comma before each additional argument..
            while !self.cursor.is_end() {
                if self.check_punctuation_advance(Punctuation::CloseParen) {
                    break;
                }

                self.expect_punctuation(Punctuation::Comma)?;

                let arg = self.parse_expression()?;
                args.push(arg);
            }
        }

        Ok(Expression::new_boxed(ExpressionKind::Call(lhs, args)))
    }

    pub fn parse_if(&mut self) -> PResult<Box<Expression>> {
        self.expect_keyword(Keyword::If)?;
        let condition = self.parse_expression()?;
        let body = self.parse_block_expression()?;
        let else_body = if self.check_keyword_advance(Keyword::Else) {
            if self.check_keyword(Keyword::If) {
                Some(self.parse_if()?)
            } else {
                Some(self.parse_block_as_expression()?)
            }
        } else {
            None
        };

        Ok(Expression::new_boxed(ExpressionKind::If(
            condition, body, else_body,
        )))
    }

    pub fn parse_while(&mut self) -> PResult<Box<Expression>> {
        self.expect_keyword(Keyword::While)?;
        let condition = self.parse_expression()?;
        let body = self.parse_block_expression()?;

        Ok(Expression::new_boxed(ExpressionKind::While(
            condition, body,
        )))
    }
}
