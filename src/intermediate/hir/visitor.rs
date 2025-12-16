use crate::intermediate::hir::nodes::{
    Block, Constant, Enum, Expr, ExprKind, Function, HIRNode, Impl, Item, ItemKind, LetStatement,
    Module, OwningNode, Parameter, RefPath, Statement, StatementKind, Struct, UsePath,
};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

/// Provides an interface from traversing the HLIR tree.
///
/// Functions named `super_` are default implementations that traverse the tree further, and
/// are not meant to be overriden. They should be called during the corresponding `visit_`
/// function implementations to keep traversing the tree further, you can also omit this under conditions
/// if you do not want traversal to continue.
///
/// Implementations should be provided by overriding the `visit_` functions for the specified types
/// you want.
///
/// To start traversal, the function `visit_root` should be called on this trait with a reference
/// to the [`HLIR`].
pub trait HLIRVisitor {
    fn super_root(&mut self, hlir: &HLIR) {
        if let Some(root_node) = hlir.owning_node(OwnerDefId::ROOT_NODE)
            && let Some(root_module) = root_node.hir_module_ref()
        {
            self.visit_module(root_module, hlir);
        }
    }
    fn super_module(&mut self, module: &Module, hlir: &HLIR) {
        for item_id in &module.item_ids {
            if let Some(item) = hlir.owning_node_item(*item_id) {
                self.visit_item(item, hlir);
            }
        }
    }
    fn super_item(&mut self, item: &Item, hlir: &HLIR) {
        match &item.kind {
            ItemKind::Module(module) => self.visit_module(module, hlir),
            ItemKind::Function(function) => self.visit_function(function, hlir),
            ItemKind::Struct(structure) => self.visit_struct(structure, hlir),
            ItemKind::Enum(enumeration) => self.visit_enum(enumeration, hlir),
            ItemKind::Constant(constant) => self.visit_constant(constant, hlir),
            ItemKind::Impl(impl_info) => self.visit_impl(impl_info, hlir),
            ItemKind::Use(use_path) => self.visit_use(use_path, hlir),
        }
    }
    fn super_impl_item(&mut self, item: &Item, hlir: &HLIR) {
        self.super_item(item, hlir);
    }
    fn super_use(&mut self, _use_info: &UsePath, _hlir: &HLIR) {}
    fn super_impl(&mut self, impl_info: &Impl, hlir: &HLIR) {
        for item_id in &impl_info.items {
            if let Some(item) = hlir.owning_node_item(*item_id) {
                self.visit_impl_item(item, hlir);
            }
        }
    }
    fn super_function(&mut self, function: &Function, hlir: &HLIR) {
        if let Some(func_body) = &function.body {
            for param_id in &func_body.params {
                if let Some(HIRNode::Param(param)) = hlir.get_hir_node(*param_id) {
                    self.visit_function_param(param, hlir);
                }
            }
            if let Some(HIRNode::Block(func_block)) = hlir.get_hir_node(func_body.block) {
                self.visit_block(func_block, hlir);
            }
        }
    }
    fn super_function_param(&mut self, _parameter: &Parameter, _hlir: &HLIR) {}
    fn super_enum(&mut self, _enumeration: &Enum, _hlir: &HLIR) {}
    fn super_struct(&mut self, _structure: &Struct, _hlir: &HLIR) {}
    fn super_block(&mut self, block: &Block, hlir: &HLIR) {
        for statement_id in &block.statements {
            if let Some(HIRNode::Statement(statement)) = hlir.get_hir_node(*statement_id) {
                self.visit_statement(statement, block, hlir);
            }
        }
    }
    fn super_statement(&mut self, statement: &Statement, _parent_block: &Block, hlir: &HLIR) {
        match &statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement(statement.id, let_statement, hlir)
            }
            StatementKind::Item(owner_def_id) => {
                if let Some(item) = hlir.owning_node_item(*owner_def_id) {
                    self.visit_item(item, hlir);
                }
            }
            StatementKind::Expr(hir_id) | StatementKind::Semi(hir_id) => {
                if let Some(HIRNode::Expr(expr)) = hlir.get_hir_node(*hir_id) {
                    self.visit_expr(expr, hlir);
                }
            }
        }
    }
    fn super_let_statement(&mut self, _id: HirId, let_statement: &LetStatement, hlir: &HLIR) {
        if let Some(init_id) = &let_statement.initial_value {
            self.visit_expr_by_id(*init_id, hlir);
        }
    }
    fn super_constant(&mut self, constant: &Constant, hlir: &HLIR) {
        self.visit_expr_by_id(constant.expr, hlir);
    }
    fn super_expr(&mut self, expr: &Expr, hlir: &HLIR) {
        match &expr.kind {
            ExprKind::Path(ref_path) => self.visit_path(expr.id, ref_path, hlir),
            ExprKind::Block(hir_id) => {
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node(*hir_id) {
                    self.visit_block(block, hlir);
                }
            }
            ExprKind::BinaryOp(_, hir_id, hir_id1) => {
                self.visit_expr_by_id(*hir_id, hlir);
                self.visit_expr_by_id(*hir_id1, hlir);
            }
            ExprKind::UnaryOp(_, hir_id) => self.visit_expr_by_id(*hir_id, hlir),
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                self.visit_expr_by_id(*hir_id, hlir);
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node(*hir_id1) {
                    self.visit_block(block, hlir);
                    if let Some(else_id) = hir_id2 {
                        self.visit_expr_by_id(*else_id, hlir);
                    }
                }
            }
            ExprKind::Loop(hir_id) => {
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node(*hir_id) {
                    self.visit_block(block, hlir);
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                self.visit_expr_by_id(*hir_id, hlir);
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node(*hir_id1) {
                    self.visit_block(block, hlir);
                }
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                self.visit_expr_by_id(*hir_id, hlir);
                self.visit_expr_by_id(*hir_id1, hlir);
            }
            ExprKind::Call(hir_id, hir_ids) => {
                self.visit_expr_by_id(*hir_id, hlir);
                for param_id in hir_ids {
                    self.visit_expr_by_id(*param_id, hlir);
                }
            }
            ExprKind::MethodCall(hir_id, _, hir_ids) => {
                self.visit_expr_by_id(*hir_id, hlir);
                for param_id in hir_ids {
                    self.visit_expr_by_id(*param_id, hlir);
                }
            }
            ExprKind::Index(hir_id, hir_id1) => {
                self.visit_expr_by_id(*hir_id, hlir);
                self.visit_expr_by_id(*hir_id1, hlir);
            }
            ExprKind::FieldAccess(hir_id, _) => self.visit_expr_by_id(*hir_id, hlir),
            ExprKind::StructInit(struct_initialisation) => {
                for field in &struct_initialisation.fields {
                    self.visit_expr_by_id(field.expr, hlir);
                }
            }
            ExprKind::Return(Some(hir_id)) => {
                self.visit_expr_by_id(*hir_id, hlir);
            }
            ExprKind::Cast(hir_id, _) => {
                self.visit_expr_by_id(*hir_id, hlir);
            }
            _ => {}
        }
    }
    fn super_path(&mut self, _hir_id: HirId, _path: &RefPath, _hlir: &HLIR) {}

    fn visit_expr_by_id(&mut self, expr_id: HirId, hlir: &HLIR) {
        if let Some(HIRNode::Expr(expr)) = hlir.get_hir_node(expr_id) {
            self.visit_expr(expr, hlir);
        }
    }
    fn visit_block_by_id(&mut self, block_id: HirId, hlir: &HLIR) {
        if let Some(HIRNode::Block(block)) = hlir.get_hir_node(block_id) {
            self.visit_block(block, hlir);
        }
    }

    fn visit_path(&mut self, hir_id: HirId, path: &RefPath, hlir: &HLIR) {
        self.super_path(hir_id, path, hlir)
    }
    fn visit_expr(&mut self, expr: &Expr, hlir: &HLIR) {
        self.super_expr(expr, hlir)
    }
    fn visit_constant(&mut self, constant: &Constant, hlir: &HLIR) {
        self.super_constant(constant, hlir);
    }
    fn visit_let_statement(&mut self, id: HirId, let_statement: &LetStatement, hlir: &HLIR) {
        self.super_let_statement(id, let_statement, hlir);
    }
    fn visit_statement(&mut self, statement: &Statement, parent_block: &Block, hlir: &HLIR) {
        self.super_statement(statement, parent_block, hlir)
    }
    fn visit_block(&mut self, block: &Block, hlir: &HLIR) {
        self.super_block(block, hlir)
    }
    fn visit_struct(&mut self, structure: &Struct, hlir: &HLIR) {
        self.super_struct(structure, hlir)
    }
    fn visit_enum(&mut self, enumeration: &Enum, hlir: &HLIR) {
        self.super_enum(enumeration, hlir)
    }
    fn visit_function_param(&mut self, parameter: &Parameter, hlir: &HLIR) {
        self.super_function_param(parameter, hlir)
    }
    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        self.super_function(function, hlir)
    }
    fn visit_impl(&mut self, impl_info: &Impl, hlir: &HLIR) {
        self.super_impl(impl_info, hlir)
    }
    fn visit_use(&mut self, use_info: &UsePath, hlir: &HLIR) {
        self.super_use(use_info, hlir)
    }
    fn visit_item(&mut self, item: &Item, hlir: &HLIR) {
        self.super_item(item, hlir)
    }
    fn visit_impl_item(&mut self, item: &Item, hlir: &HLIR) {
        self.super_impl_item(item, hlir)
    }
    fn visit_module(&mut self, module: &Module, hlir: &HLIR) {
        self.super_module(module, hlir)
    }
    fn visit_root(&mut self, hlir: &HLIR) {
        self.super_root(hlir)
    }
}

