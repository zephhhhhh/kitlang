use crate::{
    ast,
    intermediate::hir::errors::{LoweringError, LoweringErrorKind},
};

pub mod errors;
use errors::LowerResult;

pub mod nodes;
use nodes::{HIRNode, Item, ItemKind, OwningNode, OwningNodeKind};

pub mod exprs;
use exprs::{Expr, ExprKind};

pub mod statements;
use statements::{LetStatement, Statement, StatementKind};

pub mod visitor;

/// Definition ID that is only valid when relative to the "owner".
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDefId(pub u32);

impl LocalDefId {
    pub const PLACEHOLDER_ID: LocalDefId = LocalDefId(u32::MAX);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }
}

impl ::std::fmt::Debug for LocalDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalDefId({})", self.0)
    }
}

/// Definition ID that describes the index into the global "module" nodes of an owning HIR node.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerDefId(pub u32);

impl OwnerDefId {
    pub const PLACEHOLDER_ID: OwnerDefId = OwnerDefId(u32::MAX);
    pub const ROOT_NODE: OwnerDefId = OwnerDefId(0);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }
}

impl ::std::fmt::Debug for OwnerDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OwnerDefId({})", self.0)
    }
}

/// Definition ID that is relative to a specified module.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefId {
    pub module_id: u32,
    pub def_id: LocalDefId,
}

impl DefId {
    pub const PLACEHOLDER_ID: DefId = DefId {
        module_id: u32::MAX,
        def_id: LocalDefId::PLACEHOLDER_ID,
    };

    pub fn is_placeholder(self) -> bool {
        self.module_id == u32::MAX || self.def_id.is_placeholder()
    }
}

impl ::std::fmt::Debug for DefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefId({} : {:?})", self.module_id, self.def_id)
    }
}

/// Definition ID that describes which ID into the modules "path_table" owns the node, and which
/// index into the owners body the node refers to.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirId {
    pub owner: OwnerDefId,
    pub id: LocalDefId,
}

impl From<HirId> for LocalDefId {
    fn from(value: HirId) -> Self {
        value.id
    }
}

impl From<HirId> for OwnerDefId {
    fn from(value: HirId) -> Self {
        value.owner
    }
}

impl From<&HirId> for LocalDefId {
    fn from(value: &HirId) -> Self {
        value.id
    }
}

impl From<&HirId> for OwnerDefId {
    fn from(value: &HirId) -> Self {
        value.owner
    }
}

impl HirId {
    pub const PLACEHOLDER_ID: HirId = HirId {
        owner: OwnerDefId::PLACEHOLDER_ID,
        id: LocalDefId::PLACEHOLDER_ID,
    };

    pub fn is_placeholder(self) -> bool {
        self.owner.is_placeholder() || self.id.is_placeholder()
    }
}

impl HirId {
    pub fn next_id(mut self) -> Self {
        self.id = LocalDefId(self.id.0 + 1);
        self
    }
}

impl ::std::fmt::Debug for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HirId({:?} : {:?})", self.owner, self.id)
    }
}

#[derive(Default)]
pub struct HLIR {
    pub owner_nodes: Vec<OwningNode>,
}

impl HLIR {
    pub fn make_error(&self, kind: LoweringErrorKind) -> LoweringError {
        LoweringError::new(kind)
    }
}

impl HLIR {
    pub fn owner_node_count(&self) -> u32 {
        self.owner_nodes.len() as u32
    }

    pub fn root_module(&self) -> Option<&OwningNode> {
        self.owning_node(OwnerDefId::ROOT_NODE)
    }

    pub fn root_module_mut(&mut self) -> Option<&mut OwningNode> {
        self.owning_node_mut(OwnerDefId::ROOT_NODE)
    }

    pub fn owning_node(&self, id: OwnerDefId) -> Option<&OwningNode> {
        self.owner_nodes.get(id.0 as usize)
    }

    pub fn owning_node_mut(&mut self, id: OwnerDefId) -> Option<&mut OwningNode> {
        self.owner_nodes.get_mut(id.0 as usize)
    }

