use crate::ast::{
    BinaryOpKind, Block, Expression, ExpressionAssociation, ExpressionKind, ExpressionOrder,
    FieldInitialisation, IdentPath, Literal, MethodCall, SpannedIdentPath, Statement,
    StatementKind, StructInitialisation, UnaryOpKind,
};

use crate::parser::Parser;
use crate::parser::errors::{PResult, ParseError, ParseErrorKind};

use crate::token::{Keyword, LiteralKind, Punctuation, TokenKind};

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

    fn process_atom(
        &mut self,
        mut atom: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        while !self.cursor.is_end() {
            atom = match self.peek_at(0)?.kind {
                TokenKind::Punctuation(Punctuation::OpenParen) => {
                    self.parse_call_continued(atom, span_start)
                }
                TokenKind::Punctuation(Punctuation::OpenBracket) => {
                    self.parse_array_index(atom, span_start)
                }
                TokenKind::Punctuation(Punctuation::Dot) => match self.peek_at(2)?.kind {
                    TokenKind::Punctuation(Punctuation::OpenParen) => {
                        self.parse_member_call(atom, span_start)
                    }
                    _ => self.parse_member_access(atom, span_start),
                },
                ref c if !self.is_double_eq() && c == &TokenKind::Punctuation(Punctuation::Eq) => {
                    self.parse_assign_continued(atom, span_start)
                }
                _ => return Ok(atom),
            }?;
        }
        Ok(atom)
    }

    pub fn parse_expr_atom(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

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
                Keyword::Break => self.parse_break(),
                _ => {
                    println!("Not implemented: {:?}", kw);
                    todo!()
                }
            },
            TokenKind::Ident(_) | TokenKind::Punctuation(Punctuation::Colon) => {
                let span_start = self.begin_span();
                let path = self.parse_spanned_path()?;
                // FIXME: REALLY HACKY
                if self.check_kind(Punctuation::OpenBrace)
                    && self.check_ident_at(1)
                    && self.check_kind_at(2, Punctuation::Colon)
                {
                    self.parse_struct_initialiser(path)
                } else {
                    Ok(Expression::new_boxed(
                        ExpressionKind::IdentPath(path),
                        self.finish_span(span_start),
                    ))
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
        self.process_atom(atom, span_start)
    }

    fn parse_parens_expr(&mut self) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenParen)?;
        let expr = self.parse_expression()?;
        self.expect_kind(Punctuation::CloseParen)?;
        Ok(expr)
    }

    pub fn parse_expression(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        let expr = self.parse_expr_atom()?;

        if self.is_binary_op(0)?.is_some() {
            self.parse_binary_op_continued(expr, span_start)
        } else {
            Ok(expr)
        }
    }

    pub fn parse_block_as_expression(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();
        Ok(Expression::new_boxed(
            ExpressionKind::Block(self.parse_block_expression()?),
            self.finish_span(span_start),
        ))
    }

    pub fn parse_block_expression(&mut self) -> PResult<Box<Block>> {
        let span_start = self.begin_span();
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
                            last.span,
                        ));
                    }
                }
            }
            statements.push(new_statement);
        }

        Ok(Box::new(Block::new(
            statements,
            self.finish_span(span_start),
        )))
    }

    fn parse_literal(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();
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

        Ok(Expression::new_boxed(
            ExpressionKind::Literal(kind),
            self.finish_span(span_start),
        ))
    }

    fn parse_assign_continued(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Eq)?;

        let value_expr = self.parse_expression()?;

        Ok(Expression::new_boxed(
            ExpressionKind::Assign(lhs, value_expr),
            self.finish_span(span_start),
        ))
    }

    fn parse_unary_op(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

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

        Ok(Expression::new_boxed(
            ExpressionKind::Unary(unary, target_expr),
            self.finish_span(span_start),
        ))
    }

    fn parse_binary_op_continued_ordered(
        &mut self,
        lhs: Box<Expression>,
        min_order: ExpressionOrderBound,
        span_start: u32,
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

                rhs = self.parse_binary_op_continued_ordered(rhs, new_min, span_start)?;
            } else {
                let result = Expression::new_boxed(
                    ExpressionKind::BinaryOp(binary_op, lhs, rhs),
                    self.finish_span(span_start),
                );

                return self.parse_binary_op_continued_ordered(
                    result,
                    ExpressionOrderBound::Unbounded,
                    span_start,
                );
            }
        }

        Ok(Expression::new_boxed(
            ExpressionKind::BinaryOp(binary_op, lhs, rhs),
            self.finish_span(span_start),
        ))
    }

    fn parse_binary_op_continued(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.parse_binary_op_continued_ordered(lhs, ExpressionOrderBound::Unbounded, span_start)
    }

    fn parse_call_continued(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenParen)?;

        let args = self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
            s.parse_expression()
        })?;

        Ok(Expression::new_boxed(
            ExpressionKind::Call(lhs, args),
            self.finish_span(span_start),
        ))
    }

    fn parse_if(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

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

        Ok(Expression::new_boxed(
            ExpressionKind::If(condition, body, else_body),
            self.finish_span(span_start),
        ))
    }

    fn parse_while(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        self.expect_kind(Keyword::While)?;
        let condition = self.parse_expression()?;
        let body = self.parse_block_expression()?;

        Ok(Expression::new_boxed(
            ExpressionKind::While(condition, body),
            self.finish_span(span_start),
        ))
    }

    fn parse_break(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        self.expect_kind(Keyword::Break)?;
        Ok(Expression::new_boxed(
            ExpressionKind::Break,
            self.finish_span(span_start),
        ))
    }

    fn parse_continue(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        self.expect_kind(Keyword::Continue)?;
        Ok(Expression::new_boxed(
            ExpressionKind::Continue,
            self.finish_span(span_start),
        ))
    }

    fn parse_return(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        self.expect_kind(Keyword::Return)?;

        // After a return the only valid values are either: ';', '}' or an `expression`.
        if self.check_kind(Punctuation::SemiColon) || self.check_kind(Punctuation::CloseBrace) {
            Ok(Expression::new_boxed(
                ExpressionKind::Return(None),
                self.finish_span(span_start),
            ))
        } else {
            let return_expr = self.parse_expression()?;
            Ok(Expression::new_boxed(
                ExpressionKind::Return(Some(return_expr)),
                self.finish_span(span_start),
            ))
        }
    }

    fn parse_array_index(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::OpenBracket)?;
        let index_expr = self.parse_expression()?;
        self.expect_kind(Punctuation::CloseBracket)?;

        Ok(Expression::new_boxed(
            ExpressionKind::Index(lhs, index_expr),
            self.finish_span(span_start),
        ))
    }

    fn parse_member_access(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Dot)?;
        let member_name = self.expect_ident()?;

        Ok(Expression::new_boxed(
            ExpressionKind::FieldAccess(lhs, member_name.into()),
            self.finish_span(span_start),
        ))
    }

    fn parse_member_call(
        &mut self,
        lhs: Box<Expression>,
        span_start: u32,
    ) -> PResult<Box<Expression>> {
        self.expect_kind(Punctuation::Dot)?;
        let method_ident = self.expect_ident_spanned()?;
        self.expect_kind(Punctuation::OpenParen)?;

        let args = self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
            s.parse_expression()
        })?;

        Ok(Expression::new_boxed(
            ExpressionKind::MethodCall(MethodCall::new_boxed(lhs, method_ident, args)),
            self.finish_span(span_start),
        ))
    }

    fn parse_self(&mut self) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();
        self.expect_kind(Keyword::This)?;
        let span = self.finish_span(span_start);

        let ident_path = IdentPath::new("self");

        Ok(Expression::new_boxed(
            ExpressionKind::IdentPath(SpannedIdentPath::new(ident_path, span)),
            span,
        ))
    }

    fn parse_field_initialisation(&mut self) -> PResult<FieldInitialisation> {
        let span_start = self.begin_span();

        let ident = self.expect_ident()?;
        self.expect_kind(Punctuation::Colon)?;
        let value = self.parse_expression()?;

        Ok(FieldInitialisation::new(
            ident.into(),
            value,
            self.finish_span(span_start),
        ))
    }

    fn parse_struct_initialiser(&mut self, lhs: SpannedIdentPath) -> PResult<Box<Expression>> {
        let span_start = self.begin_span();

        self.expect_kind(Punctuation::OpenBrace)?;

        let fields = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_field_initialisation()
        })?;

        Ok(Expression::new_boxed(
            ExpressionKind::StructInit(StructInitialisation::new_boxed(lhs, fields)),
            self.finish_span(span_start),
        ))
    }
}
