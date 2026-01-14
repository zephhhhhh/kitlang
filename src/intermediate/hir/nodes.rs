use ::std::fmt::{Debug, Display};

use crate::KitSmallVec;
use crate::intermediate::hir::{DefId, HLIRExt, HirId, LocalDefId, OwnerDefId};

use crate::ast::{
    BinaryOpKind, Ident, IdentPath, Literal, Mutability, SourceSpan, SpannedIdent,
    SpannedIdentPath, Ty as ASTTy, UnaryOpKind, Visibility,
};
use crate::intermediate::resolver::TypeID;
use crate::intermediate::types::KitTy;

use itertools::Itertools;
use paste::paste;

// Owning nodes..

/// Describes a kind of node that "controls" a scope and owns it's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwningNodeKind {
    /// An item node, such as a function, struct, or enum.
    Item(Item),
    /// The same as `OwningNodeKind::Item`, but for items defined inside an `impl` block.
    ImplItem(Item),
}

impl OwningNodeKind {
    /// Get the identifier of the item in the node, if it has one.
    #[inline]
    #[must_use]
    pub fn ident(&self) -> Option<String> {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item.ident(),
        }
    }

    /// Get the owner definition ID of the item in the node.
    #[inline]
    #[must_use]
    pub const fn owner_id(&self) -> OwnerDefId {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item.owner_id,
        }
    }

    /// Set the owner definition ID of the item in the node.
    #[inline]
    pub const fn set_owner_id(&mut self, owner_id: OwnerDefId) {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item.owner_id = owner_id,
        }
    }

    /// Get a reference to the item in the node.
    #[inline]
    #[must_use]
    pub const fn item(&self) -> &Item {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item,
        }
    }

    /// Get a mutable reference to the item in the node.
    #[inline]
    #[must_use]
    pub const fn item_mut(&mut self) -> &mut Item {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item,
        }
    }

    /// Get the source span of the item in the node.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Item(item) | Self::ImplItem(item) => item.span(),
        }
    }
}

/// The type of a value in HIR, either unresolved (AST type) or resolved (Kit type).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub enum Type {
    /// The type has not yet been resolved to it's Kit type (either a `TypeID` or a built-in type).
    Unresolved(ASTTy),
    /// The type has been resolved to it's Kit type (either a `TypeID` or a built-in type).
    Resolved(KitTy),
}

impl Type {
    /// Returns [`Type::Unresolved`] if the type is `Unresolved`, otherwise calls `f` with the
    /// inner [`KitTy`] value and returns the result as a Resolved type.
    #[inline]
    #[must_use]
    pub fn and_then<F: FnOnce(KitTy) -> KitTy>(self, f: F) -> Type {
        match self {
            Self::Resolved(v) => Type::Resolved(f(v)),
            Self::Unresolved(u) => Type::Unresolved(u),
        }
    }

    /// Returns [`Type::Unresolved`] if the type is `Unresolved`, otherwise calls `f` with the
    /// inner [`KitTy`] value and returns the result.
    #[inline]
    #[must_use]
    pub fn map<F: FnOnce(KitTy) -> Type>(self, f: F) -> Type {
        match self {
            Self::Resolved(v) => f(v),
            Self::Unresolved(u) => Type::Unresolved(u),
        }
    }
}

impl Type {
    /// Create a new `Type` representing the unit type.
    #[inline]
    #[must_use]
    pub const fn unit() -> Self {
        Self::Resolved(KitTy::Unit)
    }

    /// Check if the type is the unit type.
    #[inline]
    #[must_use]
    pub fn is_unit(&self) -> bool {
        let Some(resolved) = self.resolved() else {
            return false;
        };
        *resolved == KitTy::Unit
    }

