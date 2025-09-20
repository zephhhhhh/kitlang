use crate::ast::{self, Ident, Mutability, SourceSpan};

use super::{HirId, OwnerDefId};

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
    pub ty: ast::Ty, // TODO: Change this.
    pub initial_value: Option<HirId>,
}
