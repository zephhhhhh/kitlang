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
    Block(HirId),
    Literal(ast::Literal),
    BinaryOp(ast::BinaryOpKind, HirId, HirId),
    UnaryOp(ast::UnaryOpKind, HirId),
    If(HirId, HirId, Option<HirId>),
    While(HirId, HirId),
    Assign(HirId, HirId),
    Call(HirId, Vec<HirId>),
    MethodCall(HirId, ast::Ident, Vec<HirId>),
    Index(HirId, HirId),
    FieldAccess(HirId, ast::Ident),
    StructInit(StructInitialisation),
    Path(RefPath),
    Continue,
    Break,
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