    /// Check if the type is resolved.
    #[inline]
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }

    /// Check if the type is an unresolved infer type.
    /// Meaning that this value's type should be inferred by the type checker.
    #[inline]
    #[must_use]
    pub const fn is_infer(&self) -> bool {
        matches!(self, Self::Unresolved(ASTTy::Infer))
    }

    /// Check if the type is a tuple type, either resolved or unresolved.
    #[inline]
    #[must_use]
    pub const fn is_tuple(&self) -> bool {
        match self {
            Self::Unresolved(ASTTy::Tuple(..)) => true,
            Self::Resolved(kit_ty) => matches!(kit_ty, KitTy::Tuple(_)),
            Self::Unresolved(_) => false,
        }
    }

    /// Returns `true` if the type is a reference type.
    #[inline]
    #[must_use]
    pub const fn is_ref(&self) -> bool {
        match self {
            Self::Resolved(kit_ty) => kit_ty.is_ref(),
            Self::Unresolved(_) => false,
        }
    }

    /// Returns `true` if the type is a mutable reference type.
    #[inline]
    #[must_use]
    pub const fn is_ref_mut(&self) -> bool {
        match self {
            Self::Resolved(kit_ty) => kit_ty.is_ref_mut(),
            Self::Unresolved(_) => false,
        }
    }

    /// Returns `true` if the type is a reference or mutable reference type.
    #[inline]
    #[must_use]
    pub const fn is_any_ref(&self) -> bool {
        self.is_ref() || self.is_ref_mut()
    }

    /// Get the resolved Kit type, if it exists.
    #[inline]
    #[must_use]
    pub const fn resolved(&self) -> Option<&KitTy> {
        match self {
            Self::Resolved(kit_ty) => Some(kit_ty),
            Self::Unresolved(_) => None,
        }
    }

    /// If the type is a container type (array, slice, reference), get the inner type.
    /// Otherwise, return `None`.
    /// # Note
    /// This only works for resolved types.
    #[inline]
    #[must_use]
    pub const fn inner_type(&self) -> Option<&KitTy> {
        match self {
            Self::Resolved(kit_ty) => match kit_ty {
                KitTy::Array(inner_ty, _)
                | KitTy::Slice(inner_ty)
                | KitTy::RefMut(inner_ty)
                | KitTy::Ref(inner_ty) => Some(inner_ty),
                _ => None,
            },
            Self::Unresolved(_) => None,
        }
    }

    /// Performs a _single_ dereference of the type if it is a reference or mutable reference.
    /// # Returns
    /// *   `Some(KitTy)`: Of the inner type, if `self` is a reference or mutable reference type.
    /// *   `None`: If `self` is not a reference type, or `self` is [`Type::Unresolved`].
    /// # Example
    /// *   If `self` is `&i32`, this function will return `i32`.
    /// *   If `self` is `&&i32`, this function will return `&i32`.
    /// *   If `self` is `i32`, this function will return `None`.
    /// *   If `self` is [`Type::Unresolved(..)`], this function will return `None`.
    /// # Note
    /// This only works for [`Type::Resolved`] types.
    #[inline]
    #[must_use]
    pub const fn deref(&self) -> Option<&KitTy> {
        match self {
            Self::Resolved(kit_ty) => kit_ty.deref(),
            Self::Unresolved(_) => None,
        }
    }

    /// Performs dereferencing recursively until a non-reference type is found.
    /// # Returns
    /// *   `Some(KitTy)`: Of the final inner type, if `self` is a reference or mutable reference type.
    /// *   `None`: If `self` is not a reference type, or `self` is [`Type::Unresolved`].
    /// # Example
    /// *   If `self` is `&i32`, this function will return `i32`.
    /// *   If `self` is `&&i32`, this function will return `i32`.
    /// *   If `self` is `i32`, this function will return `None`.
    /// *   If `self` is [`Type::Unresolved(..)`], this function will return `None`.
    /// # Note
    /// This only works for [`Type::Resolved`] types.
    #[inline]
    #[must_use]
    pub fn recursive_deref(&self) -> Option<&KitTy> {
        match self {
            Self::Resolved(kit_ty) => kit_ty.recursive_deref(),
            Self::Unresolved(_) => None,
        }
    }

    /// Performs dereferencing recursively until a non-reference type is found.
    /// This function will return the final inner type even if `self` is not a reference type.
    /// # Returns
    /// *   `Some(KitTy)`: Of the final inner type, if `self` is a reference or mutable reference type.
    /// *   `Some(KitTy)`: Of `self`, if `self` is not a reference type
    /// *   `None`: If `self` is [`Type::Unresolved(..)`].
    /// # Example
    /// *   If `self` is `&i32`, this function will return `i32`.
    /// *   If `self` is `&&i32`, this function will return `i32`.
    /// *   If `self` is `i32`, this function will return `i32`.
    /// *   If `self` is [`Type::Unresolved(..)`], this function will return `None`.
    #[inline]
    #[must_use]
    pub fn recursive_derefed(&self) -> Option<&KitTy> {
        match self {
            Self::Resolved(kit_ty) => Some(kit_ty.recursive_derefed()),
            Self::Unresolved(_) => None,
        }
    }

    /// Performs dereferencing recursively until a non-reference type is found.
    /// This function will return the final inner type even if the original type is not a reference type.
    /// # Returns
    /// *   [`Type`]: Of the final inner type, if the inner type is a reference or mutable reference type.
    /// *   `None`: If `self` is not a reference type, or `self` is [`Type::Unresolved`].
    /// # Example
    /// *   If `self` is `&i32`, this function will return [`Type::Resolved(i32)`].
    /// *   If `self` is `&&i32`, this function will return [`Type::Resolved(i32)`].
    /// *   If `self` is `i32`, this function will return [`Type::Resolved(i32)`].
    /// *   If `self` is [`Type::Unresolved(..)`], this function will return [`Type::Unresolved(..)`].
    #[inline]
    #[must_use]
    pub fn recursive_derefed_type(&self) -> Type {
        match self.recursive_derefed() {
            Some(inner_ty) => Type::Resolved(inner_ty.clone()),
            None => self.clone(),
        }
    }

    /// Check if all references that make up this type are mutable references.
    /// # Returns
    /// *   `true`: If all references are mutable references.
    /// *   `false`: If any reference is an immutable reference, or if the type is not a reference type.
    #[inline]
    #[must_use]
    pub fn are_all_refs_mut(&self) -> bool {
        let first = self.resolved().filter(|_| self.is_any_ref());
        std::iter::successors(first, |ty| ty.deref())
            .take_while(|ty| ty.is_any_ref())
            .all(KitTy::is_ref_mut)
    }

    /// Returns if all references that make up this type are immutable references.
    /// # Returns
    /// *   `true`: If all references are immutable references.
    /// *   `false`: If any reference is an mutable reference, or if the type is not a reference type.
    #[inline]
    #[must_use]
    pub fn are_all_refs_non_mut(&self) -> bool {
        let first = self.resolved().filter(|_| self.is_any_ref());
        std::iter::successors(first, |ty| ty.deref())
            .take_while(|ty| ty.is_any_ref())
            .all(KitTy::is_ref)
    }
}

