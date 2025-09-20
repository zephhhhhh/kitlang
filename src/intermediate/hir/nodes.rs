use super::{
    HirId, LocalDefId, OwnerDefId,
    exprs::{Expr, RefPath, ResolvedID},
    statements::Statement,
};
use crate::ast::{self, Ident, IdentPath, SourceSpan, SpannedIdent, SpannedIdentPath, Visibility};
use paste::paste;

// FIXME: Create HIR::Ty type.

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

#[derive(Clone, PartialEq)]
pub enum HIRNode {
    Param(Parameter),
    Block(Block),
    Expr(Expr),
    Statement(Statement),
    Field(StructField),
    Path(RefPath),
}

impl ::std::fmt::Debug for HIRNode {
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

/// An individual parameter to a function, includes the name `Identifier`, the [`Ty`] of the
/// parameter, as well as if it is declared as `mutable` or not.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Parameter {
    pub id: HirId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub mutable: ast::Mutability,
}

impl Parameter {
    #[inline]
    pub fn new(id: HirId, ident: SpannedIdent, span: SourceSpan, mutable: ast::Mutability) -> Self {
        Self {
            id,
            ident,
            span,
            mutable,
        }
    }
}

impl ::std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parameter {{ id: {:?}, name: {:?}, mutable: {:?} }}",
            self.id, self.ident, self.mutable
        )
    }
}

/// The return type of a function. If not specified this will default to
/// `FunctionReturnTy::Default` which is the unit type `()`, otherwise this holds the specified
/// return type.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum FunctionReturnTy {
    /// No return type specified.
    Default,
    /// Specified return type.
    Ty(Box<ast::Ty>),
}

/// The "signature" of a function, holds the return type and the parameter types.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FunctionSig {
    pub parameters: Vec<ast::Ty>,
    pub output: FunctionReturnTy,
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
    pub sig: FunctionSig,
    pub decl_span: SourceSpan,
    pub full_span: SourceSpan,
    pub body: Option<FunctionBody>,
}

impl ::std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("ident", &self.ident.str())
            .field("sig", &self.sig)
            .field("vis", &self.vis)
            .field("body", &self.body)
            .finish()
    }
}

/// A field of a struct, containing the name of the field and it's associated [`Ty`].
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub id: HirId,
    pub ident: SpannedIdent,
    pub span: SourceSpan,
    pub ty: ast::Ty,
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
    pub ty: ast::Ty,
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
    pub import_path: SpannedIdentPath,
    pub span: SourceSpan,
    pub resolved_id: Option<ResolvedID>,
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
    pub span: SourceSpan,
}
