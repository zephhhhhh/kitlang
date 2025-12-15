use ::std::fmt::{Debug, Display};

use crate::intermediate::hir::{DefId, HirId, LocalDefId, OwnerDefId};

use crate::ast::{
    BinaryOpKind, Ident, IdentPath, Literal, Mutability, SourceSpan, SpannedIdent,
    SpannedIdentPath, Ty as ASTTy, UnaryOpKind, Visibility,
};
use crate::intermediate::resolver::TypeID;
use crate::intermediate::types::KitTy;
use paste::paste;

// Owning nodes..

/// Describes a kind of node that "controls" a scope and owns it's contents.
#[derive(Debug, Clone, PartialEq)]
pub enum OwningNodeKind {
    Item(Item),
    ImplItem(Item),
}

impl OwningNodeKind {
    pub fn ident(&self) -> Option<String> {
        match self {
            OwningNodeKind::Item(item) => item.ident(),
            OwningNodeKind::ImplItem(item) => item.ident(),
        }
    }

    pub fn owner_id(&self) -> OwnerDefId {
        match self {
            OwningNodeKind::Item(item) => item.owner_id,
            OwningNodeKind::ImplItem(item) => item.owner_id,
        }
    }

    pub fn set_owner_id(&mut self, owner_id: OwnerDefId) {
        match self {
            OwningNodeKind::Item(item) => item.owner_id = owner_id,
            OwningNodeKind::ImplItem(item) => item.owner_id = owner_id,
        }
    }

    pub fn item(&self) -> Option<&Item> {
        match self {
            OwningNodeKind::Item(item) | OwningNodeKind::ImplItem(item) => Some(item),
        }
    }

    pub fn item_mut(&mut self) -> Option<&mut Item> {
        match self {
            OwningNodeKind::Item(item) | OwningNodeKind::ImplItem(item) => Some(item),
        }
    }

    pub fn span(&self) -> Option<SourceSpan> {
        match self {
            OwningNodeKind::Item(item) | OwningNodeKind::ImplItem(item) => item.span(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Type {
    Unresolved(ASTTy),
    Resolved(KitTy),
}

impl Type {
    pub fn unit() -> Self {
        Self::Resolved(KitTy::Unit)
    }

    pub fn is_unit(&self) -> bool {
        let Some(resolved) = self.resolved() else {
            return false;
        };
        *resolved == KitTy::Unit
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }

    pub fn is_infer(&self) -> bool {
        matches!(self, Self::Unresolved(ASTTy::Infer))
    }

    pub fn resolved(&self) -> Option<&KitTy> {
        match self {
            Type::Resolved(kit_ty) => Some(kit_ty),
            _ => None,
        }
    }
}

impl Type {
    pub fn from_ast_ty(ty: &ASTTy) -> Self {
        match KitTy::try_from_ast_ty(ty) {
            Some(t) => Self::Resolved(t),
            None => Self::Unresolved(ty.clone()),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unresolved(ty) => write!(f, "Unresolved({:?})", ty),
            Type::Resolved(kit_ty) => {
                if let Some(ty_str) = kit_ty.to_type_str() {
                    write!(f, "{}", ty_str)
                } else {
                    write!(f, "{:?}", kit_ty)
                }
            }
        }
    }
}

impl From<KitTy> for Type {
    fn from(value: KitTy) -> Self {
        Self::Resolved(value)
    }
}

impl From<&KitTy> for Type {
    fn from(value: &KitTy) -> Self {
        Self::Resolved(*value)
    }
}

impl From<crate::ast::Ty> for Type {
    fn from(value: crate::ast::Ty) -> Self {
        Self::Unresolved(value)
    }
}

impl From<&crate::ast::Ty> for Type {
    fn from(value: &crate::ast::Ty) -> Self {
        Self::Unresolved(value.clone())
    }
}

#[derive(Clone, PartialEq)]
pub enum HIRNode {
    Param(Parameter),
    Block(Block),
    Expr(Expr),
    Statement(Statement),
    Field(StructField),
    Path(RefPath),
}

impl HIRNode {
    pub fn span(&self) -> SourceSpan {
        match self {
            HIRNode::Param(parameter) => parameter.span,
            HIRNode::Block(block) => block.span,
            HIRNode::Expr(expr) => expr.span,
            HIRNode::Statement(statement) => statement.span,
            HIRNode::Field(struct_field) => struct_field.span,
            HIRNode::Path(ref_path) => ref_path.span(),
        }
    }
}

impl Debug for HIRNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Param(arg0) => arg0.fmt(f),
            Self::Block(arg0) => arg0.fmt(f),
            Self::Expr(arg0) => arg0.fmt(f),
            Self::Statement(arg0) => arg0.fmt(f),
            Self::Field(arg0) => arg0.fmt(f),
            Self::Path(arg0) => f.debug_tuple("Path").field(&arg0).finish(),
        }
    }
}

macro_rules! impl_hir_node_from {
    ($target:ty, $variant:ident) => {
        impl<'a> From<&'a HIRNode> for Option<&'a $target> {
            fn from(value: &'a HIRNode) -> Self {
                match value {
                    HIRNode::$variant(a) => Some(a),
                    _ => None,
                }
            }
        }

        impl<'a> From<&'a mut HIRNode> for Option<&'a mut $target> {
            fn from(value: &'a mut HIRNode) -> Self {
                match value {
                    HIRNode::$variant(a) => Some(a),
                    _ => None,
                }
            }
        }
    };
}