impl Type {
    /// Create a `Type` from an AST type, attempting to resolve it to a Kit type if the resolution is trivial (e.g. built-in types).
    #[inline]
    #[must_use]
    pub fn from_ast_ty(ty: &ASTTy) -> Self {
        KitTy::try_from_ast_ty(ty).map_or_else(|| Self::Unresolved(ty.clone()), Self::Resolved)
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unresolved(ty) => write!(f, "Unresolved({ty:?})"),
            Self::Resolved(kit_ty) => {
                if let Some(ty_str) = kit_ty.to_type_str() {
                    write!(f, "{ty_str}")
                } else {
                    write!(f, "{kit_ty:?}")
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
        Self::Resolved(value.clone())
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

/// A node within an owning HIR node.
/// Represents things such as statements, expressions, parameters, etc.
#[derive(Clone, PartialEq)]
pub enum HirNode {
    /// Parameter of a function.
    Param(Parameter),
    /// Block of statements.
    Block(Block),
    /// An expression.
    /// # Example
    /// ```ignore
    /// 3 + 3
    /// ```
    Expr(Expr),
    /// A statement.
    /// This may be a variable declaration, or an expression, etc.
    /// A logical way to think about this would be a 'line' of code.
    /// That may be made up of multiple expressions.
    /// # Example
    /// ```ignore
    /// fn some_func() {
    ///     // This is what this statement type represents.
    ///     let x = 3 + 3;
    /// }
    /// ```
    Statement(Statement),
    /// A variable binding.
    /// This may be of multiple different kinds of binding, however the 'atomic' unit is a single variable.
    /// Other kinds of variable bindings such as tuple destructuring are represented as a collection of these.
    Binding(VarBinding),
    /// A field within a struct.
    Field(StructField),
    /// A reference to another node via a path.
    /// Can be either resolved or unresolved.
    Path(RefPath),
}

impl HirNode {
    /// Get the source span of the node.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Param(parameter) => parameter.span,
            Self::Block(block) => block.span,
            Self::Expr(expr) => expr.span,
            Self::Statement(statement) => statement.span,
            Self::Binding(binding) => binding.span,
            Self::Field(struct_field) => struct_field.span,
            Self::Path(ref_path) => ref_path.span(),
        }
    }
}

impl Debug for HirNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Param(arg0) => arg0.fmt(f),
            Self::Block(arg0) => arg0.fmt(f),
            Self::Expr(arg0) => arg0.fmt(f),
            Self::Statement(arg0) => arg0.fmt(f),
            Self::Binding(arg0) => arg0.fmt(f),
            Self::Field(arg0) => arg0.fmt(f),
            Self::Path(arg0) => f.debug_tuple("Path").field(&arg0).finish(),
        }
    }
}

