use crate::ast::{
    Constant, Enum, EnumVariant, Function, FunctionReturnTy, FunctionSig, Impl, Item, ItemKind,
    Module, ModuleKind, Struct, StructField, UseImport, VariantData, Visibility,
};

use crate::parser::Parser;
use crate::parser::errors::{PResult, ParseError, ParseErrorKind};

use crate::token::{Keyword, Punctuation, TokenKind};

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
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        let native = self.check_kind_advance(Keyword::Native);
        self.expect_kind(Keyword::Fn)?;
        let func_name = self.expect_ident_spanned()?;

        let func_sig_span_start = self.begin_span();
        self.expect_kind(Punctuation::OpenParen)?;

        let function_arguments =
            self.parse_block_like(Punctuation::Comma, Punctuation::CloseParen, |s| {
                Ok(s.parse_variable_pattern()?.into_param())
            })?;
        let func_sig_span = self.finish_span(func_sig_span_start);

        for (i, arg) in function_arguments.iter().enumerate() {
            if arg.ident.str() == "self" {
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
            TokenKind::Punctuation(Punctuation::OpenBrace)
            | TokenKind::Punctuation(Punctuation::SemiColon) => FunctionReturnTy::Default,
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

        let decl_span = self.finish_span(span_start);

        let function_body = if self.check_kind_advance(Punctuation::SemiColon) {
            None
        } else {
            let function_body = self.parse_block_expression()?;
            if native {
                return Err(ParseError::new(
                    ParseErrorKind::NativeFunctionCannotDefineABody,
                    func_name.span,
                ));
            }
            Some(function_body)
        };

        let function_sig = FunctionSig {
            parameters: function_arguments,
            output: func_return_type,
            span: func_sig_span,
        };

        let item_kind = ItemKind::Fn(Box::new(Function::new(
            func_name,
            native,
            function_sig,
            decl_span,
            function_body,
        )));

        Ok(Item::new(item_kind, public, self.finish_span(span_start)))
    }

    fn parse_struct_field_definition(&mut self, allow_vis_specifier: bool) -> PResult<StructField> {
        let span_start = self.begin_span();

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
        let var_ident = self.expect_ident_spanned()?;
        self.expect_kind(Punctuation::Colon)?;
        let ty = self.parse_ty()?;

        Ok(StructField::new(
            var_ident,
            ty,
            public,
            self.finish_span(span_start),
        ))
    }

    pub fn parse_struct(&mut self) -> PResult<Item> {
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Struct)?;

        let struct_ident = self.expect_ident_spanned()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            // Unit struct
            return Ok(Item::new(
                ItemKind::Struct(Struct::new_boxed(struct_ident, vec![])),
                public,
                self.finish_span(span_start),
            ));
        }

        self.expect_kind(Punctuation::OpenBrace)?;

        let fields = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_struct_field_definition(true)
        })?;

        Ok(Item::new(
            ItemKind::Struct(Struct::new_boxed(struct_ident, fields)),
            public,
            self.finish_span(span_start),
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
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Enum)?;

        let enum_ident = self.expect_ident_spanned()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            // Enum with 0 variants.. (Unit enum)
            return Ok(Item::new(
                ItemKind::Enum(Enum::new_boxed(enum_ident, vec![])),
                public,
                self.finish_span(span_start),
            ));
        }

        self.expect_kind(Punctuation::OpenBrace)?;

        let variants = self.parse_block_like(Punctuation::Comma, Punctuation::CloseBrace, |s| {
            s.parse_enum_variant()
        })?;

        Ok(Item::new(
            ItemKind::Enum(Enum::new_boxed(enum_ident, variants)),
            public,
            self.finish_span(span_start),
        ))
    }

    pub fn parse_const(&mut self) -> PResult<Item> {
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Const)?;
        let var_ident = self.expect_ident_spanned()?;
        self.expect_kind(Punctuation::Colon)?;
        let ty = self.parse_ty()?;
        self.expect_kind(Punctuation::Eq)?;
        let expr = self.parse_expression()?;
        self.expect_kind(Punctuation::SemiColon)?;

        Ok(Item::new(
            ItemKind::Const(Constant::new_boxed(var_ident, ty, expr)),
            public,
            self.finish_span(span_start),
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
        let span_start = self.begin_span();

        self.expect_kind(Keyword::Impl)?;

        let target = self.parse_spanned_path()?;

        // Parse module body..
        self.expect_kind(Punctuation::OpenBrace)?;

        let items =
            self.parse_block_like_no_delimiter(Punctuation::CloseBrace, |s| s.parse_impl_item())?;

        Ok(Item::new(
            ItemKind::Impl(Impl::new_boxed(target, items)),
            Visibility::Public,
            self.finish_span(span_start),
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
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Use)?;

        let path = self.parse_spanned_path()?;

        self.expect_kind(Punctuation::SemiColon)?;

        Ok(Item::new(
            ItemKind::Use(UseImport::new(path)),
            public,
            self.finish_span(span_start),
        ))
    }

    pub fn parse_mod(&mut self) -> PResult<Item> {
        let span_start = self.begin_span();

        let public = self.parse_visibility()?;
        self.expect_kind(Keyword::Mod)?;
        let module_ident = self.expect_ident_spanned()?;

        if self.check_kind_advance(Punctuation::SemiColon) {
            return Ok(Item::new(
                ItemKind::Mod(Module::new_boxed(module_ident, ModuleKind::Declaration)),
                public,
                self.finish_span(span_start),
            ));
        }

        // Parse module body..
        self.expect_kind(Punctuation::OpenBrace)?;

        let items =
            self.parse_block_like_no_delimiter(Punctuation::CloseBrace, |s| s.parse_mod_item())?;

        Ok(Item::new(
            ItemKind::Mod(Module::new_boxed(
                module_ident,
                ModuleKind::Definition(items),
            )),
            public,
            self.finish_span(span_start),
        ))
    }
}
