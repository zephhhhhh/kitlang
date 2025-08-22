use crate::{
    ast::{
        Constant, Enum, EnumVariant, Function, FunctionReturnTy, FunctionSig, Ident, Item,
        ItemKind, Module, ModuleKind, Struct, StructField, Ty, Visibility,
    },
    parser::errors::{ParseError, ParseErrorKind},
    token::{Keyword, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

impl Parser<'_, '_> {
    pub fn parse_item(&mut self) -> PResult<Item> {
        let (_, token) = self.expect_next_significant_token()?;
        match token.kind {
            TokenKind::Keyword(Keyword::Const) => self.parse_const(),
            TokenKind::Keyword(Keyword::Fn) => self.parse_function(),
            TokenKind::Keyword(Keyword::Struct) => self.parse_struct(),
            TokenKind::Keyword(Keyword::Impl) => self.parse_implementation(),
            TokenKind::Keyword(Keyword::Enum) => self.parse_enum(),
            TokenKind::Keyword(Keyword::Mod) => self.parse_mod(),
            _ => Err(ParseError::new(
                ParseErrorKind::WrongTokenKind(token.kind.clone()),
                token,
            )),
        }
    }

    pub fn parse_function(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
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

        Ok(Item::new(item_kind, public))
    }

    fn parse_struct_field_definition(&mut self) -> PResult<StructField> {
        let public = self.parse_visibility()?;
        let var_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::Colon)?;
        let type_ident = self.expect_ident()?;

        Ok(StructField::new(
            var_ident.into(),
            type_ident.into(),
            public,
        ))
    }

    pub fn parse_struct(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_keyword(Keyword::Struct)?;

        let struct_ident = self.expect_ident()?;

        if self.check_punctuation_advance(Punctuation::SemiColon) {
            // Unit struct
            return Ok(Item::new(
                ItemKind::Struct(Struct::new_boxed(struct_ident.into(), vec![])),
                public,
            ));
        }

        self.expect_punctuation(Punctuation::OpenBrace)?;

        let mut fields = Vec::new();

        if !self.check_punctuation_advance(Punctuation::CloseBrace) {
            fields.push(self.parse_struct_field_definition()?);

            // Parse a comma before each additional argument..
            while !self.cursor.is_end() {
                if self.check_punctuation_advance(Punctuation::CloseBrace) {
                    break;
                }
                // Comma on last element, but then end structure..
                if self.check_punctuation(Punctuation::Comma)
                    && self.peek_at(1)?.kind == TokenKind::Punctuation(Punctuation::CloseBrace)
                {
                    self.cursor.advance_by(2);
                    break;
                }

                self.expect_punctuation(Punctuation::Comma)?;

                fields.push(self.parse_struct_field_definition()?);
            }
        }

        Ok(Item::new(
            ItemKind::Struct(Struct::new_boxed(struct_ident.into(), fields)),
            public,
        ))
    }

    fn parse_enum_variant(&mut self) -> PResult<EnumVariant> {
        todo!()
    }

    pub fn parse_enum(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_keyword(Keyword::Enum)?;

        let enum_ident = self.expect_ident()?;

        if self.check_punctuation_advance(Punctuation::SemiColon) {
            // Enum with 0 variants..
            return Ok(Item::new(
                ItemKind::Enum(Enum::new_boxed(enum_ident.into(), vec![])),
                public,
            ));
        }

        self.expect_punctuation(Punctuation::OpenBrace)?;

        let mut variants = Vec::new();

        if !self.check_punctuation_advance(Punctuation::CloseBrace) {
            variants.push(self.parse_enum_variant()?);

            // Parse a comma before each additional argument..
            while !self.cursor.is_end() {
                if self.check_punctuation_advance(Punctuation::CloseBrace) {
                    break;
                }
                // Comma on last element, but then end structure..
                if self.check_punctuation(Punctuation::Comma)
                    && self.peek_at(1)?.kind == TokenKind::Punctuation(Punctuation::CloseBrace)
                {
                    self.cursor.advance_by(2);
                    break;
                }

                self.expect_punctuation(Punctuation::Comma)?;

                variants.push(self.parse_enum_variant()?);
            }
        }

        Ok(Item::new(
            ItemKind::Enum(Enum::new_boxed(enum_ident.into(), variants)),
            public,
        ))
    }

    pub fn parse_const(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_keyword(Keyword::Const)?;
        let var_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::Colon)?;
        let ty_ident = self.expect_ident()?;
        self.expect_punctuation(Punctuation::Eq)?;
        let expr = self.parse_expression()?;
        self.expect_punctuation(Punctuation::SemiColon)?;

        Ok(Item::new(
            ItemKind::Const(Constant::new_boxed(var_ident.into(), ty_ident.into(), expr)),
            public,
        ))
    }

    fn parse_impl_item(&mut self) -> PResult<Item> {
        // Only allowed to parse functions and consts..
        todo!()
    }

    pub fn parse_implementation(&mut self) -> PResult<Item> {
        self.expect_keyword(Keyword::Impl)?;
        let ty = self.expect_ident()?;

        // TODO: Parse block expression (only consts, functions allowed)..

        todo!()
    }

    fn parse_mod_item(&mut self) -> PResult<Item> {
        let Some((_offset, token)) = self.find_next_significant_token()? else {
            return Err(ParseError::new(
                ParseErrorKind::NoTokens,
                self.cursor.eof_span(),
            ));
        };

        if token.kind.can_start_item() {
            self.parse_item()
        } else {
            Err(ParseError::new(
                ParseErrorKind::ExpectedItem(token.kind.clone()),
                token,
            ))
        }
    }

    pub fn parse_mod(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_keyword(Keyword::Mod)?;
        let module_ident = self.expect_ident()?;

        if self.check_punctuation_advance(Punctuation::SemiColon) {
            return Ok(Item::new(
                ItemKind::Mod(Module::new_boxed(
                    module_ident.into(),
                    ModuleKind::Declaration,
                )),
                public,
            ));
        }

        // Parse module body..
        self.expect_punctuation(Punctuation::OpenBrace)?;

        let mut items = Vec::new();

        if !self.check_punctuation_advance(Punctuation::CloseBrace) {
            items.push(self.parse_mod_item()?);

            // Parse a comma before each additional argument..
            while !self.cursor.is_end() {
                if self.check_punctuation_advance(Punctuation::CloseBrace) {
                    break;
                }

                items.push(self.parse_mod_item()?);
            }
        }

        Ok(Item::new(
            ItemKind::Mod(Module::new_boxed(
                module_ident.into(),
                ModuleKind::Definition(items),
            )),
            public,
        ))
    }
}