macro_rules! impl_hir_node_from {
    ($target:ty, $variant:ident) => {
        impl<'a> From<&'a HirNode> for Option<&'a $target> {
            fn from(value: &'a HirNode) -> Self {
                match value {
                    HirNode::$variant(a) => Some(a),
                    _ => None,
                }
            }
        }

        impl<'a> From<&'a mut HirNode> for Option<&'a mut $target> {
            fn from(value: &'a mut HirNode) -> Self {
                match value {
                    HirNode::$variant(a) => Some(a),
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
impl_hir_node_from!(VarBinding, Binding);

/// Owning HIR node, which contains other HIR nodes and manages their scope.
/// Has a kind, which describes what type of owning node it is (e.g. an `Item` or an `Item` within an `impl` block).
#[derive(Debug, Clone, PartialEq)]
pub struct OwningNode {
    pub kind: OwningNodeKind,
    /// The HIR nodes owned by this owning node.
    pub nodes: Vec<HirNode>,
}

impl OwningNode {
    /// Create a new `OwningNode` with the specified kind.
    #[inline]
    #[must_use]
    pub const fn new(kind: OwningNodeKind) -> Self {
        Self {
            kind,
            nodes: Vec::new(),
        }
    }

    /// Create a new `OwningNode` from an `Item`, specifying whether it is an item within an `impl` block.
    #[inline]
    #[must_use]
    pub const fn from_item(item: Item, impl_item: bool) -> Self {
        Self::new(if impl_item {
            OwningNodeKind::ImplItem(item)
        } else {
            OwningNodeKind::Item(item)
        })
    }

    /// Set the owner definition ID of the owning node.
    #[inline]
    pub const fn set_owner_id(&mut self, owner_id: OwnerDefId) {
        self.kind.set_owner_id(owner_id);
    }

    /// Get the owner definition ID of the owning node.
    #[inline]
    #[must_use]
    pub const fn owner_id(&self) -> OwnerDefId {
        self.kind.owner_id()
    }

    /// Get the identifier of the item in the owning node, if it has one.
    #[inline]
    #[must_use]
    pub fn ident(&self) -> Option<String> {
        self.kind.ident()
    }

    /// Get a reference to the item in the owning node.
    #[inline]
    #[must_use]
    pub const fn item(&self) -> &Item {
        self.kind.item()
    }

    /// Get a mutable reference to the item in the owning node.
    #[inline]
    #[must_use]
    pub const fn item_mut(&mut self) -> &mut Item {
        self.kind.item_mut()
    }

    /// Get the source span of the item in the owning node, if it has one.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.kind.span()
    }
}

impl OwningNode {
    /// Get the number of local HIR nodes owned by this owning node.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub const fn local_count(&self) -> u32 {
        self.nodes.len() as u32
    }

    /// Get the next available local definition ID for a new HIR node.
    #[inline]
    #[must_use]
    pub const fn next_local_id(&self) -> LocalDefId {
        LocalDefId(self.local_count())
    }

    /// Get the next available HIR ID for a new HIR node.
    /// Combines the owning node's owner ID with the next local definition ID.
    #[inline]
    #[must_use]
    pub const fn next_hir_id(&self) -> HirId {
        HirId {
            owner: self.owner_id(),
            id: self.next_local_id(),
        }
    }

    /// Insert a new HIR node into the owning node, returning its assigned local definition ID.
    #[inline]
    pub fn insert_hir_node(&mut self, hir_node: HirNode) -> LocalDefId {
        let new_id = self.next_local_id();
        self.nodes.push(hir_node);
        new_id
    }

    /// Get a reference to a HIR node by its local definition ID.
    #[inline]
    #[must_use]
    pub fn get_hir_node(&self, id: LocalDefId) -> Option<&HirNode> {
        self.nodes.get(id.0 as usize)
    }

    /// Get a mutable reference to a HIR node by its local definition ID.
    #[inline]
    #[must_use]
    pub fn get_hir_node_mut(&mut self, id: LocalDefId) -> Option<&mut HirNode> {
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
                #[inline] #[must_use] pub const fn [<hir_ $item_name _mut>](&mut self) -> Option<&mut $return_ty> {
                    let item = match &mut self.kind {
                        OwningNodeKind::Item(item) => item,
                        OwningNodeKind::ImplItem(item) => item,
                    };
                    match &mut item.kind {
                        $item_kind => Some($return_expr),
                        _ => None,
                    }
                }

                #[inline] #[must_use] pub const fn [<hir_ $item_name _ref>](&self) -> Option<&$return_ty> {
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

            impl<'a> From<&'a OwningNode> for Option<&'a $return_ty> {
                fn from(value: &'a OwningNode) -> Self {
                    value.[<hir_ $item_name _ref>]()
                }
            }

            impl<'a> From<&'a mut OwningNode> for Option<&'a mut $return_ty> {
                fn from(value: &'a mut OwningNode) -> Self {
                    value.[<hir_ $item_name _mut>]()
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

/// Describes the span of a module, either declaration or implementation.
/// Used to differentiate between the two in error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSpan {
    /// Module declaration span.
    /// E.g. `module a;`
    Declaration(SourceSpan),
    /// Span of a module with a body.
    /// E.g. `module a { ... }`
    Implementation(SourceSpan),
}

impl ModuleSpan {
    /// Get the source span of the module span.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Declaration(source_span) | Self::Implementation(source_span) => *source_span,
        }
    }
}

/// Identifier for a module, either raw or spanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleIdent {
    /// Essentially the only module with this kind of [`ModuleIdent`] will be the root node.
    RawIdent(Ident),
    /// Almost all modules will have a [`SpannedIdent`], an Identifier with the span of the identifier.
    SpannedIdent(SpannedIdent),
}

impl ModuleIdent {
    /// Get the identifier of the module.
    #[inline]
    #[must_use]
    pub const fn ident(&self) -> &Ident {
        match self {
            Self::RawIdent(ident) => ident,
            Self::SpannedIdent(spanned_ident) => &spanned_ident.ident,
        }
    }