    pub fn owning_node_unchecked(&self, id: OwnerDefId) -> &OwningNode {
        self.owner_nodes.get(id.0 as usize).expect("Node exists.")
    }

    pub fn owning_node_mut_unchecked(&mut self, id: OwnerDefId) -> &mut OwningNode {
        self.owner_nodes
            .get_mut(id.0 as usize)
            .expect("Node exists.")
    }

    pub fn next_owner_id(&self) -> OwnerDefId {
        OwnerDefId(self.owner_node_count())
    }

    pub fn insert_owning_node(&mut self, mut owner_node: OwningNode) -> OwnerDefId {
        let next_id = self.next_owner_id();
        owner_node.set_owner_id(next_id);
        self.owner_nodes.push(owner_node);
        next_id
    }

    pub fn insert_owning_node_with_parent(
        &mut self,
        owner_node: OwningNode,
        parent_id: OwnerDefId,
    ) -> OwnerDefId {
        let is_normal_item = matches!(&owner_node.kind, OwningNodeKind::Item(_));
        let new_node_id = self.insert_owning_node(owner_node);
        if is_normal_item {
            self.update_parent_item_list(new_node_id, parent_id);
        }
        new_node_id
    }

    fn update_parent_item_list(&mut self, new_node: OwnerDefId, parent_id: OwnerDefId) {
        if let Some(parent) = self.owning_node_mut(parent_id) {
            if let Some(parent_module) = parent.hir_module_mut() {
                parent_module.item_ids.push(new_node);
            }
        }
    }
}

impl HLIR {
    pub fn get_hir_node(&self, hir_id: HirId) -> Option<&HIRNode> {
        self.owning_node(hir_id.owner)?.get_hir_node(hir_id.id)
    }

    pub fn get_hir_node_unchecked(&self, hir_id: HirId) -> &HIRNode {
        self.owning_node_unchecked(hir_id.owner)
            .get_hir_node(hir_id.id)
            .expect("HIRNode exists.")
    }

    pub fn get_hir_node_mut(&mut self, hir_id: HirId) -> Option<&mut HIRNode> {
        self.owning_node_mut(hir_id.owner)?
            .get_hir_node_mut(hir_id.id)
    }

    pub fn get_hir_node_mut_unchecked(&mut self, hir_id: HirId) -> &mut HIRNode {
        self.owning_node_mut_unchecked(hir_id.owner)
            .get_hir_node_mut(hir_id.id)
            .expect("HIRNode exists.")
    }

    pub fn next_hir_id_on(&self, owner_id: impl Into<OwnerDefId>) -> HirId {
        if let Some(owner_node) = self.owning_node(owner_id.into()) {
            owner_node.next_hir_id()
        } else {
            HirId::PLACEHOLDER_ID
        }
    }

    pub fn insert_hir_node(
        &mut self,
        owner_id: impl Into<OwnerDefId>,
        hir_node: HIRNode,
    ) -> Option<HirId> {
        let owner = owner_id.into();
        let local_id = self.owning_node_mut(owner)?.insert_hir_node(hir_node);
        Some(HirId {
            owner,
            id: local_id,
        })
    }
}

impl HLIR {
    pub fn owning_node_item(&self, owner_id: OwnerDefId) -> Option<&Item> {
        self.owning_node(owner_id)?.item()
    }

    pub fn owning_node_item_mut(&mut self, owner_id: OwnerDefId) -> Option<&mut Item> {
        self.owning_node_mut(owner_id)?.item_mut()
    }
}

// /// Trait for implementing a way of walking the HIR.
// /// # Notes
// /// Each function returns a bool indicating if the walking should continue.
// /// The functions to be overriden are the `visit_` functions,
// /// functions beginning with `super_` are the default implementations
// pub trait HLIRVisitor {
//     fn visit_module(&mut self, module: &nodes::Module) -> bool { true }
//     fn visit_item(&mut self, item: &Item) -> bool { true }
//     fn visit_function(&mut self, func: &nodes::Function) -> bool { true }
//     fn visit_block(&mut self, block: &nodes::Block) -> bool { true }
//     fn visit_statement(&mut self, statement: &statements::Statement) -> bool { true }
//     fn visit_expr(&mut self, expr: &exprs::Expr) -> bool { true }
//     fn visit_impl(&mut self, imp: &nodes::Impl) -> bool { true }
//     fn visit_struct(&mut self, struct_info: &nodes::Struct) { }
//     fn visit_enum(&mut self, enum_info: &nodes::Enum) { }
// }