impl_hir_node_from!(Parameter, Param);
impl_hir_node_from!(Block, Block);
impl_hir_node_from!(Expr, Expr);
impl_hir_node_from!(Statement, Statement);
impl_hir_node_from!(StructField, Field);
impl_hir_node_from!(RefPath, Path);

#[derive(Debug, Clone, PartialEq)]
pub struct OwningNode {
    pub kind: OwningNodeKind,
    pub nodes: Vec<HIRNode>,
}

impl OwningNode {
    pub fn new(kind: OwningNodeKind) -> Self {
        Self {
            kind,
            nodes: Vec::new(),
        }
    }

    pub fn from_item(item: Item, impl_item: bool) -> Self {
        Self::new(if impl_item {
            OwningNodeKind::ImplItem(item)
        } else {
            OwningNodeKind::Item(item)
        })
    }

    pub fn set_owner_id(&mut self, owner_id: OwnerDefId) {
        self.kind.set_owner_id(owner_id);
    }

    pub fn owner_id(&self) -> OwnerDefId {
        self.kind.owner_id()
    }

    pub fn ident(&self) -> Option<String> {
        self.kind.ident()
    }

    pub fn item(&self) -> Option<&Item> {
        self.kind.item()
    }

    pub fn item_mut(&mut self) -> Option<&mut Item> {
        self.kind.item_mut()
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.kind.span()
    }
}

impl OwningNode {
    pub fn local_count(&self) -> u32 {
        self.nodes.len() as u32
    }

    pub fn next_local_id(&self) -> LocalDefId {
        LocalDefId(self.local_count())
    }

    pub fn next_hir_id(&self) -> HirId {
        HirId {
            owner: self.owner_id(),
            id: self.next_local_id(),
        }
    }

    pub fn insert_hir_node(&mut self, hir_node: HIRNode) -> LocalDefId {
        let new_id = self.next_local_id();
        self.nodes.push(hir_node);
        new_id
    }

    pub fn get_hir_node(&self, id: LocalDefId) -> Option<&HIRNode> {
        self.nodes.get(id.0 as usize)
    }

    pub fn get_hir_node_mut(&mut self, id: LocalDefId) -> Option<&mut HIRNode> {
        self.nodes.get_mut(id.0 as usize)
    }
}