    /// Get the [`SourceSpan`] of the module identifier.
    /// Will return the null span for raw identifiers.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::RawIdent(_) => SourceSpan::new(0, 0),
            Self::SpannedIdent(spanned_ident) => spanned_ident.span,
        }
    }
}

/// A module in HIR, with a list of [`OwnerDefId`]s of the items it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub owner_id: OwnerDefId,
    pub ident: ModuleIdent,
    pub span: ModuleSpan,
    /// Whether the module is `pub` or not.
    pub vis: Visibility,
    /// The item IDs of the items contained in the module.
    pub item_ids: Vec<OwnerDefId>,
}

/// An individual parameter to a function.
/// # Example
/// ```ignore
/// fn some_func(
///     // This is what this definition represents.
///     mut x: i32,
/// ) { ... }
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd)]
pub struct Parameter {
    pub id: HirId,
    /// The [`OwnerDefId`] of the function this parameter belongs to.
    pub fn_id: OwnerDefId,
    /// Variable binding of the param.
    pub binding: HirId,
    /// The span of the full parameter definition (Including type and mutability).
    pub span: SourceSpan,
}

impl Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parameter {{ id: {:?}, binding: {:?} }}",
            self.id, self.binding
        )
    }
}

/// The "signature" of a function, holds the return type and the parameter types.
/// # Example
/// ```ignore
/// fn some_func
/// // This is what this represents.
/// (x: f32, y: f32) -> bool
/// { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct FunctionSig {
    /// The identifiers of the parameters, in order.
    pub parameter_idents: Vec<String>,
    /// The types of the parameters, in order.
    pub parameters: Vec<Type>,
    /// The return type of the function.
    pub output: Type,
    /// The span of the full function signature.
    pub span: SourceSpan,
}

/// The body of a [`Function`], containing the parameter IDs and the block ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionBody {
    /// [`HirId`] of the parameters of the function.
    pub params: Vec<HirId>,
    /// [`HirId`] of the block that makes up the function body.
    pub block: HirId,
}

/// A function in HIR, containing its signature, body, and other metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct Function {
    pub owner_id: OwnerDefId,
    /// The identifier of the function name.
    pub ident: SpannedIdent,
    /// Whether the function is `pub` or not.
    pub vis: Visibility,
    /// Whether the function is marked `native`.
    pub native: bool,
    /// Whether the function is a method (i.e. defined within an `impl` block with a `self` as the first parameter).
    pub is_method: bool,
    /// Whether the function is marked `global`.
    pub is_global: bool,
    /// The signature of the function. (return type, parameter types, etc.)
    pub sig: FunctionSig,
    /// The span of the function declaration (from `fn` to the end of the signature).
    pub decl_span: SourceSpan,
    /// The full span of the function (declaration + body).
    pub full_span: SourceSpan,
    /// The body of the function, if it is not native.
    pub body: Option<FunctionBody>,
}

// We intentially ignore decl_span and full_span, as they just clutter the debug output.
#[allow(clippy::missing_fields_in_debug)]
impl Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("ident", &self.ident.str())
            .field("native", &self.native)
            .field("is_method", &self.is_method)
            .field("is_global", &self.is_global)
            .field("sig", &self.sig)
            .field("vis", &self.vis)
            .field("body", &self.body)
            .finish()
    }
}

impl Function {
    /// If the function is a method, get the type of the `self` parameter.
    /// Otherwise, return `None`.
    #[inline]
    #[must_use]
    pub fn self_type(&self) -> Option<&Type> {
        if self.is_method {
            self.sig.parameters.first()
        } else {
            None
        }
    }
}

/// A field of a struct, containing the name of the field and it's associated [`Type`].
/// # Example
/// ```ignore
/// struct ExampleStruct {
///     // This is what this definition represents.
///     pub field_name: i32,
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    pub id: HirId,
    /// The identifier of the field.
    pub ident: SpannedIdent,
    /// The span of the field definition (including type and visibility).
    pub span: SourceSpan,
    /// The type of the field.
    pub ty: Type,
    /// Whether the field is `pub` or not.
    pub vis: Visibility,
}

/// A struct in HIR, containing its identifier, fields, and other metadata.
/// # Example
/// ```ignore
/// // This is what this definition represents.
/// struct ExampleStruct {
///     pub field_name: i32,
///     ...
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Struct {
    pub owner_id: OwnerDefId,
    /// The identifier of the struct.
    pub ident: SpannedIdent,
    /// The span of the full struct definition.
    pub span: SourceSpan,
    /// Whether the struct is `pub` or not.
    pub vis: Visibility,
    /// The [`HirId`]s of each [`StructField`] of the struct in order.
    pub fields: Vec<HirId>,
}

/// An enum in HIR, containing its identifier and other metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub owner_id: OwnerDefId,
    /// The identifier of the enum.
    pub ident: SpannedIdent,
    /// The span of the full enum definition.
    pub span: SourceSpan,
    /// Whether the enum is `pub` or not.
    pub vis: Visibility,
    // TODO: Variants..
}

