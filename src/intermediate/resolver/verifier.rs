use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::nodes::RefPath;
use crate::intermediate::hir::visitor::HLIRVisitor;
use crate::intermediate::hir::{HLIR, HirId};

use crate::intermediate::resolver::errors::{
    ResolutionFailure, UnresolvedReference, UnresolvedReferences,
};

struct UnresolvedReferenceChecker {
    pub unresolved_references: Vec<UnresolvedReference>,
}

impl HLIRVisitor for UnresolvedReferenceChecker {
    fn visit_path(&mut self, id: HirId, path: &RefPath, hlir: &HLIR) {
        if let RefPath::Unresolved(ident_path) = path {
            self.unresolved_references.push(UnresolvedReference {
                path: ident_path.clone(),
                id,
                failure: ResolutionFailure::NotFound,
            });
        }

        self.super_path(id, path, hlir);
    }
}

impl UnresolvedReferenceChecker {
    pub fn new() -> Self {
        Self {
            unresolved_references: Vec::new(),
        }
    }

    pub fn verify_references(mut self, hlir: &HLIR) -> LowerResult<()> {
        self.visit_root(hlir);

        if self.unresolved_references.is_empty() {
            Ok(())
        } else {
            Err(LoweringError::new(LoweringErrorKind::UnresolvedReferences(
                UnresolvedReferences {
                    references: self.unresolved_references,
                },
            )))
        }
    }
}

pub fn verify_references(hlir: &mut HLIR) -> LowerResult<()> {
    UnresolvedReferenceChecker::new().verify_references(hlir)
}
