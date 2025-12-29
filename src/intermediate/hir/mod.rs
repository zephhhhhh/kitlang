//! The HIR module provides the primary IR structure for the compiler.
//! It transforms the AST into a flatter representation optimized for analysis passes like resolution and type checking.
//!
//! The [`HLIR`] organizes nodes into "owning nodes" (top-level items like functions, structs, impls) and their owned
//! nodes (expressions, statements), each addressable via [`HirId`]. This structure enables efficient lookups and
//! storage of side-channel data like type information without deep tree traversals, and having to store a way of navigating
//! this tree structure.
//!
//! This module defines core ID types ([`HirId`], [`OwnerDefId`], [`LocalDefId`]), the [`HLIR`] container,
//! program metadata ([`ProgramMetaData`]), and utilities for lowering, [traversing](crate::intermediate::hir::visitor),
//! and manipulating the HIR.

pub mod errors;
mod hlir;
pub mod lowerer;
pub mod nodes;
pub mod visitor;

pub use hlir::*;

use ::std::fmt::Debug;

use crate::ast::{ASTRoot, IdentPath, SourceSpan};
use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::nodes::Type;
use crate::intermediate::resolver::{
    ADTTypeInfo, Namespace, RootNamespace, TypeRegistry, resolve_paths,
};
use crate::intermediate::type_check::{TypeMap, run_type_checker};

pub use crate::intermediate::hir::errors::{LoweringError, LoweringErrorKind};
use crate::intermediate::types::KitTy;

/// Definition ID that is only valid when relative to the "owner".
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDefId(pub u32);

impl LocalDefId {
    /// Placeholder ID constant.
    pub const PLACEHOLDER_ID: Self = Self(u32::MAX);

    /// Check if this is a placeholder ID. I.e. (`self == u32::MAX`).
    #[inline]
    #[must_use]
    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }
}

impl Debug for LocalDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalDefId({})", self.0)
    }
}

/// Definition ID that describes the index into the global "module" nodes of an owning HIR node.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerDefId(pub u32);

impl OwnerDefId {
    /// Placeholder ID constant.
    pub const PLACEHOLDER_ID: Self = Self(u32::MAX);
    /// Owner definition of the root node (I.e. the module with the main function.)
    pub const ROOT_NODE: Self = Self(0);

    /// Check if this is a placeholder ID. I.e. (`self == u32::MAX`).
    #[inline]
    #[must_use]
    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }
}

impl Debug for OwnerDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OwnerDefId({})", self.0)
    }
}

/// Definition ID that is relative to a specified module.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefId {
    pub module_id: u32,
    pub def_id: LocalDefId,
}

impl DefId {
    // Placeholder ID constant.
    pub const PLACEHOLDER_ID: Self = Self {
        module_id: u32::MAX,
        def_id: LocalDefId::PLACEHOLDER_ID,
    };

    /// Check if this is a placeholder ID. I.e. `module_id == u32::MAX` or `def_id == u32::MAX`.
    #[inline]
    #[must_use]
    pub fn is_placeholder(self) -> bool {
        self.module_id == u32::MAX || self.def_id.is_placeholder()
    }
}

impl Debug for DefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefId({} : {:?})", self.module_id, self.def_id)
    }
}

/// Definition ID that describes which ID into the modules `"path_table"` owns the node, and which
/// index into the owners body the node refers to.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirId {
    /// Index into the HLIR owning node list of the owner of this node.
    pub owner: OwnerDefId,
    /// Index into the owning node's HIR node list.
    pub id: LocalDefId,
}

impl From<HirId> for LocalDefId {
    fn from(value: HirId) -> Self {
        value.id
    }
}

impl From<HirId> for OwnerDefId {
    fn from(value: HirId) -> Self {
        value.owner
    }
}

impl From<&HirId> for LocalDefId {
    fn from(value: &HirId) -> Self {
        value.id
    }
}

impl From<&HirId> for OwnerDefId {
    fn from(value: &HirId) -> Self {
        value.owner
    }
}

impl HirId {
    /// Placeholder ID constant.
    pub const PLACEHOLDER_ID: Self = Self {
        owner: OwnerDefId::PLACEHOLDER_ID,
        id: LocalDefId::PLACEHOLDER_ID,
    };

    /// Check if this is a placeholder ID. I.e. `owner == u32::MAX` or `id == u32::MAX`.
    #[inline]
    #[must_use]
    pub fn is_placeholder(self) -> bool {
        self.owner.is_placeholder() || self.id.is_placeholder()
    }
}

impl HirId {
    /// Get the next ID on the current owner by incrementing the local ID by 1.
    #[inline]
    #[must_use]
    pub const fn next_id(mut self) -> Self {
        self.id = LocalDefId(self.id.0 + 1);
        self
    }
}

impl Debug for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HirId({:?} : {:?})", self.owner, self.id)
    }
}

/// Contains metadata about the program stored in HIR, such as type information, namespace etc.
#[derive(Debug, Default, Clone)]
pub struct ProgramMetaData {
    /// The program's namespace information, I.e. all defined modules, types, functions, etc.
    pub namespace: RootNamespace,
    /// The program's type registry, containing all defined types.
    pub type_registry: TypeRegistry,
    /// The program's type map, containing side channel
    /// information about what types a given [`HirId`] evaluates to.
    pub type_map: TypeMap,
}