// pub fn walk_hlir_root(hlir: &HLIR, mut visitor: &mut impl HLIRVisitor)

// impl HLIR {
//     // fn walk_module(&self, module: OwnerDefId, visitor: &mut impl HLIRVisitor) -> Option<bool> {
//     //     let root = self.root_module()?.hir_module_ref()?;
//     //     let continue_walk = visitor.visit_module(root);
//     //     Some(visitor.visit_module(root))
//     // }
//
//     fn walk_items(&self, items: &[OwnerDefId], visitor: &mut impl HLIRVisitor) -> Option<()> {
//         for item_id in items {
//             if let Some(node) = self.owning_node(*item_id) {
//                 if let OwningNodeKind::Item(item) = &node.kind {
//                     if !visitor.visit_item(item) {
//                         continue;
//                     }
//                     match &item.kind {
//                         ItemKind::Module(_) => self.walk_module(item.owner_id, visitor)?,
//                         ItemKind::Function(_) => self.walk_function(item.owner_id, visitor)?,
//                         ItemKind::Struct(s) => visitor.visit_struct(s),
//                         ItemKind::Enum(e) => visitor.visit_enum(e),
//                         ItemKind::Constant(constant) => todo!(),
//                         ItemKind::Impl(_) => todo!(),
//                         ItemKind::Use(use_path) => todo!(),
//                     }
//                 }
//             }
//         }
//
//         Some(())
//     }
//
//     fn walk_function(&self, function_id: OwnerDefId, visitor: &mut impl HLIRVisitor) -> Option<()> {
//         Some(())
//     }
//
//     fn walk_module(&self, module_id: OwnerDefId, visitor: &mut impl HLIRVisitor) -> Option<()> {
//         let module = self.owning_node(module_id)?.hir_module_ref()?;
//
//         let items_to_walk = if visitor.visit_module(module) {
//             Some(module.item_ids.clone())
//         } else {
//             None
//         };
//
//         if let Some(item_ids) = items_to_walk {
//             self.walk_items(&item_ids, visitor)
//         } else {
//             Some(())
//         }
//     }
//
//     pub fn walk(&self, mut visitor: impl HLIRVisitor) -> Option<()> {
//         self.walk_module(OwnerDefId::ROOT_NODE, &mut visitor)
//     }
// }

