use ::std::fmt::Debug;

use crate::ast::{ASTRoot, IdentPath, SourceSpan};

use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::nodes::{HirNode, Item, OwningNode, OwningNodeKind, Type};
use crate::intermediate::resolver::{ADTTypeInfo, Namespace, TypeRegistry, resolve_paths};
use crate::intermediate::type_check::{TypeMap, run_type_checker};

pub use crate::intermediate::hir::errors::{LoweringError, LoweringErrorKind};
use crate::intermediate::types::KitTy;

pub mod lowerer;

pub mod errors;
pub mod nodes;
pub mod visitor;

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

/// High Level Intermediate Representation of the program.
///
/// Contains all HIR nodes in owning nodes, which are organized in a structure where
/// top level items such as structures, impls, etc each have their own owning node.
///
/// And all lower level nodes such as statements, expressions, etc are stored within those owning nodes.
///
/// In this fashion, the HIR can represent the entire program in a structure that is much flatter
/// than the tree structure of the AST, while still maintaining the hierarchical relationships.
///
/// This means that all nodes have a small `HirId` that can be used to reference them, without needing
/// to traverse a large tree structure.
///
/// This makes lookups and storing side channel data such as type information much easier and more efficient.
#[derive(Default, Clone)]
pub struct HLIR {
    /// List of all owning nodes in the HIR.
    pub owner_nodes: Vec<OwningNode>,
}

impl HLIR {
    /// Get the total count of owner nodes in the HIR.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub const fn owner_node_count(&self) -> u32 {
        self.owner_nodes.len() as u32
    }

    /// Get an iterator over all owner IDs in the HIR.
    #[inline]
    pub fn owner_id_iter(&self) -> impl Iterator<Item = OwnerDefId> {
        (0..self.owner_node_count()).map(OwnerDefId)
    }

    /// Get a reference to the root module owning node.
    #[inline]
    #[must_use]
    pub fn root_module(&self) -> Option<&OwningNode> {
        self.owning_node(OwnerDefId::ROOT_NODE)
    }

    /// Get a mutable reference to the root module owning node.
    #[inline]
    #[must_use]
    pub fn root_module_mut(&mut self) -> Option<&mut OwningNode> {
        self.owning_node_mut(OwnerDefId::ROOT_NODE)
    }

    /// Get a reference to the owning node with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    pub fn owning_node(&self, id: OwnerDefId) -> Option<&OwningNode> {
        self.owner_nodes.get(id.0 as usize)
    }

    /// Get a mutable reference to the owning node with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    pub fn owning_node_mut(&mut self, id: OwnerDefId) -> Option<&mut OwningNode> {
        self.owner_nodes.get_mut(id.0 as usize)
    }

    /// Get a reference to the owning node without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    pub fn owning_node_unchecked(&self, id: OwnerDefId) -> &OwningNode {
        self.owner_nodes.get(id.0 as usize).expect("Node exists.")
    }

    /// Get a mutable reference to the owning node without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    pub fn owning_node_mut_unchecked(&mut self, id: OwnerDefId) -> &mut OwningNode {
        self.owner_nodes
            .get_mut(id.0 as usize)
            .expect("Node exists.")
    }

    /// Get the next available owner ID for inserting a new owning node.
    #[inline]
    #[must_use]
    pub const fn next_owner_id(&self) -> OwnerDefId {
        OwnerDefId(self.owner_node_count())
    }

    /// Insert a new owning node into the HIR, returning its assigned [`OwnerDefId`].
    #[inline]
    pub fn insert_owning_node(&mut self, mut owner_node: OwningNode) -> OwnerDefId {
        let next_id = self.next_owner_id();
        owner_node.set_owner_id(next_id);
        self.owner_nodes.push(owner_node);
        next_id
    }

    /// Insert a new owning node into the HIR, updating its parent's item list,
    /// and returning its assigned [`OwnerDefId`].
    #[inline]
    pub fn insert_owning_node_with_parent(
        &mut self,
        owner_node: OwningNode,
        parent_id: OwnerDefId,
    ) -> OwnerDefId {
        let is_normal_item = matches!(&owner_node.kind, OwningNodeKind::Item(_));
        let new_node_id = self.insert_owning_node(owner_node);
        if is_normal_item {
            self.update_parent_item_list(new_node_id, parent_id);
        }
        new_node_id
    }

    /// Push a new item to the parent owning node's item list.
    fn update_parent_item_list(&mut self, new_node: OwnerDefId, parent_id: OwnerDefId) {
        if let Some(parent_module) = self
            .owning_node_mut(parent_id)
            .and_then(|o| o.hir_module_mut())
        {
            parent_module.item_ids.push(new_node);
        }
    }
}

impl HLIR {
    /// Get a reference to the [`HirNode`] with a specified [`HirId`].
    #[inline]
    #[must_use]
    pub fn get_hir_node(&self, hir_id: HirId) -> Option<&HirNode> {
        self.owning_node(hir_id.owner)?.get_hir_node(hir_id.id)
    }