impl ProgramMetaData {
    /// Get a reference to the [`Namespace`] defined at a given `path`, if it exists.
    #[inline]
    #[must_use]
    pub fn get_namespace(&self, path: &IdentPath) -> Option<&Namespace> {
        self.namespace.find_definition(path)
    }

    /// Get a mutable reference to the [`Namespace`] defined at a given `path`, if it exists.
    #[inline]
    #[must_use]
    pub fn get_namespace_mut(&mut self, path: &IdentPath) -> Option<&mut Namespace> {
        self.namespace.find_definition_mut(path)
    }

    /// Find a method in the namespace given its defining path, data type identifier, and method identifier.
    /// # Returns
    /// The method namespace if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_method(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<&Namespace> {
        self.namespace
            .find_method(defined_in, data_type_ident, method_ident)
    }

    /// Find a method owner definition ID in the namespace given its defining path, data type identifier, and method identifier.
    /// # Returns
    /// The method owner definition ID if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_method_owner_def(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<OwnerDefId> {
        self.namespace
            .find_method_owner_def(defined_in, data_type_ident, method_ident)
    }

    /// Find the owner definition ID for a method defined on an ADT.
    /// # Returns
    /// The method owner definition ID if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_adt_method_owner_def(
        &self,
        adt: &ADTTypeInfo,
        method_ident: &str,
    ) -> Option<OwnerDefId> {
        self.find_method_owner_def(&adt.defined_in, adt.type_ident.str(), method_ident)
    }

    /// Find the owner definition ID for a method defined on a given type.
    /// # Returns
    /// The method owner definition ID if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_ty_method_owner_def(&self, ty: &Type, method_ident: &str) -> Option<OwnerDefId> {
        let Type::Resolved(resolved_ty) = ty else {
            return None;
        };
        match resolved_ty {
            KitTy::Abstract(adt_id) => {
                let adt = self.type_registry.get_from_type_id(*adt_id)?;
                self.find_method_owner_def(&adt.defined_in, adt.type_ident.str(), method_ident)
            }
            t => self.find_method_owner_def(
                &IdentPath::new_empty(true),
                &t.to_type_str()?,
                method_ident,
            ),
        }
    }

    /// Try to get the type name of a given type.
    /// # Returns
    /// The type name if it can be determined, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn try_type_name(&self, ty: impl Into<Type>) -> Option<String> {
        match ty.into() {
            Type::Unresolved(ty) => Some(ty.get_type_ident()),
            Type::Resolved(KitTy::Abstract(ty_id)) => {
                let abs_ty = self.type_registry.get_from_type_id(ty_id)?;
                let type_path = abs_ty.defined_in.extend_ident(&abs_ty.type_ident.ident);
                Some(type_path.to_string())
            }
            Type::Resolved(kit_ty) => kit_ty.to_type_str(),
        }
    }

    /// Get the type name of a given type, or `"??"` if it cannot be determined.
    /// # Returns
    /// The type name, or `"??"` if the type name cannot be determined.
    #[inline]
    #[must_use]
    pub fn type_name(&self, ty: impl Into<Type>) -> String {
        self.try_type_name(ty).unwrap_or_else(|| String::from("??"))
    }
}

/// Get the source span for a given HIR ID.
/// # Returns
/// The source span if found, otherwise a null span.
pub(crate) fn get_span_by_id(hlir: &HLIR, hir_id: HirId) -> SourceSpan {
    hlir.get_hir_node(hir_id)
        .map_or_else(SourceSpan::null_span, nodes::HirNode::span)
}

// Outward facing API..

/// Lower the output of the parser stage to HIR.
/// # Note
/// This function does not do any later processing.
/// # Errors
/// This function will return an error if any part of the AST cannot be lowered properly.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures,
/// as well as which stage of lowering it occurred in.
pub fn lower_ast_to_hir(ast: &ASTRoot) -> LowerResult<HLIR> {
    lowerer::lower_ast_to_hir(ast)
}

/// Lower the output of the parser state to HIR, using a provided metadata structure.
/// # Note
/// Unlike `lower_ast_to_hir`, this function does do all HIR processing.
/// (Type checking, resolution, etc).
/// # Errors
/// This function will return an error if any part of the AST cannot be lowered properly.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures,
/// as well as which stage of lowering it occurred in.
pub fn parse_ast_to_hir_processed_with(
    ast: &ASTRoot,
    meta_data: &mut ProgramMetaData,
) -> LowerResult<HLIR> {
    let mut hlir = lower_ast_to_hir(ast)?;

    // Resolution..
    resolve_paths(&mut hlir, meta_data)
        .map_err(|e| LoweringErrorKind::ResolverError(e).with_no_span())?;

    // Type checking..
    run_type_checker(&mut hlir, meta_data)?;

    Ok(hlir)
}

/// Lower the output of the parser state to HIR.
/// # Note
/// Unlike `lower_ast_to_hir`, this function does do all HIR processing.
/// (Type checking, resolution, etc).
/// # Errors
/// This function will return an error if any part of the AST cannot be lowered properly.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures,
/// as well as which stage of lowering it occurred in.
pub fn parse_ast_to_hir_processed(ast: &ASTRoot) -> LowerResult<(ProgramMetaData, HLIR)> {
    let mut meta_data = ProgramMetaData::default();

    let hlir = parse_ast_to_hir_processed_with(ast, &mut meta_data)?;

    Ok((meta_data, hlir))
}
