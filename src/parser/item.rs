use crate::{
    ast::{
        Constant, Enum, EnumVariant, Function, FunctionReturnTy, FunctionSig, Impl, Item, ItemKind,
        Module, ModuleKind, Struct, StructField, UseImport, VariantData, Visibility,
    },
    parser::errors::{ParseError, ParseErrorKind},
    token::{Keyword, Punctuation, TokenKind},
};

use super::{Parser, errors::PResult};

impl Parser<'_, '_> {
    pub fn parse_item(&mut self) -> PResult<Item> {
        let (_, token) = self.expect_next_significant_token()?;
        match token.kind {
            TokenKind::Keyword(Keyword::Use) => self.parse_use(),
            TokenKind::Keyword(Keyword::Const) => self.parse_const(),
            TokenKind::Keyword(Keyword::Fn) => self.parse_function(false),
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

    pub fn parse_function(&mut self, from_impl_block: bool) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Fn)?;
        let func_name = self.expect_ident()?;
        self.expect_kind(Punctuation::OpenParen)?;

        let function_arguments =
            self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
                Ok(s.parse_variable_pattern()?.into_param())
            })?;

        for (i, arg) in function_arguments.iter().enumerate() {
            if arg.ident.0 == "self" {
                if !from_impl_block {
                    return Err(ParseError::new(
                        ParseErrorKind::SelfMustBeUsedInAMethod,
                        arg.span,
                    ));
                } else if i > 0 {
                    return Err(ParseError::new(
                        ParseErrorKind::SelfMustBeFirstArgument,
                        arg.span,
                    ));
                }
            }
        }

        let return_type_token = self.peek_at(0)?;
        let func_return_type = match return_type_token.kind {
            TokenKind::Punctuation(Punctuation::OpenBrace) => FunctionReturnTy::Default,
            TokenKind::Punctuation(Punctuation::Minus) => {
                self.cursor.advance();
                self.expect_kind(Punctuation::GreaterThan)?;
                FunctionReturnTy::Ty(Box::new(self.parse_ty()?))
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
            func_name.into(),
            function_sig,
            Some(function_body),
        )));

        Ok(Item::new(item_kind, public))
    }

    fn parse_struct_field_definition(&mut self, allow_vis_specifier: bool) -> PResult<StructField> {
        let public = if allow_vis_specifier {
            self.parse_visibility()?
        } else {
            if self.check_kind(Keyword::Pub) {
                return Err(self.make_error(ParseErrorKind::UnexpectedToken(
                    self.peek_at(0)?.kind.clone(),
                )));
            }
            Visibility::Public
        };
        let var_ident = self.expect_ident()?;
        self.expect_kind(Punctuation::Colon)?;
        let ty = self.parse_ty()?;

        Ok(StructField::new(var_ident.into(), ty, public))
    }

    pub fn parse_struct(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Struct)?;

        let struct_ident = self.expect_ident()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            // Unit struct
            return Ok(Item::new(
                ItemKind::Struct(Struct::new_boxed(struct_ident.into(), vec![])),
                public,
            ));
        }

        self.expect_kind(Punctuation::OpenBrace)?;

        let fields = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_struct_field_definition(true)
        })?;

        Ok(Item::new(
            ItemKind::Struct(Struct::new_boxed(struct_ident.into(), fields)),
            public,
        ))
    }

    fn parse_enum_variant(&mut self) -> PResult<EnumVariant> {
        let variant_ident = self.expect_ident()?;

        if self.check_kind_advance(Punctuation::OpenBrace) {
            // 'Struct' style enum..
            let struct_fields =
                self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
                    s.parse_struct_field_definition(false)
                })?;

            Ok(EnumVariant::new(
                variant_ident.into(),
                VariantData::Struct(struct_fields),
            ))
        } else if self.check_kind_advance(Punctuation::OpenParen) {
            // 'Tuple' style enum..
            let tuple_tys =
                self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
                    s.parse_ty()
                })?;

            Ok(EnumVariant::new(
                variant_ident.into(),
                VariantData::Tuple(tuple_tys),
            ))
        } else {
            Ok(EnumVariant::new(variant_ident.into(), VariantData::Unit))
        }
    }

    pub fn parse_enum(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Enum)?;

        let enum_ident = self.expect_ident()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            // Enum with 0 variants.. (Unit enum)
            return Ok(Item::new(
                ItemKind::Enum(Enum::new_boxed(enum_ident.into(), vec![])),
                public,
            ));
        }

        self.expect_kind(Punctuation::OpenBrace)?;

        let variants = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_enum_variant()
        })?;

        Ok(Item::new(
            ItemKind::Enum(Enum::new_boxed(enum_ident.into(), variants)),
            public,
        ))
    }

    pub fn parse_const(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Const)?;
        let var_ident = self.expect_ident()?;
        self.expect_kind(Punctuation::Colon)?;
        let ty = self.parse_ty()?;
        self.expect_kind(Punctuation::Eq)?;
        let expr = self.parse_expression()?;
        self.expect_kind(Punctuation::SemiColon)?;

        Ok(Item::new(
            ItemKind::Const(Constant::new_boxed(var_ident.into(), ty, expr)),
            public,
        ))
    }

    fn parse_impl_item(&mut self) -> PResult<Item> {
        let (_, token) = self.expect_next_significant_token()?;
        match token.kind {
            TokenKind::Keyword(Keyword::Const) => self.parse_const(),
            TokenKind::Keyword(Keyword::Fn) => self.parse_function(true),
            _ => Err(ParseError::new(
                ParseErrorKind::WrongTokenKind(token.kind.clone()),
                token,
            )),
        }
    }

    pub fn parse_implementation(&mut self) -> PResult<Item> {
        self.expect_kind(Keyword::Impl)?;
        let ty = self.parse_path()?;

        // Parse module body..
        self.expect_kind(Punctuation::OpenBrace)?;

        let items =
            self.parse_block_like_no_delimiter(Punctuation::CloseBrace, |s| s.parse_impl_item())?;

        Ok(Item::new(
            ItemKind::Impl(Impl::new_boxed(ty, items)),
            Visibility::Public,
        ))
    }

    fn parse_mod_item(&mut self) -> PResult<Item> {
        let Some((_offset, token)) = self.find_next_significant_token()? else {
            return Err(self.no_token_error());
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

    pub fn parse_use(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Use)?;
        let path = self.parse_path()?;

        self.expect_kind(Punctuation::SemiColon)?;

        Ok(Item::new(ItemKind::Use(UseImport::new(path)), public))
    }

    pub fn parse_mod(&mut self) -> PResult<Item> {
        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Mod)?;
        let module_ident = self.expect_ident()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            return Ok(Item::new(
                ItemKind::Mod(Module::new_boxed(
                    module_ident.into(),
                    ModuleKind::Declaration,
                )),
                public,
            ));
        }

        // Parse module body..
        self.expect_kind(Punctuation::OpenBrace)?;

        let items =
            self.parse_block_like_no_delimiter(Punctuation::CloseBrace, |s| s.parse_mod_item())?;

        Ok(Item::new(
            ItemKind::Mod(Module::new_boxed(
                module_ident.into(),
                ModuleKind::Definition(items),
            )),
            public,
        ))
    }
}