macro_rules! impl_item_kind_shorthand {
    (
        $item_name: ident,
        $item_kind: pat_param => $return_expr: expr,
        $return_ty: ty
    ) => {
        paste! {
            impl OwningNode {
                #[inline] pub fn [<hir_ $item_name _mut>](&mut self) -> Option<&mut $return_ty> {
                    let item = match &mut self.kind {
                        OwningNodeKind::Item(item) => item,
                        OwningNodeKind::ImplItem(item) => item,
                    };
                    match &mut item.kind {
                        $item_kind => Some($return_expr),
                        _ => None,
                    }
                }

                #[inline] pub fn [<hir_ $item_name _ref>](&self) -> Option<&$return_ty> {
                    let item = match &self.kind {
                        OwningNodeKind::Item(item) => item,
                        OwningNodeKind::ImplItem(item) => item,
                    };
                    match &item.kind {
                        $item_kind => Some($return_expr),
                        _ => None,
                    }
                }
            }
        }
    };
}

impl_item_kind_shorthand!(module, ItemKind::Module(module) => module, Module);
impl_item_kind_shorthand!(function, ItemKind::Function(f) => f, Function);
impl_item_kind_shorthand!(enum, ItemKind::Enum(e) => e, Enum);
impl_item_kind_shorthand!(struct, ItemKind::Struct(s) => s, Struct);
impl_item_kind_shorthand!(const, ItemKind::Constant(c) => c, Constant);
impl_item_kind_shorthand!(impl, ItemKind::Impl(i) => i, Impl);
impl_item_kind_shorthand!(use, ItemKind::Use(u) => u, UsePath);

// Item kind tys..

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleSpan {
    Declaration(SourceSpan),
    Implementation(SourceSpan),
}

impl ModuleSpan {
    pub fn span(&self) -> SourceSpan {
        match self {
            ModuleSpan::Declaration(source_span) | ModuleSpan::Implementation(source_span) => {
                *source_span
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleIdent {
    RawIdent(Ident),
    SpannedIdent(SpannedIdent),
}

impl ModuleIdent {
    pub fn ident(&self) -> &Ident {
        match self {
            ModuleIdent::RawIdent(ident) => ident,
            ModuleIdent::SpannedIdent(spanned_ident) => &spanned_ident.ident,
        }
    }

    pub fn span(&self) -> SourceSpan {
        match self {
            ModuleIdent::RawIdent(_) => SourceSpan::new(0, 0),
            ModuleIdent::SpannedIdent(spanned_ident) => spanned_ident.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub owner_id: OwnerDefId,
    pub ident: ModuleIdent,
    pub span: ModuleSpan,
    pub vis: Visibility,
    pub item_ids: Vec<OwnerDefId>,
}

/// An individual parameter to a function.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Parameter {
    pub id: HirId,
    pub fn_id: OwnerDefId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub mutable: Mutability,
}

impl Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parameter {{ id: {:?}, name: {:?}, mutable: {:?} }}",
            self.id, self.ident, self.mutable
        )
    }
}

/// The "signature" of a function, holds the return type and the parameter types.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FunctionSig {
    pub parameters: Vec<Type>,
    pub output: Type,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionBody {
    pub params: Vec<HirId>,
    pub block: HirId,
}

#[derive(Clone, PartialEq)]
pub struct Function {
    pub owner_id: OwnerDefId,
    pub ident: SpannedIdent,
    pub vis: Visibility,
    pub native: bool,
    pub is_method: bool,
    pub is_global: bool,
    pub sig: FunctionSig,
    pub decl_span: SourceSpan,
    pub full_span: SourceSpan,
    pub body: Option<FunctionBody>,
}

impl Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("ident", &self.ident.str())
            .field("is_method", &self.is_method)
            .field("sig", &self.sig)
            .field("vis", &self.vis)
            .field("body", &self.body)
            .finish()
    }
}

/// A field of a struct, containing the name of the field and it's associated [`Type`].
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub id: HirId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub ty: Type,
    pub vis: Visibility,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub owner_id: OwnerDefId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub vis: Visibility,
    pub fields: Vec<HirId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub owner_id: OwnerDefId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub vis: Visibility,
    // TODO: Variants..
}