/// A declaration of a constant, with the expression it should be evaluated to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constant {
    pub owner_id: OwnerDefId,
    /// The identifier of the constant.
    pub ident: SpannedIdent,
    /// The span of the full constant definition.
    pub span: SourceSpan,
    /// The type of the constant.
    pub ty: Type,
    /// Whether the constant is `pub` or not.
    pub vis: Visibility,
    /// The [`HirId`] of the expression that the constant evaluates to.
    pub expr: HirId,
}

/// An `impl` block in HIR, containing the type being implemented on and the items within the block.
/// # Example
/// ```ignore
/// impl ExampleStruct {
///     fn some_method(self) { ... }
///     ...
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impl {
    /// The span of the full `impl` block.
    pub span: SourceSpan,
    /// The owner definition ID of the `impl` block.
    pub owner_id: OwnerDefId,
    /// The span of the type being implemented on.
    pub ty_span: SourceSpan,
    /// The type being implemented on.
    pub self_ty: IdentPath,
    /// The list of [`OwnerDefId`]s of the items within the `impl` block.
    pub items: Vec<OwnerDefId>,
}

/// A `use` path in HIR, containing the path being imported and other metadata.
/// # Example
/// ```ignore
/// use some::module::path::ItemA;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsePath {
    pub owner_id: OwnerDefId,
    /// The list of import paths being imported.
    pub imports: Vec<IdentPath>,
    /// The span of the full `use` statement.
    pub span: SourceSpan,
    /// Whether the `use` statement is `pub` or not.
    /// I.e. if the imports should be re-exported.
    pub vis: Visibility,
}

// Item implementation..

/// The different kinds of items in HIR.
/// Differentiates between modules, functions, structs, constants, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    /// A module item.
    Module(Module),
    /// A function definition.
    Function(Function),
    /// A struct definition.
    Struct(Struct),
    /// An enum definition.
    Enum(Enum),
    /// A constant definition.
    Constant(Constant),
    /// An `impl` block.
    Impl(Impl),
    /// A `use` import statement.
    Use(UsePath),
}

impl ItemKind {
    /// Get the identifier of the item, if it has one.
    #[inline]
    #[must_use]
    pub fn ident(&self) -> Option<String> {
        match self {
            Self::Module(md) => Some(md.ident.ident().string()),
            Self::Function(f) => Some(f.ident.string()),
            Self::Struct(s) => Some(s.ident.string()),
            Self::Enum(e) => Some(e.ident.string()),
            Self::Constant(c) => Some(c.ident.string()),
            Self::Impl(_) | Self::Use(_) => None,
        }
    }

    /// Get the source span of the item, for error reporting.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Module(md) => md.ident.span(),
            Self::Function(f) => f.decl_span,
            Self::Struct(s) => s.ident.span,
            Self::Enum(e) => e.ident.span,
            Self::Constant(c) => c.ident.span,
            Self::Impl(a) => a.span,
            Self::Use(u) => u.span,
        }
    }
}

/// An item in HIR, containing its owner definition ID and kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub owner_id: OwnerDefId,
    pub kind: ItemKind,
}

impl Item {
    /// Create a new `Item` with the specified kind and a placeholder owner ID.
    #[inline]
    #[must_use]
    pub const fn new(kind: ItemKind) -> Self {
        Self {
            owner_id: OwnerDefId::PLACEHOLDER_ID,
            kind,
        }
    }

    /// Get the identifier of the item, if it has one.
    #[inline]
    #[must_use]
    pub fn ident(&self) -> Option<String> {
        self.kind.ident()
    }

    /// Get the source span of the item, for error reporting.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.kind.span()
    }
}

/// A block of statements in HIR.
/// Holds a list of [`HirId`]s that make up the block and whether
/// it is the root block of an item or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: HirId,
    /// The list of `HirNode::Statement`s in the block.
    pub statements: Vec<HirId>,
    /// Whether the block is the root block of an item (e.g. function body) or not.
    pub root_block: bool,
    /// The full source span of the block.
    pub span: SourceSpan,
}

// Statement..

/// The different kinds of statements in HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    /// A `let` statement.
    /// I.e. A variable declaration/initialization.
    /// # Example
    /// ```ignore
    /// let mut x: i32 = 5;
    /// ```
    Let(LetStatement),
    /// An item definition within a block.
    /// # Example
    /// ```ignore
    /// fn outer_func() {
    ///     // This is the item referenced by this statement type.
    ///     fn inner_func() { ... }
    /// }
    /// ```
    Item(OwnerDefId),
    /// An expression statement.
    /// # Example
    /// ```ignore
    /// 3 + 3
    /// ```
    Expr(HirId),
    /// A semi-colon terminated expression statement.
    /// # Example
    /// ```ignore
    /// 3 + 3;
    /// ```
    Semi(HirId),
}

/// A statement in HIR, containing its kind and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub id: HirId,
    pub kind: StatementKind,
    pub span: SourceSpan,
}

