use crate::ast::{self, SourceSpan, Visibility};

use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::nodes::{
    Block, Constant, Enum, Expr, ExprKind, Function, FunctionBody, FunctionSig, HIRNode, Impl,
    Item, ItemKind, LetStatement, Module, ModuleIdent, ModuleSpan, OwningNode, OwningNodeKind,
    Parameter, RefPath, Statement, StatementKind, Struct, StructField, StructFieldInit,
    StructInitialisation, Type, UsePath,
};
use crate::intermediate::hir::{HLIR, HirId, LoweringError, LoweringErrorKind, OwnerDefId};

struct HLIRLowerer<'a> {
    hlir: &'a mut HLIR,
}

// High-level parse logic..
impl HLIRLowerer<'_> {
    fn lower_ast_impl_item(
        &mut self,
        item: &ast::Item,
        parent_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        match &item.kind {
            ast::ItemKind::Const(constant) => {
                self.lower_const(constant, item.span, parent_node, true, item.vis)
            }
            ast::ItemKind::Fn(function) => {
                self.lower_function(function, item.span, parent_node, true, item.vis)
            }
            _ => {
                // This shouldn't be possible..
                Err(LoweringError::new(LoweringErrorKind::RemoveMeMessage(
                    "Non-allowed item type in impl block?".into(),
                    Some(item.span),
                )))
            }
        }
    }

    fn lower_ast_item(
        &mut self,
        item: &ast::Item,
        parent_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        match &item.kind {
            ast::ItemKind::Const(constant) => {
                self.lower_const(constant, item.span, parent_node, false, item.vis)
            }
            ast::ItemKind::Fn(function) => {
                self.lower_function(function, item.span, parent_node, false, item.vis)
            }
            ast::ItemKind::Mod(module) => {
                self.lower_module(module, item.span, parent_node, item.vis)
            }
            ast::ItemKind::Enum(e) => self.lower_enum(e, item.span, parent_node, item.vis),
            ast::ItemKind::Struct(s) => self.lower_struct(s, item.span, parent_node, item.vis),
            ast::ItemKind::Impl(im) => self.lower_impl(im, item.span, parent_node),
            ast::ItemKind::Use(useimport) => {
                self.lower_use(useimport, item.span, parent_node, item.vis)
            }
        }
    }

    fn walk_items(&mut self, items: &[ast::Item], parent_node: OwnerDefId) -> LowerResult<()> {
        for item in items {
            self.lower_ast_item(item, parent_node)?;
        }

        Ok(())
    }

    pub fn build_hir_representation(&mut self, ast: &ast::ASTRoot) -> LowerResult<()> {
        let top_level_mod_id = self
            .hlir
            .insert_owning_node(OwningNode::new(OwningNodeKind::Item(Item::new(
                ItemKind::Module(Module {
                    owner_id: OwnerDefId::ROOT_NODE,
                    ident: ModuleIdent::RawIdent("RootModule".into()),
                    span: ModuleSpan::Implementation(ast.full_file_span),
                    vis: Visibility::Public,
                    item_ids: vec![],
                }),
            ))));

        self.walk_items(&ast.items, top_level_mod_id)?;

        Ok(())
    }
}

