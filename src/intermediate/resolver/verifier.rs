//! The purpose of this module is to verify that all reference paths have been resolved after the
//! resolution phase of the compiler.
//!
//! Traverses the [`HLIR`] and records any `RefPath::Unresolved`, and aggregating all such instances into one
//! `ResolverError` containing all unresolved references.
//!
//! This file does not perform resolution or mutate the HLIR.
//! It only detects unresolved references and surfaces them as diagnostics for the resolution pipeline.
//!
//! This module is exclusively focused on verification of unresolved references.

use crate::intermediate::hir::nodes::RefPath;
use crate::intermediate::hir::visitor::HLIRVisitor;
use crate::intermediate::hir::{HLIR, HirId};

use crate::intermediate::resolver::errors::{
    ResolutionFailure, ResolveResult, ResolverErrorKind, UnresolvedReference, UnresolvedReferences,
};

/// Visitor that checks for unresolved references in the HLIR.
/// It collects all unresolved references found during the visit.
#[derive(Default)]
struct UnresolvedReferenceChecker {
    /// All found unresolved references.
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
    /// Verifies all references in the given HLIR.
    /// Visits all `path` nodes and collects any that are still unresolved.
    /// # Returns
    /// * `Ok(())` if all references are resolved.
    /// * `Err(ResolverError)` if there are any unresolved references.
    /// # Errors
    /// This function will return an error if there are any unresolved references in the [`HLIR`] remaining.
    /// The returned error will contain all unresolved references found.
    pub fn verify_references(mut self, hlir: &HLIR) -> ResolveResult<()> {
        self.visit_root(hlir);

        if self.unresolved_references.is_empty() {
            Ok(())
        } else {
            Err(
                ResolverErrorKind::UnresolvedReferences(UnresolvedReferences {
                    references: self.unresolved_references,
                })
                .with_no_span(),
            )
        }
    }
}

/// Verifies all references in the given HLIR.
/// Visits all `path` nodes and collects any that are still unresolved.
/// # Returns
/// * `Ok(())` if all references are resolved.
/// * `Err(ResolverError)` if there are any unresolved references.
/// # Errors
/// This function will return an error if there are any unresolved references in the [`HLIR`] remaining.
/// The returned error will contain all unresolved references found.
pub fn verify_references(hlir: &HLIR) -> ResolveResult<()> {
    UnresolvedReferenceChecker::default().verify_references(hlir)
}