/// A `let` statement in HIR, containing the identifier, mutability, type, and optional initial value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetStatement {
    /// Pattern of the variable binding,
    pub binding: HirId,
    /// The type of the variable.
    /// This may be a defined type, or `ASTTy::Infer` if the type declaration is absent.
    pub ty: Type,
    /// The optional initial value of the variable.
    /// *   If `None`, the variable is uninitialized.
    /// *   If `Some`, the [`HirId`] of the expression that initializes the variable.
    pub initial_value: Option<HirId>,
}

// Exprs..

/// A field initialisation within a struct initialisation expression.
/// # Example
/// ```ignore
/// let x = ExampleStruct {
///     // This is what this struct field initialisation represents.
///     value: 5,
///     ...
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructFieldInit {
    /// The identifier of the field being initialised.
    pub ident: Ident,
    /// The [`HirId`] of the expression being assigned to the field.
    pub expr: HirId,
}

/// A struct initialisation expression in HIR, containing the type path and field initialisations.
/// # Example
/// ```ignore
/// let x = ExampleStruct {
///     value: 5,
///     ...
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructInitialisation {
    /// The path to the struct type being initialised.
    pub ty_path: RefPath,
    /// The field initialisations within the struct initialisation.
    pub fields: Vec<StructFieldInit>,
}

/// The different kinds of expressions in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Block(`HIRNode::Block`).
    Block(HirId),
    /// Literal(`Literal`)
    Literal(Literal),
    /// Binary Operation(`kind`, lhs: `HIRNode::Expr`, rhs: `HIRNode::Expr`)
    BinaryOp(BinaryOpKind, HirId, HirId),
    /// Unary Operation(`kind`, `HIRNode::Expr`)
    UnaryOp(UnaryOpKind, HirId),
    /// If(`condition`: `HIRNode::Expr`, `true_block`: `HIRNode::Block`, `else`: `HIRNode::Expr`)
    If(HirId, HirId, Option<HirId>),
    /// Infinite loop, with block: `HIRNode::Block`
    Loop(HirId),
    /// For loop, with (`enclosing_block`, `binding`, `iterable`: `HIRNode::Expr`, `loop_block`: `HIRNode::Block`)
    For(HirId, HirId, HirId, HirId),
    /// While(`condition`: `HIRNode::Expr`, `block`: `HIRNode::Block`)
    While(HirId, HirId),
    /// Assign(`target`: `HIRNode::Expr`, `value`: `HIRNode::Expr`)
    Assign(HirId, HirId),
    /// Call(`target`: `HIRNode::Expr`, `args`: `Vec<HIRNode::Expr>`)
    Call(HirId, Vec<HirId>),
    /// Method Call(`target`: `HIRNode::Expr`, `method_name`: Ident, `args`: `Vec<HIRNode::Expr>`)
    MethodCall(HirId, SpannedIdent, Vec<HirId>),
    /// Index(`target`: `HIRNode::Expr`, `index`: `HIRNode::Expr`)
    Index(HirId, HirId),
    /// Field Access(`target`: `HIRNode::Expr`, `field_name`: Ident)
    FieldAccess(HirId, Ident),
    /// Struct Initialisation
    StructInit(StructInitialisation),
    /// Path
    Path(RefPath),
    /// Continue to next loop iteration
    Continue,
    /// Break from loop
    Break,
    /// Return from function(`Option<HIRNode::Expr>`)
    Return(Option<HirId>),
    /// Cast an expression from one type to another.
    Cast(HirId, Type),
    /// Range expression, start, end, (`inclusive`: `bool`)
    Range(HirId, HirId, bool),
    /// Tuple expression with multiple elements
    Tuple(Vec<HirId>),
    /// Array expression with multiple elements
    ArrayInit(Vec<HirId>),
    /// A reference to another element.
    Reference(HirId, Mutability),
}

/// An expression in HIR, containing its kind and source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub id: HirId,
    pub kind: ExprKind,
    pub span: SourceSpan,
}

/// The resolved ID of a reference path in HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedID {
    /// The path was resolved to a [`HirId`].
    Hir(HirId),
    /// The path was resolved to a [`DefId`].
    Def(DefId),
    /// The path was resolved to an [`OwnerDefId`].
    OwnerDef(OwnerDefId),
    /// The path was resolved to a [`TypeID`].
    TypeDef(TypeID),
}

impl ResolvedID {
    /// Get the `HirId` if this `ResolvedID` is of the `Hir` variant.
    #[inline]
    #[must_use]
    pub const fn hir_id(&self) -> Option<HirId> {
        match self {
            Self::Hir(hir_id) => Some(*hir_id),
            _ => None,
        }
    }

    /// Get the `DefId` if this `ResolvedID` is of the `Def` variant.
    #[inline]
    #[must_use]
    pub const fn def_id(&self) -> Option<DefId> {
        match self {
            Self::Def(def_id) => Some(*def_id),
            _ => None,
        }
    }