/// A declaration of a constant, with the expression it should be evaluated to.
#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    pub owner_id: OwnerDefId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub ty: Type,
    pub vis: Visibility,
    pub expr: HirId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    pub span: SourceSpan,
    pub owner_id: OwnerDefId,
    // FIXME: Make this the hir::Ty.
    pub ty_span: SourceSpan,
    pub self_ty: IdentPath,
    pub items: Vec<OwnerDefId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsePath {
    pub owner_id: OwnerDefId,
    pub imports: Vec<IdentPath>,
    pub span: SourceSpan,
    pub resolved_id: Option<ResolvedID>,
    pub vis: Visibility,
}

// Item implementation..

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Module(Module),
    Function(Function),
    Struct(Struct),
    Enum(Enum),
    Constant(Constant),
    Impl(Impl),
    Use(UsePath),
}

impl ItemKind {
    pub fn ident(&self) -> Option<String> {
        match self {
            ItemKind::Module(md) => Some(md.ident.ident().string()),
            ItemKind::Function(f) => Some(f.ident.string()),
            ItemKind::Struct(s) => Some(s.ident.string()),
            ItemKind::Enum(e) => Some(e.ident.string()),
            ItemKind::Constant(c) => Some(c.ident.string()),
            ItemKind::Impl(_) | ItemKind::Use(_) => None,
        }
    }