    /// Get a reference to the [`HirNode`] with a specified [`HirId`] without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    pub fn get_hir_node_unchecked(&self, hir_id: HirId) -> &HirNode {
        self.owning_node_unchecked(hir_id.owner)
            .get_hir_node(hir_id.id)
            .expect("HIRNode exists.")
    }

    /// Get a mutable reference to the [`HirNode`] with a specified [`HirId`].
    #[inline]
    #[must_use]
    pub fn get_hir_node_mut(&mut self, hir_id: HirId) -> Option<&mut HirNode> {
        self.owning_node_mut(hir_id.owner)?
            .get_hir_node_mut(hir_id.id)
    }

    /// Get a mutable reference to the [`HirNode`] with a specified [`HirId`] without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    pub fn get_hir_node_mut_unchecked(&mut self, hir_id: HirId) -> &mut HirNode {
        self.owning_node_mut_unchecked(hir_id.owner)
            .get_hir_node_mut(hir_id.id)
            .expect("HIRNode exists.")
    }

    /// Get the next available [`HirId`] for a specified owner.
    #[inline]
    #[must_use]
    pub fn next_hir_id_on(&self, owner_id: impl Into<OwnerDefId>) -> HirId {
        self.owning_node(owner_id.into())
            .map_or(HirId::PLACEHOLDER_ID, nodes::OwningNode::next_hir_id)
    }

    /// Insert a new [`HirNode`] into the HIR under the specified owner, returning its assigned [`HirId`].
    #[inline]
    pub fn insert_hir_node(
        &mut self,
        owner_id: impl Into<OwnerDefId>,
        hir_node: HirNode,
    ) -> Option<HirId> {
        let owner = owner_id.into();
        let local_id = self.owning_node_mut(owner)?.insert_hir_node(hir_node);
        Some(HirId {
            owner,
            id: local_id,
        })
    }
}

impl HLIR {
    /// Get a reference to the owning node's item with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    pub fn owning_node_item(&self, owner_id: OwnerDefId) -> Option<&Item> {
        Some(self.owning_node(owner_id)?.item())
    }

    /// Get a mutable reference to the owning node's item with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    pub fn owning_node_item_mut(&mut self, owner_id: OwnerDefId) -> Option<&mut Item> {
        Some(self.owning_node_mut(owner_id)?.item_mut())
    }
}

// "Helper" functions..
impl HLIR {
    /// Get the source span for a given owning node ID, if it exists.
    #[inline]
    #[must_use]
    pub fn span_by_owner_id(&self, id: OwnerDefId) -> Option<SourceSpan> {
        Some(self.owning_node(id)?.span())
    }

    /// Get the source span for a given [`HirId`], if it exists.
    #[inline]
    #[must_use]
    pub fn span_by_hir_id(&self, id: HirId) -> Option<SourceSpan> {
        Some(self.get_hir_node(id)?.span())
    }
}

impl AsRef<Self> for HLIR {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Debug for HLIR {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HLIR")
            .field("hir_nodes", &self.owner_nodes)
            .finish()
    }
}

/// Contains metadata about the program stored in HIR, such as type information, namespace etc.
#[derive(Debug, Clone)]
pub struct ProgramMetaData {
    /// The program's namespace information, I.e. all defined modules, types, functions, etc.
    pub namespace: Namespace,
    /// The program's type registry, containing all defined types.
    pub type_registry: TypeRegistry,
    /// The program's type map, containing side channel
    /// information about what types a given [`HirId`] evaluates to.
    pub type_map: TypeMap,
}

impl ProgramMetaData {
    /// Find a method in the namespace given its defining path, data type identifier, and method identifier.
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
    #[inline]
    #[must_use]
    pub fn try_type_name(&self, ty: impl Into<Type>) -> Option<String> {
        match ty.into() {
            Type::Unresolved(ty) => ty.get_type_ident(),
            Type::Resolved(KitTy::Abstract(ty_id)) => {
                let abs_ty = self.type_registry.get_from_type_id(ty_id)?;
                let type_path = abs_ty.defined_in.extend_ident(&abs_ty.type_ident.ident);
                Some(type_path.to_string())
            }
            Type::Resolved(kit_ty) => kit_ty.to_type_str(),
        }
    }

    /// Get the type name of a given type, or `"UnknownType"` if it cannot be determined.
    #[inline]
    #[must_use]
    pub fn type_name(&self, ty: impl Into<Type>) -> String {
        self.try_type_name(ty)
            .unwrap_or_else(|| String::from("UnknownType"))
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

/// Lower the output of the parser state to HIR.
/// # Note
/// Unlike `lower_ast_to_hir`, this function does do all HIR processing.
/// (Type checking, resolution, etc).
/// # Errors
/// This function will return an error if any part of the AST cannot be lowered properly.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures,
/// as well as which stage of lowering it occurred in.
pub fn parse_ast_to_hir_processed(ast: &ASTRoot) -> LowerResult<(ProgramMetaData, HLIR)> {
    let mut hlir = lower_ast_to_hir(ast)?;
    let mut meta_data = ProgramMetaData {
        namespace: Namespace::default_root_definition(),
        type_registry: TypeRegistry::default(),
        type_map: TypeMap::default(),
    };

    // Resolution..
    resolve_paths(&mut hlir, &mut meta_data)
        .map_err(|e| LoweringErrorKind::ResolverError(e).with_no_span())?;

    // Type checking..
    run_type_checker(&mut hlir, &mut meta_data)?;

    Ok((meta_data, hlir))
}