/// The purpose of this is to allow multiple mutable references to the individual disjoint nodes in
/// the HLIR representation.
/// # Safety
/// This is safe as long as no new nodes are added in any form, so we only mutate already existing
/// nodes, and no references are stored beyond the lifetime of 'a.
///
/// Therefore this struct offers no way to access the underlying HLIR directly, and only exposes
/// methods for accessing already existing nodes.
///
/// Since we borrow [`HLIR`] mutably for this, we can guarantee that we are the only reference to
/// the [`HLIR`] and so therefore this is no way for new nodes to be added, guaranteeing the safety
/// of this.
pub struct HLIRDisjointMut<'a> {
    hlir: &'a mut HLIR,
}

impl<'a> HLIRDisjointMut<'a> {
    pub fn new(hlir: &'a mut HLIR) -> Self {
        Self { hlir }
    }

    pub fn nonmut_ref<'b>(&'a self) -> &'b HLIR
    where
        'a: 'b,
    {
        self.hlir
    }
}

impl HLIRDisjointMut<'_> {
    pub fn get_hir_node_mut_as<'a, F>(&mut self, id: HirId) -> Option<&'a mut F>
    where
        Option<&'a mut F>: From<&'a mut HIRNode>,
    {
        self.get_hir_node_mut(id).and_then(|node| node.into())
    }

    pub fn get_hir_node_mut(&mut self, id: HirId) -> Option<&'static mut HIRNode> {
        Some(self.hir_node_mut(id)?.value_mut())
    }

    pub fn hir_node_mut(&mut self, id: HirId) -> Option<DisjointHIRNode> {
        Some(DisjointHIRNode::from_mut_ref(
            self.hlir.get_hir_node_mut(id)?,
        ))
    }

    pub fn owning_node_mut(&mut self, id: OwnerDefId) -> Option<DisjointOwningNode> {
        Some(DisjointOwningNode::from_mut_ref(
            self.hlir.owning_node_mut(id)?,
        ))
    }

    pub fn get_owning_node_mut(&mut self, id: OwnerDefId) -> Option<&'static mut OwningNode> {
        Some(self.owning_node_mut(id)?.value_mut())
    }

    pub fn owning_node_item_mut(&mut self, id: OwnerDefId) -> Option<DisjointItem> {
        Some(DisjointItem::from_mut_ref(
            self.hlir.owning_node_item_mut(id)?,
        ))
    }
}

