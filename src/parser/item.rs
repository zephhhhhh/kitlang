use crate::{
    ast::{
        Function, FunctionReturnTy, FunctionSig, Ident, Item, ItemKind, Parameter, Ty, Visibility,
    },
    parser::errors::{ParseError, ParseErrorKind},
    token::{Keyword, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

impl Parser<'_, '_> {
    pub fn parse_item(&mut self) -> PResult<Item> {
        let (_, token) = self.expect_next_significant_token()?;
        match token.kind {
            TokenKind::Keyword(Keyword::Fn) => self.parse_function(),
            TokenKind::Keyword(Keyword::Struct) => self.parse_struct(),
            TokenKind::Keyword(Keyword::Impl) => self.parse_implementation(),
            _ => Err(ParseError::new(
                ParseErrorKind::WrongTokenKind(token.kind.clone()),
                token,
            )),
        }
    }

    pub fn parse_function(&mut self) -> PResult<Item> {
        let public = self.check_keyword_advance(Keyword::Pub);
        self.expect_keyword(Keyword::Fn)?;
        let func_name_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::OpenParen)?;

        let mut function_arguments = Vec::new();

        if !self.check_punctuation_advance(Punctuation::CloseParen) {
            // There are arguments.. Parse the first one..
            function_arguments.push(self.parse_variable_pattern()?.into());

            // Parse a comma before each additional argument..
            while !self.cursor.is_end() {
                if self.check_punctuation_advance(Punctuation::CloseParen) {
                    break;
                }

                self.expect_punctuation(Punctuation::Comma)?;

                function_arguments.push(self.parse_variable_pattern()?.into());
            }
        }

        let return_type_token = self.peek_at(0)?;
        let func_return_type = match return_type_token.kind {
            TokenKind::Punctuation(Punctuation::OpenBrace) => FunctionReturnTy::Default,
            TokenKind::Punctuation(Punctuation::Minus) => {
                self.cursor.advance();
                self.expect_punctuation(Punctuation::GreaterThan)?;
                FunctionReturnTy::Ty(Box::new(Ty(self.expect_ident()?)))
            }
            _ => {
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken(return_type_token.kind.clone()),
                    return_type_token,
                ));
            }
        };

        let function_body = self.parse_block_expression()?;

        let function_sig = FunctionSig {
            parameters: function_arguments,
            output: func_return_type,
        };

        let item_kind = ItemKind::Fn(Box::new(Function::new(
            Ident(func_name_ident),
            function_sig,
            Some(function_body),
        )));

        Ok(Item::new(item_kind, Visibility::from_is_public(public)))
    }

    pub fn parse_struct(&mut self) -> PResult<Item> {
        let public = self.check_keyword_advance(Keyword::Pub);
        self.expect_keyword(Keyword::Struct)?;

        let struct_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::OpenBrace)?;

        // TODO: Parse struct body..

        todo!()
    }

    pub fn parse_implementation(&mut self) -> PResult<Item> {
        self.expect_keyword(Keyword::Impl)?;

        // TODO: Parse block expression (only consts, functions allowed)..

        todo!()
    }
}