impl ::std::fmt::Debug for HLIR {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // let derp = self.owner_node_lut.iter().map(|cont| cont.node()).collect::<Vec<_>>();
        f.debug_struct("HLIR")
            .field("hir_nodes", &self.owner_nodes)
            .finish()
    }
}

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
            ast::ItemKind::Const(constant) => self.lower_const(constant, parent_node, true),
            ast::ItemKind::Fn(function) => self.lower_function(function, parent_node, true),
            _ => {
                // This shouldn't be possible..
                panic!("Non-allowed item type in impl block?");
            }
        }
    }

    fn lower_ast_item(
        &mut self,
        item: &ast::Item,
        parent_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        match &item.kind {
            ast::ItemKind::Const(constant) => self.lower_const(constant, parent_node, false),
            ast::ItemKind::Fn(function) => self.lower_function(function, parent_node, false),
            ast::ItemKind::Mod(module) => self.lower_module(module, parent_node),
            ast::ItemKind::Enum(e) => self.lower_enum(e, parent_node),
            ast::ItemKind::Struct(s) => self.lower_struct(s, parent_node),
            ast::ItemKind::Impl(im) => self.lower_impl(im, parent_node),
            ast::ItemKind::Use(useimport) => self.lower_use(useimport, parent_node),
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
                ItemKind::Module(nodes::Module {
                    owner_id: OwnerDefId::ROOT_NODE,
                    ident: "ModuleRoot".into(),
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
        parent_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        let module_ident = m.ident.string();
        let mod_id = self.hlir.next_owner_id();
        let item = Item::new(ItemKind::Module(nodes::Module {
            owner_id: mod_id,
            ident: module_ident.into(),
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
        parent_node: OwnerDefId,
        impl_item: bool,
    ) -> LowerResult<OwnerDefId> {
        let fn_node_id = self.hlir.next_owner_id();
        let node_item = Item::new(ItemKind::Function(nodes::Function {
            owner_id: fn_node_id,
            ident: f.ident.ident(),
            sig: nodes::FunctionSig {
                parameters: f.sig.parameters.iter().map(|p| p.ty.clone()).collect(),
                output: if let ast::FunctionReturnTy::Ty(t) = &f.sig.output {
                    nodes::FunctionReturnTy::Ty(t.clone())
                } else {
                    nodes::FunctionReturnTy::Default
                },
            },
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
                HIRNode::Param(nodes::Parameter {
                    id: param_id,
                    ident: param.ident.ident(),
                    mutable: param.mutable,
                }),
            );
            param_ids.push(param_id);
        }

        // Parse block expression..
        if let Some(body_block) = &f.body {
            let body_id = self.lower_block(body_block, fn_node_id)?;

            self.hlir
                .owning_node_mut_unchecked(fn_node_id)
                .hir_function_mut()
                .expect("Function exists.")
                .body = Some(nodes::FunctionBody {
                block: body_id,
                params: param_ids,
            });
        }

        Ok(fn_node_id)
    }

    fn lower_struct(
        &mut self,
        ast_struct: &ast::Struct,
        owner_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        let struct_node_id = self.hlir.next_owner_id();
        let struct_item = Item::new(ItemKind::Struct(nodes::Struct {
            owner_id: struct_node_id,
            ident: ast_struct.ident.ident(),
            vis: ast::Visibility::Public,
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
                    HIRNode::Field(nodes::StructField {
                        id: self.hlir.next_hir_id_on(struct_node_id),
                        ident: s.ident.ident(),
                        ty: s.ty.clone(),
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
        owner_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        let enum_node_id = self.hlir.next_owner_id();
        let enum_item = Item::new(ItemKind::Enum(nodes::Enum {
            owner_id: enum_node_id,
            ident: ast_enum.ident.clone(),
            vis: ast::Visibility::Public,
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
        owner_node: OwnerDefId,
        impl_item: bool,
    ) -> LowerResult<OwnerDefId> {
        let const_node_id = self.hlir.next_owner_id();
        let const_item = Item::new(ItemKind::Constant(nodes::Constant {
            owner_id: const_node_id,
            ident: ast_const.ident.ident(),
            ty: ast_const.ty.clone(),
            vis: ast::Visibility::Public,
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

    fn lower_impl(&mut self, im: &ast::Impl, owner_node: OwnerDefId) -> LowerResult<OwnerDefId> {
        let impl_node_id = self.hlir.next_owner_id();
        let impl_item = Item::new(ItemKind::Impl(nodes::Impl {
            owner_id: impl_node_id,
            self_ty: im.target_path.clone(),
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
        owner_node: OwnerDefId,
    ) -> LowerResult<OwnerDefId> {
        let use_node_id = self.hlir.next_owner_id();
        let use_item = Item::new(ItemKind::Use(nodes::UsePath {
            owner_id: use_node_id,
            import_path: useimport.path.clone(),
            resolved_id: None,
        }));
        self.hlir.insert_owning_node_with_parent(
            OwningNode::new(OwningNodeKind::Item(use_item)),
            owner_node,
        );

        Ok(use_node_id)
    }

    fn lower_block(&mut self, block: &ast::Block, owner_node: OwnerDefId) -> LowerResult<HirId> {
        let block_id = self.hlir.next_hir_id_on(owner_node);
        self.hlir.insert_hir_node(
            owner_node,
            HIRNode::Block(nodes::Block {
                id: block_id,
                statements: vec![],
            }),
        );

        let mut statement_node_ids = Vec::new();
        for statement in &block.statements {
            if let Some(statement_id) = self.lower_statement(statement, owner_node)? {
                statement_node_ids.push(statement_id);
            }
        }

        if let HIRNode::Block(block) = self.hlir.get_hir_node_mut_unchecked(block_id) {
            block.statements = statement_node_ids;
        } else {
            panic!("WHAT");
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
            ast::ExpressionKind::IdentPath(ident_path) => Ok(ExprKind::Path(
                exprs::RefPath::Unresolved(ident_path.clone()),
            )),
            ast::ExpressionKind::Continue => Ok(ExprKind::Continue),
            ast::ExpressionKind::Break => Ok(ExprKind::Break),
            ast::ExpressionKind::Return(ret_val) => match ret_val {
                Some(expr) => Ok(ExprKind::Return(Some(
                    self.lower_expression(expr, owner_node)?,
                ))),
                None => Ok(ExprKind::Return(None)),
            },

            ast::ExpressionKind::Block(block) => {
                let block_id = self.lower_block(block, owner_node)?;
                Ok(ExprKind::Block(block_id))
            }
            ast::ExpressionKind::BinaryOp(binary_op_kind, lhs, rhs) => {
                let expr_1 = self.lower_expression(lhs, owner_node)?;
                let expr_2 = self.lower_expression(rhs, owner_node)?;
                Ok(ExprKind::BinaryOp(*binary_op_kind, expr_1, expr_2))
            }
            ast::ExpressionKind::Unary(unary_op_kind, rhs) => {
                let expr_1 = self.lower_expression(rhs, owner_node)?;
                Ok(ExprKind::UnaryOp(*unary_op_kind, expr_1))
            }
            ast::ExpressionKind::If(expression, block, expression1) => {
                let condition = self.lower_expression(expression, owner_node)?;
                let if_block = self.lower_block(block, owner_node)?;
                let else_expr = if let Some(else_e) = expression1 {
                    Some(self.lower_expression(else_e, owner_node)?)
                } else {
                    None
                };
                Ok(ExprKind::If(condition, if_block, else_expr))
            }
            ast::ExpressionKind::While(expression, block) => {
                let condition = self.lower_expression(expression, owner_node)?;
                let while_block = self.lower_block(block, owner_node)?;
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
                    field_inits.push(exprs::StructFieldInit {
                        ident: field.ident.clone(),
                        expr: field_val_expr,
                    });
                }
                Ok(ExprKind::StructInit(exprs::StructInitialisation {
                    ty_path: exprs::RefPath::Unresolved(struct_initialisation.path.clone()),
                    fields: field_inits,
                }))
            }
        }?;

        let expr = Expr {
            id: self.hlir.next_hir_id_on(owner_node),
            kind: expr_kind,
        };

        // FIXME: Probably error if it can't be lowered.
        Ok(self
            .hlir
            .insert_hir_node(owner_node, HIRNode::Expr(expr))
            .unwrap_or(HirId::PLACEHOLDER_ID))
    }

    fn lower_statement(
        &mut self,
        statement: &ast::Statement,
        owner_node: OwnerDefId,
    ) -> LowerResult<Option<HirId>> {
        let lowered = match &statement.kind {
            ast::StatementKind::Let(local) => {
                let init_value = if let ast::LocalKind::Initialise(expr) = &local.kind {
                    Some(self.lower_expression(expr, owner_node)?)
                } else {
                    None
                };

                let statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Let(LetStatement {
                        ident: local.ident.ident.clone(),
                        ty: local.ty.clone(),
                        initial_value: init_value,
                    }),
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(statement))
            }
            ast::StatementKind::Item(item) => {
                let item_id = self.lower_ast_item(item, owner_node)?;
                let statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Item(item_id),
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(statement))
            }
            ast::StatementKind::Expr(expression) => {
                let expr_id = self.lower_expression(expression, owner_node)?;
                let statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Expr(expr_id),
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(statement))
            }
            ast::StatementKind::Semi(expression) => {
                let expr_id = self.lower_expression(expression, owner_node)?;
                let statement = Statement {
                    id: self.hlir.next_hir_id_on(owner_node),
                    kind: StatementKind::Semi(expr_id),
                };
                self.hlir
                    .insert_hir_node(owner_node, HIRNode::Statement(statement))
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