    /// Get the `OwnerDefId` if this `ResolvedID` is of the `OwnerDef` variant.
    #[inline]
    #[must_use]
    pub const fn owner_def_id(&self) -> Option<OwnerDefId> {
        match self {
            Self::OwnerDef(owner_def_id) => Some(*owner_def_id),
            _ => None,
        }
    }

    /// Get the `TypeID` if this `ResolvedID` is of the `TypeDef` variant.
    #[inline]
    #[must_use]
    pub const fn type_id(&self) -> Option<TypeID> {
        match self {
            Self::TypeDef(type_id) => Some(*type_id),
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

/// A reference path in HIR, which can be either unresolved (AST path) or resolved (with a `ResolvedID`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefPath {
    /// A path that has not yet been resolved.
    Unresolved(SpannedIdentPath),
    /// A path that has been resolved to a specific ID.
    Resolved(SpannedIdentPath, ResolvedID),
}

impl RefPath {
    /// Get the spanned identifier path of the path reference.
    #[inline]
    #[must_use]
    pub const fn spanned_ident_path(&self) -> &SpannedIdentPath {
        match self {
            Self::Unresolved(ident_path) | Self::Resolved(ident_path, _) => ident_path,
        }
    }

    /// Get the identifier path of the path reference.
    #[inline]
    #[must_use]
    pub const fn ident_path(&self) -> &IdentPath {
        match self {
            Self::Unresolved(ident_path) | Self::Resolved(ident_path, _) => &ident_path.path,
        }
    }

    /// Get the [`SourceSpan`] of the path reference.
    #[inline]
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Unresolved(ident_path) | Self::Resolved(ident_path, _) => ident_path.span,
        }
    }

    /// Returns `true` if the path reference has been resolved.
    #[inline]
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_, _))
    }

    /// Get the resolved ID of the path reference, if it has been resolved.
    #[inline]
    #[must_use]
    pub const fn resolved_id(&self) -> Option<ResolvedID> {
        match self {
            Self::Unresolved(_) => None,
            Self::Resolved(_, id) => Some(*id),
        }
    }

    /// Resolve the path reference to the specified `ResolvedID`.
    #[inline]
    pub fn resolve_to(&mut self, id: ResolvedID) {
        *self = Self::Resolved(self.spanned_ident_path().clone(), id);
    }

    /// Resolve the path reference to the specified `HirId`.
    #[inline]
    pub fn resolve_to_hir_id(&mut self, id: HirId) {
        *self = Self::Resolved(self.spanned_ident_path().clone(), ResolvedID::Hir(id));
    }

    /// Resolve the path reference to the specified `OwnerDefId`.
    #[inline]
    pub fn resolve_to_owner_id(&mut self, id: OwnerDefId) {
        *self = Self::Resolved(self.spanned_ident_path().clone(), ResolvedID::OwnerDef(id));
    }

    /// Resolve the path reference to the specified `DefId`.
    #[inline]
    pub fn resolve_to_def_id(&mut self, id: DefId) {
        *self = Self::Resolved(self.spanned_ident_path().clone(), ResolvedID::Def(id));
    }
}

/// Modifiers for a variable binding, such as mutability.
/// # Example
/// ```ignore
/// let mut x = 5;
/// ```
/// The `mut` modifier is represented by this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingModifiers {
    /// Whether the binding is mutable or not.
    pub mutable: Mutability,
}

/// The different kinds of variable bindings in HIR.
/// I.e. A single identifier such as `let x = ...` or a tuple binding such as `let (x, y) = ...`.
/// In this system, the `Ident` variant is used as the atomic unit of variable bindings.
/// Tuple bindings are represented as a collection of `Ident` bindings, referenced by a [`HirId`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingKind {
    /// A single identifier binding.
    /// # Example
    /// ```ignore
    /// let x = 5;
    /// ```
    Ident(SpannedIdent),
    /// A tuple binding, containing a small vector of the `HirId`s of the individual identifier bindings.
    /// # Example
    /// ```ignore
    /// let (x, y) = (5, 10);
    /// ```
    Tuple(KitSmallVec<HirId>),
}

/// A variable binding in HIR, containing its `HirId` and kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarBinding {
    pub id: HirId,
    pub kind: BindingKind,
    pub modifiers: BindingModifiers,
    pub span: SourceSpan,
}

impl VarBinding {
    /// Get the string representation of the binding.
    #[inline]
    #[must_use]
    pub fn string_repr(&self, hlir: &crate::intermediate::hir::HLIR) -> String {
        match &self.kind {
            BindingKind::Ident(ident) => ident.string(),
            BindingKind::Tuple(ids) => {
                format!(
                    "({})",
                    ids.iter()
                        .map(|pat_id| hlir
                            .binding_by_id(*pat_id)
                            .map_or_else(|| "??var??".to_string(), |p| p.string_repr(hlir)))
                        .join(", ")
                )
            }
        }
    }
}
