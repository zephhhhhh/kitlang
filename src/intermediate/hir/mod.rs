use ::std::fmt::Debug;

use crate::ast::{ASTRoot, IdentPath, SourceSpan};

use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::nodes::{HIRNode, Item, OwningNode, OwningNodeKind, Type};
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
    pub const PLACEHOLDER_ID: LocalDefId = LocalDefId(u32::MAX);

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
    pub const PLACEHOLDER_ID: OwnerDefId = OwnerDefId(u32::MAX);
    pub const ROOT_NODE: OwnerDefId = OwnerDefId(0);

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
    pub const PLACEHOLDER_ID: DefId = DefId {
        module_id: u32::MAX,
        def_id: LocalDefId::PLACEHOLDER_ID,
    };

    pub fn is_placeholder(self) -> bool {
        self.module_id == u32::MAX || self.def_id.is_placeholder()
    }
}

impl Debug for DefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefId({} : {:?})", self.module_id, self.def_id)
    }
}

/// Definition ID that describes which ID into the modules "path_table" owns the node, and which
/// index into the owners body the node refers to.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HirId {
    pub owner: OwnerDefId,
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
    pub const PLACEHOLDER_ID: HirId = HirId {
        owner: OwnerDefId::PLACEHOLDER_ID,
        id: LocalDefId::PLACEHOLDER_ID,
    };

    pub fn is_placeholder(self) -> bool {
        self.owner.is_placeholder() || self.id.is_placeholder()
    }
}

impl HirId {
    pub fn next_id(mut self) -> Self {
        self.id = LocalDefId(self.id.0 + 1);
        self
    }
}

impl Debug for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HirId({:?} : {:?})", self.owner, self.id)
    }
}

#[derive(Default, Clone)]
pub struct HLIR {
    pub owner_nodes: Vec<OwningNode>,
}

impl HLIR {
    pub fn owner_node_count(&self) -> u32 {
        self.owner_nodes.len() as u32
    }

    pub fn owner_id_iter(&self) -> impl Iterator<Item = OwnerDefId> {
        (0..self.owner_node_count()).map(OwnerDefId)
    }

    pub fn root_module(&self) -> Option<&OwningNode> {
        self.owning_node(OwnerDefId::ROOT_NODE)
    }

    pub fn root_module_mut(&mut self) -> Option<&mut OwningNode> {
        self.owning_node_mut(OwnerDefId::ROOT_NODE)
    }

    pub fn owning_node(&self, id: OwnerDefId) -> Option<&OwningNode> {
        self.owner_nodes.get(id.0 as usize)
    }

    pub fn owning_node_mut(&mut self, id: OwnerDefId) -> Option<&mut OwningNode> {
        self.owner_nodes.get_mut(id.0 as usize)
    }

    pub fn owning_node_unchecked(&self, id: OwnerDefId) -> &OwningNode {
        self.owner_nodes.get(id.0 as usize).expect("Node exists.")
    }

    pub fn owning_node_mut_unchecked(&mut self, id: OwnerDefId) -> &mut OwningNode {
        self.owner_nodes
            .get_mut(id.0 as usize)
            .expect("Node exists.")
    }

    pub fn next_owner_id(&self) -> OwnerDefId {
        OwnerDefId(self.owner_node_count())
    }

    pub fn insert_owning_node(&mut self, mut owner_node: OwningNode) -> OwnerDefId {
        let next_id = self.next_owner_id();
        owner_node.set_owner_id(next_id);
        self.owner_nodes.push(owner_node);
        next_id
    }

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

    fn update_parent_item_list(&mut self, new_node: OwnerDefId, parent_id: OwnerDefId) {
        if let Some(parent) = self.owning_node_mut(parent_id)
            && let Some(parent_module) = parent.hir_module_mut()
        {
            parent_module.item_ids.push(new_node);
        }
    }
}

impl HLIR {
    pub fn get_hir_node(&self, hir_id: HirId) -> Option<&HIRNode> {
        self.owning_node(hir_id.owner)?.get_hir_node(hir_id.id)
    }

    pub fn get_hir_node_unchecked(&self, hir_id: HirId) -> &HIRNode {
        self.owning_node_unchecked(hir_id.owner)
            .get_hir_node(hir_id.id)
            .expect("HIRNode exists.")
    }

