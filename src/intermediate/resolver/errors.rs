use crate::ast::SpannedIdentPath;
use crate::intermediate::hir::HirId;
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolutionFailure {
    NotFound,
    Inaccessible,
}

#[derive(Clone, PartialEq)]
pub struct UnresolvedReference {
    pub path: SpannedIdentPath,
    pub id: HirId,
    pub failure: ResolutionFailure,
}

impl Debug for UnresolvedReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", &self.path)
    }
}

#[derive(Clone, PartialEq)]
pub struct UnresolvedReferences {
    pub references: Vec<UnresolvedReference>,
}

impl Debug for UnresolvedReferences {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.references).finish()
    }
}
