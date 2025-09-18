use super::{DefId, HirId, OwnerDefId};
use crate::ast;

#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldInit {
    pub ident: ast::Ident,
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
    Literal(ast::Literal),
    /// Binary Operation(Kind, lhs: HIRNode::Expr, rhs: HIRNode::Expr)
    BinaryOp(ast::BinaryOpKind, HirId, HirId),
    /// Unary Operation(Kind, HIRNode::Expr)
    UnaryOp(ast::UnaryOpKind, HirId),
    /// If(condition: HIRNode::Expr, true_block: HIRNode::Block, else: HIRNode::Expr)
    If(HirId, HirId, Option<HirId>),
    /// While(condition: HIRNode::Expr, block: HIRNode::Block)
    While(HirId, HirId),
    /// Assign(target: HIRNode::Expr, value: HIRNode::Expr)
    Assign(HirId, HirId),
    /// Call(target: HIRNode::Expr, args: Vec<HIRNode::Expr>)
    Call(HirId, Vec<HirId>),
    /// Method Call(target: HIRNode::Expr, method_name: Ident, args: Vec<HIRNode::Expr>)
    MethodCall(HirId, ast::Ident, Vec<HirId>),
    /// Index(target: HIRNode::Expr, index: HIRNode::Expr)
    Index(HirId, HirId),
    /// Field Access(target: HIRNode::Expr, field_name: Ident)
    FieldAccess(HirId, ast::Ident),
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub id: HirId,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedID {
    Hir(HirId),
    Def(DefId),
    OwnerDef(OwnerDefId),
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
    Unresolved(ast::IdentPath),
    Resolved(ast::IdentPath, ResolvedID),
}

impl RefPath {
    pub fn ident_path(&self) -> &ast::IdentPath {
        match self {
            RefPath::Unresolved(ident_path) => ident_path,
            RefPath::Resolved(ident_path, _) => ident_path,
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
        *self = RefPath::Resolved(self.ident_path().clone(), id);
    }

    pub fn resolve_to_hir_id(&mut self, id: HirId) {
        *self = RefPath::Resolved(self.ident_path().clone(), ResolvedID::Hir(id));
    }

    pub fn resolve_to_owner_id(&mut self, id: OwnerDefId) {
        *self = RefPath::Resolved(self.ident_path().clone(), ResolvedID::OwnerDef(id));
    }

    pub fn resolve_to_def_id(&mut self, id: DefId) {
        *self = RefPath::Resolved(self.ident_path().clone(), ResolvedID::Def(id));
    }
}