    pub fn get_hir_node_mut(&mut self, hir_id: HirId) -> Option<&mut HIRNode> {
        self.owning_node_mut(hir_id.owner)?
            .get_hir_node_mut(hir_id.id)
    }

    pub fn get_hir_node_mut_unchecked(&mut self, hir_id: HirId) -> &mut HIRNode {
        self.owning_node_mut_unchecked(hir_id.owner)
            .get_hir_node_mut(hir_id.id)
            .expect("HIRNode exists.")
    }

    pub fn next_hir_id_on(&self, owner_id: impl Into<OwnerDefId>) -> HirId {
        if let Some(owner_node) = self.owning_node(owner_id.into()) {
            owner_node.next_hir_id()
        } else {
            HirId::PLACEHOLDER_ID
        }
    }

    pub fn insert_hir_node(
        &mut self,
        owner_id: impl Into<OwnerDefId>,
        hir_node: HIRNode,
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
    pub fn owning_node_item(&self, owner_id: OwnerDefId) -> Option<&Item> {
        self.owning_node(owner_id)?.item()
    }

    pub fn owning_node_item_mut(&mut self, owner_id: OwnerDefId) -> Option<&mut Item> {
        self.owning_node_mut(owner_id)?.item_mut()
    }
}

// "Helper" functions..
impl HLIR {
    pub fn span_by_owner_id(&self, id: OwnerDefId) -> Option<SourceSpan> {
        self.owning_node(id)?.span()
    }

    pub fn span_by_hir_id(&self, id: HirId) -> Option<SourceSpan> {
        Some(self.get_hir_node(id)?.span())
    }
}

impl AsRef<HLIR> for HLIR {
    fn as_ref(&self) -> &HLIR {
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
    pub namespace: Namespace,
    pub type_registry: TypeRegistry,
    pub type_map: TypeMap,
}

impl ProgramMetaData {
    #[inline]
    pub fn find_method(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<&Namespace> {
        self.namespace
            .find_method(defined_in, data_type_ident, method_ident)
    }

    #[inline]
    pub fn find_method_owner_def(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<OwnerDefId> {
        self.namespace
            .find_method_owner_def(defined_in, data_type_ident, method_ident)
    }

    #[inline]
    pub fn find_adt_method_owner_def(
        &self,
        adt: &ADTTypeInfo,
        method_ident: &str,
    ) -> Option<OwnerDefId> {
        self.find_method_owner_def(&adt.defined_in, adt.type_ident.str(), method_ident)
    }

    #[inline]
    pub fn find_ty_method_owner_def(&self, ty: Type, method_ident: &str) -> Option<OwnerDefId> {
        let Type::Resolved(resolved_ty) = ty else {
            return None;
        };
        match resolved_ty {
            KitTy::Abstract(adt_id) => {
                let adt = self.type_registry.get_from_type_id(adt_id)?;
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

    /// Get the type name of a given type, or "UnknownType" if it cannot be determined.
    #[inline]
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
        .map(|node| node.span())
        .unwrap_or_else(SourceSpan::null_span)
}

// Outward facing API..

/// Lower the output of the parser stage to HIR.
/// # Note
/// This function does not do any later processing.
pub fn lower_ast_to_hir(ast: &ASTRoot) -> LowerResult<HLIR> {
    lowerer::lower_ast_to_hir(ast)
}

/// Lower the output of the parser state to HIR.
/// # Note
/// Unlike `lower_ast_to_hir`, this function does do all HIR processing.
/// (Type checking, resolution, etc).
pub fn parse_ast_to_hir_processed(ast: &ASTRoot) -> LowerResult<(ProgramMetaData, HLIR)> {
    let mut hlir = lower_ast_to_hir(ast)?;

    // Resolution..
    let (namespaces, type_registry) = match resolve_paths(&mut hlir) {
        Ok((ns, tr)) => (ns, tr),
        Err(e) => return Err(LoweringErrorKind::ResolverError(e).with_no_span()),
    };

    // Type checking..
    let type_map = run_type_checker(&mut hlir, &type_registry, &namespaces)?;

    let meta_data = ProgramMetaData {
        namespace: namespaces,
        type_registry,
        type_map,
    };

    Ok((meta_data, hlir))
}