    pub fn span(&self) -> Option<SourceSpan> {
        match self {
            ItemKind::Module(md) => Some(md.ident.span()),
            ItemKind::Function(f) => Some(f.decl_span),
            ItemKind::Struct(s) => Some(s.ident.span),
            ItemKind::Enum(e) => Some(e.ident.span),
            ItemKind::Constant(c) => Some(c.ident.span),
            ItemKind::Impl(_) => None,
            ItemKind::Use(u) => Some(u.span),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub owner_id: OwnerDefId,
    pub kind: ItemKind,
}

impl Item {
    pub fn new(kind: ItemKind) -> Self {
        Self {
            owner_id: OwnerDefId::PLACEHOLDER_ID,
            kind,
        }
    }

    pub fn ident(&self) -> Option<String> {
        self.kind.ident()
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.kind.span()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: HirId,
    pub statements: Vec<HirId>,
    pub root_block: bool,
    pub span: SourceSpan,
}

// Statement..

#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    Let(LetStatement),
    Item(OwnerDefId),
    Expr(HirId),
    Semi(HirId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub id: HirId,
    pub kind: StatementKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetStatement {
    pub ident: Ident,
    pub mutable: Mutability,
    pub ty: Type, // TODO: Change this.
    pub initial_value: Option<HirId>,
}

// Exprs..

#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldInit {
    pub ident: Ident,
    pub expr: HirId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructInitialisation {
    pub ty_path: RefPath,
    pub fields: Vec<StructFieldInit>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Block(HIRNode::Block).
    Block(HirId),
    /// Literal(Literal)
    Literal(Literal),
    /// Binary Operation(Kind, lhs: HIRNode::Expr, rhs: HIRNode::Expr)
    BinaryOp(BinaryOpKind, HirId, HirId),
    /// Unary Operation(Kind, HIRNode::Expr)
    UnaryOp(UnaryOpKind, HirId),
    /// If(condition: HIRNode::Expr, true_block: HIRNode::Block, else: HIRNode::Expr)
    If(HirId, HirId, Option<HirId>),
    /// While(condition: HIRNode::Expr, block: HIRNode::Block)
    While(HirId, HirId),
    /// Assign(target: HIRNode::Expr, value: HIRNode::Expr)
    Assign(HirId, HirId),
    /// Call(target: HIRNode::Expr, args: Vec<HIRNode::Expr>)
    Call(HirId, Vec<HirId>),
    /// Method Call(target: HIRNode::Expr, method_name: Ident, args: Vec<HIRNode::Expr>)
    MethodCall(HirId, Ident, Vec<HirId>),
    /// Index(target: HIRNode::Expr, index: HIRNode::Expr)
    Index(HirId, HirId),
    /// Field Access(target: HIRNode::Expr, field_name: Ident)
    FieldAccess(HirId, Ident),
    /// Struct Initialisation
    StructInit(StructInitialisation),
    /// Path
    Path(RefPath),
    /// Continue to next loop iteration
    Continue,
    /// Break from loop
    Break,
    /// Return from function(Option<HIRNode::Expr>)
    Return(Option<HirId>),
    /// Cast an expression from one type to another.
    Cast(HirId, Type),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub id: HirId,
    pub kind: ExprKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedID {
    Hir(HirId),
    Def(DefId),
    OwnerDef(OwnerDefId),
    TypeDef(TypeID),
}

impl ResolvedID {
    pub fn hir_id(&self) -> Option<HirId> {
        match self {
            ResolvedID::Hir(hir_id) => Some(*hir_id),
            _ => None,
        }
    }

    pub fn def_id(&self) -> Option<DefId> {
        match self {
            ResolvedID::Def(def_id) => Some(*def_id),
            _ => None,
        }
    }

    pub fn owner_def_id(&self) -> Option<OwnerDefId> {
        match self {
            ResolvedID::OwnerDef(owner_def_id) => Some(*owner_def_id),
            _ => None,
        }
    }

    pub fn type_id(&self) -> Option<TypeID> {
        match self {
            ResolvedID::TypeDef(type_id) => Some(*type_id),
            _ => None,
        }
    }
}

impl From<HirId> for ResolvedID {
    fn from(value: HirId) -> Self {
        Self::Hir(value)
    }
}

impl From<&HirId> for ResolvedID {
    fn from(value: &HirId) -> Self {
        Self::Hir(*value)
    }
}

impl From<DefId> for ResolvedID {
    fn from(value: DefId) -> Self {
        Self::Def(value)
    }
}

impl From<&DefId> for ResolvedID {
    fn from(value: &DefId) -> Self {
        Self::Def(*value)
    }
}

impl From<OwnerDefId> for ResolvedID {
    fn from(value: OwnerDefId) -> Self {
        Self::OwnerDef(value)
    }
}

impl From<&OwnerDefId> for ResolvedID {
    fn from(value: &OwnerDefId) -> Self {
        Self::OwnerDef(*value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefPath {
    Unresolved(SpannedIdentPath),
    Resolved(SpannedIdentPath, ResolvedID),
}

impl RefPath {
    pub fn spanned_ident_path(&self) -> &SpannedIdentPath {
        match self {
            RefPath::Unresolved(ident_path) => ident_path,
            RefPath::Resolved(ident_path, _) => ident_path,
        }
    }

    pub fn ident_path(&self) -> &IdentPath {
        match self {
            RefPath::Unresolved(ident_path) => &ident_path.path,
            RefPath::Resolved(ident_path, _) => &ident_path.path,
        }
    }

    pub fn span(&self) -> SourceSpan {
        match self {
            RefPath::Unresolved(ident_path) => ident_path.span,
            RefPath::Resolved(ident_path, _) => ident_path.span,
        }
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, RefPath::Resolved(_, _))
    }

    pub fn resolved_id(&self) -> Option<ResolvedID> {
        match self {
            RefPath::Unresolved(_) => None,
            RefPath::Resolved(_, id) => Some(*id),
        }
    }

    pub fn resolve_to(&mut self, id: ResolvedID) {
        *self = RefPath::Resolved(self.spanned_ident_path().clone(), id);
    }

    pub fn resolve_to_hir_id(&mut self, id: HirId) {
        *self = RefPath::Resolved(self.spanned_ident_path().clone(), ResolvedID::Hir(id));
    }

    pub fn resolve_to_owner_id(&mut self, id: OwnerDefId) {
        *self = RefPath::Resolved(self.spanned_ident_path().clone(), ResolvedID::OwnerDef(id));
    }

    pub fn resolve_to_def_id(&mut self, id: DefId) {
        *self = RefPath::Resolved(self.spanned_ident_path().clone(), ResolvedID::Def(id));
    }
}
