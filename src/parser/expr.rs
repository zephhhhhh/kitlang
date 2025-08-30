use crate::{
    ast::{
        BinaryOpKind, Block, Expression, ExpressionAssociation, ExpressionKind, ExpressionOrder,
        FieldInitialisation, IdentPath, Literal, MethodCall, Statement, StatementKind,
        StructInitialisation, UnaryOpKind,
    },
    parser::errors::{ParseError, ParseErrorKind},
    token::{Keyword, LiteralKind, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

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

    fn is_double_eq(&self) -> bool {
        self.check_kind_at(0, Punctuation::Eq) && self.check_kind_at(1, Punctuation::Eq)
    }

    fn process_atom(&mut self, mut atom: Box<Expression>) -> PResult<Box<Expression>> {
        while !self.cursor.is_end() {
            atom = match self.peek_at(0)?.kind {
                TokenKind::Punctuation(Punctuation::OpenParen) => self.parse_call_continued(atom),
                TokenKind::Punctuation(Punctuation::OpenBracket) => self.parse_array_index(atom),
                TokenKind::Punctuation(Punctuation::Dot) => match self.peek_at(2)?.kind {
                    TokenKind::Punctuation(Punctuation::OpenParen) => self.parse_member_call(atom),
                    _ => self.parse_member_access(atom),
                },
                ref c if !self.is_double_eq() && c == &TokenKind::Punctuation(Punctuation::Eq) => {
                    self.parse_assign_continued(atom)
                }
                _ => return Ok(atom),
            }?;
        }
        Ok(atom)
    }

    pub fn parse_expr_atom(&mut self) -> PResult<Box<Expression>> {
        let token = self.peek_at(0)?;
        let atom = match &token.kind {
            TokenKind::StringLiteral(_) | TokenKind::Literal(_) => self.parse_literal(),
            TokenKind::Keyword(kw) => match kw {
                Keyword::True | Keyword::False => self.parse_literal(),
                Keyword::If => self.parse_if(),
                Keyword::While => self.parse_while(),
                Keyword::Continue => self.parse_continue(),
                Keyword::Return => self.parse_return(),
                Keyword::This => self.parse_self(),
                _ => {
                    println!("Not implemented: {:?}", kw);
                    todo!()
                }
            },
            TokenKind::Ident(_) | TokenKind::Punctuation(Punctuation::Colon) => {
                let path = self.parse_path()?;
                if self.check_kind(Punctuation::OpenBrace) {
                    self.parse_struct_initialiser(path)
                } else {
                    Ok(Expression::new_boxed(ExpressionKind::IdentPath(path)))
                }
            }
            TokenKind::Punctuation(Punctuation::OpenBrace) => self.parse_block_as_expression(),
            TokenKind::Punctuation(Punctuation::OpenParen) => self.parse_parens_expr(),
            TokenKind::Punctuation(Punctuation::Bang)
            | TokenKind::Punctuation(Punctuation::Minus)
            | TokenKind::Punctuation(Punctuation::Star) => self.parse_unary_op(),
            _ => Err(ParseError::new(
                ParseErrorKind::InvalidExpressionAtom(token.kind.clone()),
                token,
            )),
        }?;
        self.process_atom(atom)
    }

    fn parse_parens_expr(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenParen)?;
        let expr = self.parse_expression()?;
        self.expect_kind(Punctuation::CloseParen)?;
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
        self.expect_kind(Punctuation::OpenBrace)?;

        let mut statements: Vec<Statement> = Vec::new();

        while !self.cursor.is_end() {
            if self.check_kind_advance(Punctuation::CloseBrace) {
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

    fn parse_literal(&mut self) -> PResult<Box<Expression>> {
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

    fn parse_assign_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Eq)?;

        let value_expr = self.parse_expression()?;

        Ok(Expression::new_boxed(ExpressionKind::Assign(
            lhs, value_expr,
        )))
    }

    fn parse_unary_op(&mut self) -> PResult<Box<Expression>> {
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
                self.cursor.current_span(),
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

    fn parse_binary_op_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.parse_binary_op_continued_ordered(lhs, ExpressionOrderBound::Unbounded)
    }

    fn parse_call_continued(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenParen)?;

        let args = self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
            s.parse_expression()
        })?;

        Ok(Expression::new_boxed(ExpressionKind::Call(lhs, args)))
    }

    fn parse_if(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Keyword::If)?;
        let condition = self.parse_expression()?;
        let body = self.parse_block_expression()?;
        let else_body = if self.check_kind_advance(Keyword::Else) {
            if self.check_kind(Keyword::If) {
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

    fn parse_while(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Keyword::While)?;
        let condition = self.parse_expression()?;
        let body = self.parse_block_expression()?;

        Ok(Expression::new_boxed(ExpressionKind::While(
            condition, body,
        )))
    }

    fn parse_continue(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Keyword::Continue)?;
        Ok(Expression::new_boxed(ExpressionKind::Continue))
    }

    fn parse_return(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Keyword::Return)?;

        // After a return the only valid values are either: ';', '}' or an `expression`.
        if self.check_kind(Punctuation::SemiColon) || self.check_kind(Punctuation::CloseBrace) {
            Ok(Expression::new_boxed(ExpressionKind::Return(None)))
        } else {
            let return_expr = self.parse_expression()?;
            Ok(Expression::new_boxed(ExpressionKind::Return(Some(
                return_expr,
            ))))
        }
    }

    fn parse_array_index(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenBracket)?;
        let index_expr = self.parse_expression()?;
        self.expect_kind(Punctuation::CloseBracket)?;

        Ok(Expression::new_boxed(ExpressionKind::Index(
            lhs, index_expr,
        )))
    }

    fn parse_member_access(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Dot)?;
        let member_name = self.expect_ident()?;

        Ok(Expression::new_boxed(ExpressionKind::FieldAccess(
            lhs,
            member_name.into(),
        )))
    }

    fn parse_member_call(&mut self, lhs: Box<Expression>) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Dot)?;
        let method_ident = self.expect_ident()?;
        self.expect_kind(Punctuation::OpenParen)?;

        let args = self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
            s.parse_expression()
        })?;

        Ok(Expression::new_boxed(ExpressionKind::MethodCall(
            MethodCall::new_boxed(lhs, method_ident.into(), args),
        )))
    }

    fn parse_self(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Keyword::This)?;

        Ok(Expression::new_boxed(ExpressionKind::IdentPath(
            IdentPath::new("self"),
        )))
    }

    fn parse_field_initialisation(&mut self) -> PResult<FieldInitialisation> {
        let ident = self.expect_ident()?;
        self.expect_kind(Punctuation::Colon)?;
        let value = self.parse_expression()?;

        Ok(FieldInitialisation::new(ident.into(), value))
    }

    fn parse_struct_initialiser(&mut self, lhs: IdentPath) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenBrace)?;

        let fields = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_field_initialisation()
        })?;

        Ok(Expression::new_boxed(ExpressionKind::StructInit(
            StructInitialisation::new_boxed(lhs, fields),
        )))
    }
}