impl AsRef<HLIR> for HLIRDisjointMut<'_> {
    fn as_ref(&self) -> &HLIR {
        self.nonmut_ref()
    }
}

/// This is a wrapper around a pointer, this purpose of this is to allow methods on
/// [`HLIRDisjointMut`] to return a value, and not a reference with a lifetime, this means the
/// mutable borrow of [`HLIRDisjointMut`] when querying is only held for the duration of the
/// function call, and isn't held afterwards, preventing more mutable borrows from being made.
#[derive(Clone)]
pub struct Disjoint<T> {
    v: *mut T,
}

impl<T> std::ops::Deref for Disjoint<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.v }
    }
}

impl<T> std::ops::DerefMut for Disjoint<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.v }
    }
}

impl<T> Disjoint<T> {
    pub fn from_mut_ref(v: &mut T) -> Self {
        Self {
            v: std::ptr::from_mut(v),
        }
    }

    pub fn value(&self) -> &'static T {
        unsafe { &*self.v }
    }

    pub fn value_mut(&self) -> &'static mut T {
        unsafe { &mut *self.v }
    }
}

pub type DisjointHIRNode = Disjoint<HIRNode>;
pub type DisjointOwningNode = Disjoint<OwningNode>;
pub type DisjointItem = Disjoint<Item>;