// Individual lowering implementations..
impl HLIRLowerer<'_> {
    fn lower_module(
        &mut self,
        m: &ast::Module,
        span: SourceSpan,
        parent_node: OwnerDefId,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let mod_id = self.hlir.next_owner_id();
        let mod_span = match &m.kind {
            ast::ModuleKind::Declaration => ModuleSpan::Declaration(span),
            ast::ModuleKind::Definition(_) => ModuleSpan::Implementation(span),
        };
        let item = Item::new(ItemKind::Module(Module {
            owner_id: mod_id,
            ident: ModuleIdent::SpannedIdent(m.ident.clone()),
            span: mod_span,
            vis,
            item_ids: vec![],
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(item)),
            parent_node,
        );
        if let ast::ModuleKind::Definition(is) = &m.kind {
            self.walk_items(is, mod_id)?;
        }

        Ok(mod_id)
    }

    fn lower_function(
        &mut self,
        f: &ast::Function,
        span: SourceSpan,
        parent_node: OwnerDefId,
        impl_item: bool,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let fn_node_id = self.hlir.next_owner_id();
        let fn_out_type = match &f.sig.output {
            ast::FunctionReturnTy::Default => Type::unit(),
            ast::FunctionReturnTy::Ty(ty) => Type::from_ast_ty(ty),
        };
        let node_item = Item::new(ItemKind::Function(Function {
            owner_id: fn_node_id,
            ident: f.ident.clone(),
            vis,
            native: f.native,
            is_method: f.is_method,
            is_global: f.is_global,
            sig: FunctionSig {
                parameters: f
                    .sig
                    .parameters
                    .iter()
                    .map(|p| Type::from_ast_ty(&p.ty))
                    .collect(),
                output: fn_out_type,
                span: f.sig.span,
            },
            decl_span: f.decl_span,
            full_span: span,
            body: None,
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::from_item(node_item, impl_item),
            parent_node,
        );

        let mut param_ids = Vec::new();
        for param in &f.sig.parameters {
            let param_id = self.hlir.next_hir_id_on(fn_node_id);
            self.hlir.insert_hir_node(
                fn_node_id,
                HIRNode::Param(Parameter {
                    id: param_id,
                    fn_id: fn_node_id,
                    ident: param.ident.clone(),
                    span: param.span,
                    mutable: param.mutable,
                }),
            );
            param_ids.push(param_id);
        }

        if !f.native {
            // Parse block expression..
            if let Some(body_block) = &f.body {
                let body_id = self.lower_block(body_block, true, fn_node_id)?;

                self.hlir
                    .owning_node_mut_unchecked(fn_node_id)
                    .hir_function_mut()
                    .expect("Function exists.")
                    .body = Some(FunctionBody {
                    block: body_id,
                    params: param_ids,
                });
            }
        }

        Ok(fn_node_id)
    }

    fn lower_struct(
        &mut self,
        ast_struct: &ast::Struct,
        span: SourceSpan,
        owner_node: OwnerDefId,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let struct_node_id = self.hlir.next_owner_id();
        let struct_item = Item::new(ItemKind::Struct(Struct {
            owner_id: struct_node_id,
            ident: ast_struct.ident.clone(),
            span,
            vis,
            fields: vec![],
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(struct_item)),
            owner_node,
        );

        let mut field_ids = Vec::new();
        for s in &ast_struct.fields {
            let field_id = self
                .hlir
                .insert_hir_node(
                    struct_node_id,
                    HIRNode::Field(StructField {
                        id: self.hlir.next_hir_id_on(struct_node_id),
                        ident: s.ident.clone(),
                        span: s.span,
                        ty: Type::from_ast_ty(&s.ty),
                        vis: s.vis,
                    }),
                )
                .unwrap_or(HirId::PLACEHOLDER_ID);
            field_ids.push(field_id);
        }

        self.hlir
            .owning_node_mut_unchecked(struct_node_id)
            .hir_struct_mut()
            .expect("Is a struct.")
            .fields = field_ids;

        Ok(struct_node_id)
    }

    // FIXME: This isn't implemented properly.
    fn lower_enum(
        &mut self,
        ast_enum: &ast::Enum,
        span: SourceSpan,
        owner_node: OwnerDefId,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let enum_node_id = self.hlir.next_owner_id();
        let enum_item = Item::new(ItemKind::Enum(Enum {
            owner_id: enum_node_id,
            ident: ast_enum.ident.clone(),
            span,
            vis,
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(enum_item)),
            owner_node,
        );

        Ok(enum_node_id)
    }

    fn lower_const(
        &mut self,
        ast_const: &ast::Constant,
        span: SourceSpan,
        owner_node: OwnerDefId,
        impl_item: bool,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let const_node_id = self.hlir.next_owner_id();
        let const_item = Item::new(ItemKind::Constant(Constant {
            owner_id: const_node_id,
            ident: ast_const.ident.clone(),
            span,
            ty: Type::from_ast_ty(&ast_const.ty),
            vis,
            expr: HirId::PLACEHOLDER_ID,
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::from_item(const_item, impl_item),
            owner_node,
        );

        let value_expr = self.lower_expression(&ast_const.expr, const_node_id)?;

        self.hlir
            .owning_node_mut_unchecked(const_node_id)
            .hir_const_mut()
            .expect("Is a constant.")
            .expr = value_expr;

        Ok(const_node_id)
    }

    fn lower_impl(
        &mut self,
        im: &ast::Impl,
        span: SourceSpan,
        owner_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        let impl_node_id = self.hlir.next_owner_id();
        let impl_item = Item::new(ItemKind::Impl(Impl {
            span,
            ty_span: im.target_path.span,
            owner_id: impl_node_id,
            self_ty: im.target_path.path.clone(),
            items: vec![],
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(impl_item)),
            owner_node,
        );

        let mut impl_item_ids = Vec::new();
        for impl_item in &im.items {
            let impl_item_id = self.lower_ast_impl_item(impl_item, owner_node)?;
            impl_item_ids.push(impl_item_id);
        }

        self.hlir
            .owning_node_mut_unchecked(impl_node_id)
            .hir_impl_mut()
            .expect("Is an impl node.")
            .items = impl_item_ids;

        Ok(impl_node_id)
    }

    fn lower_use(
        &mut self,
        useimport: &ast::UseImport,
        span: SourceSpan,
        owner_node: OwnerDefId,
        vis: Visibility,
    ) -> LowerResult<OwnerDefId> {
        let use_node_id = self.hlir.next_owner_id();
        let use_item = Item::new(ItemKind::Use(UsePath {
            owner_id: use_node_id,
            imports: useimport.imports.clone(),
            span,
            resolved_id: None,
            vis,
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(use_item)),
            owner_node,
        );

        Ok(use_node_id)
    }

    fn lower_block(
        &mut self,
        block: &ast::Block,
        root_block: bool,
        owner_node: OwnerDefId,
    ) -> LowerResult<HirId> {
        let block_id = self.hlir.next_hir_id_on(owner_node);
        self.hlir.insert_hir_node(
            owner_node,
            HIRNode::Block(Block {
                id: block_id,
                statements: vec![],
                root_block,
                span: block.span,
            }),
        );

        let mut statement_node_ids = Vec::new();
        let statement_count = block.statements.len();
        for (i, statement) in block.statements.iter().enumerate() {
            if let Some(statement_id) = self.lower_statement(
                statement,
                owner_node,
                i.saturating_add(1) == statement_count,
            )? {
                statement_node_ids.push(statement_id);
            }
        }

        if let HIRNode::Block(block) = self.hlir.get_hir_node_mut_unchecked(block_id) {
            block.statements = statement_node_ids;
        } else {
            Err(LoweringError::new(LoweringErrorKind::RemoveMeMessage(
                "Failed to get block node after insertion?? WHAT".into(),
                None,
            )))?;
        }

        Ok(block_id)
    }

    fn lower_expression(
        &mut self,
        expr: &ast::Expression,
        owner_node: OwnerDefId,
    ) -> LowerResult<HirId> {
        let expr_kind = match &expr.kind {
            ast::ExpressionKind::Literal(literal) => Ok(ExprKind::Literal(literal.clone())),
            ast::ExpressionKind::IdentPath(ident_path) => {
                Ok(ExprKind::Path(RefPath::Unresolved(ident_path.clone())))
            }
            ast::ExpressionKind::Continue => Ok(ExprKind::Continue),
            ast::ExpressionKind::Break => Ok(ExprKind::Break),
            ast::ExpressionKind::Return(ret_val) => match ret_val {
                Some(expr) => Ok(ExprKind::Return(Some(
                    self.lower_expression(expr, owner_node)?,
                ))),
                None => Ok(ExprKind::Return(None)),
            },

            ast::ExpressionKind::Block(block) => {
                let block_id = self.lower_block(block, false, owner_node)?;
                Ok(ExprKind::Block(block_id))
            }
            ast::ExpressionKind::BinaryOp(binary_op_kind, lhs, rhs) => {
                let expr_1 = self.lower_expression(lhs, owner_node)?;
                let expr_2 = self.lower_expression(rhs, owner_node)?;
                Ok(ExprKind::BinaryOp(*binary_op_kind, expr_1, expr_2))
            }
            ast::ExpressionKind::UnaryOp(unary_op_kind, rhs) => {
                let expr_1 = self.lower_expression(rhs, owner_node)?;
                Ok(ExprKind::UnaryOp(*unary_op_kind, expr_1))
            }
            ast::ExpressionKind::If(expression, block, expression1) => {
                let condition = self.lower_expression(expression, owner_node)?;
                let if_block = self.lower_block(block, false, owner_node)?;
                let else_expr = if let Some(else_e) = expression1 {
                    Some(self.lower_expression(else_e, owner_node)?)
                } else {
                    None
                };
                Ok(ExprKind::If(condition, if_block, else_expr))
            }
            ast::ExpressionKind::While(expression, block) => {
                let condition = self.lower_expression(expression, owner_node)?;
                let while_block = self.lower_block(block, false, owner_node)?;
                Ok(ExprKind::While(condition, while_block))
            }
            ast::ExpressionKind::Assign(expression, expression1) => {
                let lhs = self.lower_expression(expression, owner_node)?;
                let rhs = self.lower_expression(expression1, owner_node)?;
                Ok(ExprKind::Assign(lhs, rhs))
            }
            ast::ExpressionKind::Call(expression, expressions) => {
                let to_call = self.lower_expression(expression, owner_node)?;
                let mut arg_ids = Vec::new();
                for arg_expr in expressions {
                    arg_ids.push(self.lower_expression(arg_expr, owner_node)?);
                }
                Ok(ExprKind::Call(to_call, arg_ids))
            }
            ast::ExpressionKind::MethodCall(method_call) => {
                let target_expr = self.lower_expression(&method_call.target_expr, owner_node)?;
                let method_ident = method_call.method_ident.ident();
                let mut arg_ids = Vec::new();
                for arg_expr in &method_call.args {
                    arg_ids.push(self.lower_expression(arg_expr, owner_node)?);
                }
                Ok(ExprKind::MethodCall(target_expr, method_ident, arg_ids))
            }
            ast::ExpressionKind::Index(expression, expression1) => {
                let target_expr = self.lower_expression(expression, owner_node)?;
                let index_expr = self.lower_expression(expression1, owner_node)?;
                Ok(ExprKind::Index(target_expr, index_expr))
            }
            ast::ExpressionKind::FieldAccess(expression, ident) => {
                let target_expr = self.lower_expression(expression, owner_node)?;
                Ok(ExprKind::FieldAccess(target_expr, ident.clone()))
            }
            ast::ExpressionKind::StructInit(struct_initialisation) => {
                let mut field_inits = Vec::new();
                for field in &struct_initialisation.fields {
                    let field_val_expr = self.lower_expression(&field.expr, owner_node)?;
                    field_inits.push(StructFieldInit {
                        ident: field.ident.clone(),
                        expr: field_val_expr,
                    });
                }
                Ok(ExprKind::StructInit(StructInitialisation {
                    ty_path: RefPath::Unresolved(struct_initialisation.path.clone()),
                    fields: field_inits,
                }))
            }
        }?;

        let hir_expr = Expr {
            id: self.hlir.next_hir_id_on(owner_node),
            kind: expr_kind,
            span: expr.span,
        };

        // FIXME: Probably error if it can't be lowered.
        Ok(self
            .hlir
            .insert_hir_node(owner_node, HIRNode::Expr(hir_expr))
            .unwrap_or(HirId::PLACEHOLDER_ID))
    }

    fn lower_statement(
        &mut self,
        statement: &ast::Statement,
        owner_node: OwnerDefId,
        is_last_statement: bool,
    ) -> LowerResult<Option<HirId>> {
        let lowered = match &statement.kind {
            ast::StatementKind::Let(local) => {
                let init_value = if let ast::LocalKind::Initialise(expr) = &local.kind {
                    Some(self.lower_expression(expr, owner_node)?)
                } else {
                    None
                };

                let hir_statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Let(LetStatement {
                        ident: local.ident.ident.clone(),
                        mutable: local.mutable,
                        ty: Type::from_ast_ty(&local.ty),
                        initial_value: init_value,
                    }),
                    span: statement.span,
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(hir_statement))
            }
            ast::StatementKind::Item(item) => {
                let item_id = self.lower_ast_item(item, owner_node)?;
                let hir_statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Item(item_id),
                    span: statement.span,
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(hir_statement))
            }
            ast::StatementKind::Expr(expression) => {
                let expr_id = self.lower_expression(expression, owner_node)?;
                let is_control_flow =
                    if let Some(HIRNode::Expr(e)) = self.hlir.get_hir_node(expr_id) {
                        matches!(&e.kind, ExprKind::If(_, _, _) | ExprKind::While(_, _))
                    } else {
                        false
                    };
                let hir_statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: if is_control_flow && !is_last_statement {
                        StatementKind::Semi(expr_id)
                    } else {
                        StatementKind::Expr(expr_id)
                    },
                    span: statement.span,
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(hir_statement))
            }
            ast::StatementKind::Semi(expression) => {
                let expr_id = self.lower_expression(expression, owner_node)?;
                let hir_statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Semi(expr_id),
                    span: statement.span,
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(hir_statement))
            }
            ast::StatementKind::Empty => None,
        };
        Ok(lowered)
    }
}

pub fn lower_ast_to_hir(ast: &ast::ASTRoot) -> LowerResult<HLIR> {
    let mut hir = HLIR::default();

    {
        let mut lowerer = HLIRLowerer { hlir: &mut hir };
        lowerer.build_hir_representation(ast)?;
    }

    Ok(hir)
}