/// Provides an interface from traversing the HLIR tree with mutable access to each type.
///
/// Functions named `super_` are default implementations and are not meant to be overriden.
/// They should be called during the corresponding `visit_` function implementations to keep
/// traversing the tree further, you can also omit this under conditions if you do not want
/// traversal to continue.
///
/// Implementations should be provided by overriding the `visit_` functions for the specified types
/// you want.
///
/// To start traversal, the function `walk_mut` should be called on this trait with a mutable reference
/// to the [`HLIR`].
/// # Safety
/// This is safe as long as implementations do _NOT_ store the references obtained in the functions
/// and are only used within the scope of the trait function bodies.
pub trait HLIRVisitorMut<'a> {
    fn super_root_mut(&mut self, hlir: &mut HLIRDisjointMut<'a>) {
        if let Some(root_node) = hlir.owning_node_mut(OwnerDefId::ROOT_NODE)
            && let Some(root_module) = root_node.value_mut().hir_module_mut()
        {
            self.visit_module_mut(root_module, hlir);
        }
    }
    fn super_module_mut(&mut self, module: &mut Module, hlir: &mut HLIRDisjointMut<'a>) {
        for item_id in &module.item_ids {
            if let Some(item) = hlir.owning_node_item_mut(*item_id) {
                self.visit_item_mut(item.value_mut(), hlir);
            }
        }
    }
    fn super_item_mut(&mut self, item: &mut Item, hlir: &mut HLIRDisjointMut<'a>) {
        match &mut item.kind {
            ItemKind::Module(module) => self.visit_module_mut(module, hlir),
            ItemKind::Function(function) => self.visit_function_mut(function, hlir),
            ItemKind::Struct(structure) => self.visit_struct_mut(structure, hlir),
            ItemKind::Enum(enumeration) => self.visit_enum_mut(enumeration, hlir),
            ItemKind::Constant(constant) => self.visit_constant_mut(constant, hlir),
            ItemKind::Impl(impl_info) => self.visit_impl_mut(impl_info, hlir),
            ItemKind::Use(use_path) => self.visit_use_mut(use_path, hlir),
        }
    }
    fn super_impl_item_mut(&mut self, item: &mut Item, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_item_mut(item, hlir)
    }
    fn super_use_mut(&mut self, _use_info: &mut UsePath, _hlir: &mut HLIRDisjointMut<'a>) {}
    fn super_impl_mut(&mut self, impl_info: &mut Impl, hlir: &mut HLIRDisjointMut<'a>) {
        for item_id in &impl_info.items {
            if let Some(item) = hlir.owning_node_item_mut(*item_id) {
                self.visit_impl_item_mut(item.value_mut(), hlir);
            }
        }
    }
    fn super_function_param_mut(
        &mut self,
        _parameter: &mut Parameter,
        _hlir: &mut HLIRDisjointMut<'a>,
    ) {
    }
    fn super_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'a>) {
        if let Some(func_body) = &function.body {
            for param_id in &func_body.params {
                if let Some(HIRNode::Param(param)) = hlir.get_hir_node_mut(*param_id) {
                    self.visit_function_param_mut(param, hlir);
                }
            }
            if let Some(HIRNode::Block(block)) = hlir.get_hir_node_mut(func_body.block) {
                self.visit_block_mut(block, hlir);
            }
        }
    }
    fn super_enum_mut(&mut self, _enumeration: &mut Enum, _hlir: &mut HLIRDisjointMut<'a>) {}
    fn super_struct_mut(&mut self, _structure: &Struct, _hlir: &mut HLIRDisjointMut<'a>) {}
    fn super_block_mut(&mut self, block: &Block, hlir: &mut HLIRDisjointMut<'a>) {
        for statement_id in &block.statements {
            if let Some(HIRNode::Statement(statement)) = hlir.get_hir_node_mut(*statement_id) {
                self.visit_statement_mut(statement, block, hlir);
            }
        }
    }
    fn super_statement_mut(
        &mut self,
        statement: &mut Statement,
        _parent_block: &Block,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        match &mut statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement_mut(statement.id, let_statement, hlir)
            }
            StatementKind::Item(owner_def_id) => {
                if let Some(item) = hlir.owning_node_item_mut(*owner_def_id) {
                    self.visit_item_mut(item.value_mut(), hlir);
                }
            }
            StatementKind::Expr(hir_id) | StatementKind::Semi(hir_id) => {
                if let Some(HIRNode::Expr(expr)) = hlir.get_hir_node_mut(*hir_id) {
                    self.visit_expr_mut(expr, hlir);
                }
            }
        }
    }
    fn super_let_statement_mut(
        &mut self,
        _id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        if let Some(init_id) = &let_statement.initial_value {
            self.visit_expr_by_id_mut(*init_id, hlir);
        }
    }
    fn super_constant_mut(&mut self, constant: &mut Constant, hlir: &mut HLIRDisjointMut<'a>) {
        self.visit_expr_by_id_mut(constant.expr, hlir);
    }
    fn super_expr_mut(&mut self, expr: &mut Expr, hlir: &mut HLIRDisjointMut<'a>) {
        match &mut expr.kind {
            ExprKind::Path(ref_path) => self.visit_path_mut(expr.id, ref_path, hlir),
            ExprKind::Block(hir_id) => {
                if let Some(node) = hlir.hir_node_mut(*hir_id)
                    && let HIRNode::Block(block) = node.value_mut()
                {
                    self.visit_block_mut(block, hlir);
                }
            }
            ExprKind::BinaryOp(_, hir_id, hir_id1) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                self.visit_expr_by_id_mut(*hir_id1, hlir);
            }
            ExprKind::UnaryOp(_, hir_id) => self.visit_expr_by_id_mut(*hir_id, hlir),
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node_mut(*hir_id1) {
                    self.visit_block_mut(block, hlir);
                    if let Some(else_id) = hir_id2 {
                        self.visit_expr_by_id_mut(*else_id, hlir);
                    }
                }
            }
            ExprKind::Loop(hir_id) => {
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node_mut(*hir_id) {
                    self.visit_block_mut(block, hlir);
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                if let Some(HIRNode::Block(block)) = hlir.get_hir_node_mut(*hir_id1) {
                    self.visit_block_mut(block, hlir);
                }
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                self.visit_expr_by_id_mut(*hir_id1, hlir);
            }
            ExprKind::Call(hir_id, hir_ids) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                for param_id in hir_ids {
                    self.visit_expr_by_id_mut(*param_id, hlir);
                }
            }
            ExprKind::MethodCall(hir_id, _, hir_ids) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                for param_id in hir_ids {
                    self.visit_expr_by_id_mut(*param_id, hlir);
                }
            }
            ExprKind::Index(hir_id, hir_id1) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
                self.visit_expr_by_id_mut(*hir_id1, hlir);
            }
            ExprKind::FieldAccess(hir_id, _) => self.visit_expr_by_id_mut(*hir_id, hlir),
            ExprKind::StructInit(struct_initialisation) => {
                for field in &struct_initialisation.fields {
                    self.visit_expr_by_id_mut(field.expr, hlir);
                }
            }
            ExprKind::Return(Some(hir_id)) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
            }
            ExprKind::Cast(hir_id, _) => {
                self.visit_expr_by_id_mut(*hir_id, hlir);
            }
            _ => {}
        }
    }
    fn super_path_mut(&mut self, _id: HirId, _path: &mut RefPath, _hlir: &mut HLIRDisjointMut<'a>) {
    }

    fn visit_expr_by_id_mut(&mut self, expr_id: HirId, hlir: &mut HLIRDisjointMut<'a>) {
        if let Some(HIRNode::Expr(expr)) = hlir.get_hir_node_mut(expr_id) {
            self.visit_expr_mut(expr, hlir);
        }
    }

    fn visit_path_mut(
        &mut self,
        hir_id: HirId,
        path: &mut RefPath,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        self.super_path_mut(hir_id, path, hlir)
    }
    fn visit_expr_mut(&mut self, expr: &mut Expr, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_expr_mut(expr, hlir)
    }
    fn visit_constant_mut(&mut self, constant: &mut Constant, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_constant_mut(constant, hlir);
    }
    fn visit_let_statement_mut(
        &mut self,
        id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        self.super_let_statement_mut(id, let_statement, hlir);
    }
    fn visit_statement_mut(
        &mut self,
        statement: &mut Statement,
        parent_block: &Block,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        self.super_statement_mut(statement, parent_block, hlir)
    }
    fn visit_block_mut(&mut self, block: &mut Block, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_block_mut(block, hlir)
    }
    fn visit_struct_mut(&mut self, structure: &mut Struct, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_struct_mut(structure, hlir)
    }
    fn visit_enum_mut(&mut self, enumeration: &mut Enum, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_enum_mut(enumeration, hlir)
    }
    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_function_mut(function, hlir)
    }
    fn visit_function_param_mut(
        &mut self,
        parameter: &mut Parameter,
        hlir: &mut HLIRDisjointMut<'a>,
    ) {
        self.super_function_param_mut(parameter, hlir)
    }
    fn visit_impl_mut(&mut self, impl_info: &mut Impl, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_impl_mut(impl_info, hlir)
    }
    fn visit_use_mut(&mut self, use_info: &mut UsePath, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_use_mut(use_info, hlir)
    }
    fn visit_item_mut(&mut self, item: &mut Item, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_item_mut(item, hlir)
    }
    fn visit_impl_item_mut(&mut self, item: &mut Item, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_impl_item_mut(item, hlir)
    }
    fn visit_module_mut(&mut self, module: &mut Module, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_module_mut(module, hlir)
    }
    fn visit_root_mut(&mut self, hlir: &mut HLIRDisjointMut<'a>) {
        self.super_root_mut(hlir)
    }

    fn walk_mut(&mut self, hlir: &'a mut HLIR) {
        let mut disjoint_hlir = HLIRDisjointMut::new(hlir);
        self.visit_root_mut(&mut disjoint_hlir)
    }
}
