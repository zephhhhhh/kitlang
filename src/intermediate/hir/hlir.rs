use std::fmt::Debug;

use crate::ast::SourceSpan;
use crate::intermediate::hir::nodes::{HirNode, Item, OwningNode, OwningNodeKind, VarBinding};
use crate::intermediate::hir::{HirId, OwnerDefId};

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
    /// Get the next available [`HirId`] for a specified owner.
    #[inline]
    #[must_use]
    pub fn next_hir_id_on(&self, owner_id: impl Into<OwnerDefId>) -> HirId {
        self.owning_node(owner_id.into())
            .map_or(HirId::PLACEHOLDER_ID, OwningNode::next_hir_id)
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

// Disjoint..

/// The purpose of this is to allow multiple mutable references to the individual disjoint nodes in
/// the HLIR representation.
/// # Safety
/// This is safe as long as no new nodes are added in any form, so we only mutate already existing
/// nodes, and no references are stored beyond the lifetime of 'a.
///
/// Therefore this struct offers no way to access the underlying HLIR directly, and only exposes
/// methods for accessing already existing nodes.
///
/// Since we borrow [`HLIR`] mutably for this, we can guarantee that we are the only reference to
/// the [`HLIR`] and so therefore this is no way for new nodes to be added, guaranteeing the safety
/// of this.
pub struct HLIRDisjointMut<'a> {
    hlir: &'a mut HLIR,
}

impl<'a> HLIRDisjointMut<'a> {
    #[inline]
    pub const fn new(hlir: &'a mut HLIR) -> Self {
        Self { hlir }
    }

    #[inline]
    #[must_use]
    pub const fn nonmut_ref<'b>(&'a self) -> &'b HLIR
    where
        'a: 'b,
    {
        self.hlir
    }
}

impl AsRef<HLIR> for HLIRDisjointMut<'_> {
    fn as_ref(&self) -> &HLIR {
        self.nonmut_ref()
    }
}

/// This is a wrapper around a pointer, this purpose of this is to allow methods on
/// [`HLIRDisjointMut`] to return a value, and not a reference with a lifetime, this means the
/// mutable borrow of [`HLIRDisjointMut`] when querying is only held for the duration of the
/// function call, and isn't held afterwards, preventing more mutable borrows from being made.
#[derive(Clone)]
pub struct DisjointMut<T> {
    v: *mut T,
}

impl<T> std::ops::Deref for DisjointMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.v }
    }
}

impl<T> std::ops::DerefMut for DisjointMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.v }
    }
}

impl<T> DisjointMut<T> {
    pub const fn from_mut_ref(v: &mut T) -> Self {
        Self {
            v: std::ptr::from_mut(v),
        }
    }

    #[inline]
    #[must_use]
    pub fn value(&self) -> &'static T {
        unsafe { &*self.v }
    }

    #[inline]
    #[must_use]
    pub fn value_mut(&self) -> &'static mut T {
        unsafe { &mut *self.v }
    }
}

/// This is a wrapper around a pointer, this purpose of this is to allow methods on
/// [`HLIRDisjointMut`] to return a value, and not a reference with a lifetime, this means the
/// mutable borrow of [`HLIRDisjointMut`] when querying is only held for the duration of the
/// function call, and isn't held afterwards, preventing more mutable borrows from being made.
#[derive(Clone)]
pub struct Disjoint<T> {
    v: *const T,
}

impl<T> std::ops::Deref for Disjoint<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.v }
    }
}

impl<T> Disjoint<T> {
    pub const fn from_ref(v: &T) -> Self {
        Self {
            v: std::ptr::from_ref::<T>(v),
        }
    }

    #[inline]
    #[must_use]
    pub fn value(&self) -> &'static T {
        unsafe { &*self.v }
    }
}

// pub type DisjointHIRNode = DisjointMut<HirNode>;
// pub type DisjointOwningNode = DisjointMut<OwningNode>;
// pub type DisjointItem = DisjointMut<Item>;

// pub type DisjointConstHIRNode = Disjoint<HirNode>;
// pub type DisjointConstOwningNode = Disjoint<OwningNode>;
// pub type DisjointConstItem = Disjoint<Item>;

// "Common" API

pub trait HLIRExt<'a, 'b> {
    /// Get the total count of owner nodes in the HIR.
    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    fn owner_node_count(&self) -> u32;

    /// Get a reference to the owning node with a specified [`OwnerDefId`].
    #[must_use]
    fn owning_node(&'a self, id: OwnerDefId) -> Option<&'b OwningNode>;

    /// Get a mutable reference to the owning node with a specified [`OwnerDefId`].
    #[must_use]
    fn owning_node_mut(&'a mut self, id: OwnerDefId) -> Option<&'b mut OwningNode>;

    /// Get a reference to the owning node without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[must_use]
    fn owning_node_unchecked(&'a self, id: OwnerDefId) -> &'b OwningNode;

    /// Get a mutable reference to the owning node without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[must_use]
    fn owning_node_mut_unchecked(&'a mut self, id: OwnerDefId) -> &'b mut OwningNode;

    // Derived implementations..

    /// Get the next available owner ID for inserting a new owning node.
    #[inline]
    #[must_use]
    fn next_owner_id(&self) -> OwnerDefId {
        OwnerDefId(self.owner_node_count())
    }

    /// Get a reference to the root module owning node.
    #[inline]
    #[must_use]
    fn root_module(&'a self) -> Option<&'b OwningNode> {
        self.owning_node(OwnerDefId::ROOT_NODE)
    }

    /// Get a mutable reference to the root module owning node.
    #[inline]
    #[must_use]
    fn root_module_mut(&'a mut self) -> Option<&'b mut OwningNode> {
        self.owning_node_mut(OwnerDefId::ROOT_NODE)
    }

    /// Get an iterator over all owner IDs in the HIR.
    fn owner_id_iter(&self) -> impl Iterator<Item = OwnerDefId> {
        (0..self.owner_node_count()).map(OwnerDefId)
    }

    /// Get a reference to the [`HirNode`] with a specified [`HirId`].
    #[inline]
    #[must_use]
    fn get_hir_node(&'a self, hir_id: HirId) -> Option<&'b HirNode> {
        self.owning_node(hir_id.owner)?.get_hir_node(hir_id.id)
    }

    /// Get a mutable reference to the [`HirNode`] with a specified [`HirId`].
    #[inline]
    #[must_use]
    fn get_hir_node_mut(&'a mut self, hir_id: HirId) -> Option<&'b mut HirNode> {
        self.owning_node_mut(hir_id.owner)?
            .get_hir_node_mut(hir_id.id)
    }

    /// Get the source span for a given owning node ID, if it exists.
    #[inline]
    #[must_use]
    fn span_by_owner_id(&'a self, id: OwnerDefId) -> Option<SourceSpan> {
        Some(self.owning_node(id)?.span())
    }

    /// Get the source span for a given [`HirId`], if it exists.
    #[inline]
    #[must_use]
    fn span_by_hir_id(&'a self, id: HirId) -> Option<SourceSpan> {
        Some(self.get_hir_node(id)?.span())
    }

    /// Get a reference to a variable binding by its [`HirId`],
    /// if the node exists and is a binding.
    #[inline]
    #[must_use]
    fn binding_by_id(&'a self, id: HirId) -> Option<&'b VarBinding> {
        let node = self.get_hir_node(id)?;
        match node {
            HirNode::Binding(binding) => Some(binding),
            _ => None,
        }
    }

    /// Get a reference to the owning node's item with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    fn owning_node_item(&'a self, owner_id: OwnerDefId) -> Option<&'b Item> {
        Some(self.owning_node(owner_id)?.item())
    }

    /// Get a mutable reference to the owning node's item with a specified [`OwnerDefId`].
    #[inline]
    #[must_use]
    fn owning_node_item_mut(&'a mut self, owner_id: OwnerDefId) -> Option<&'b mut Item> {
        Some(self.owning_node_mut(owner_id)?.item_mut())
    }

    /// Get a reference to the owning node's item, cast to type `F`, (if possible) with a specified [`OwnerDefId`].
    #[inline]
    fn owning_node_item_as<F>(&'a self, owner_id: OwnerDefId) -> Option<&'b F>
    where
        Option<&'b F>: From<&'b OwningNode>,
    {
        self.owning_node(owner_id)
            .and_then(std::convert::Into::into)
    }

    /// Get a mutable reference to the owning node's item, cast to type `F`, (if possible) with a specified [`OwnerDefId`].
    #[inline]
    fn owning_node_item_mut_as<F>(&'a mut self, owner_id: OwnerDefId) -> Option<&'b mut F>
    where
        Option<&'b mut F>: From<&'b mut OwningNode>,
    {
        self.owning_node_mut(owner_id)
            .and_then(std::convert::Into::into)
    }

    /// Get a reference to the [`HirNode`] with a specified [`HirId`] without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    fn get_hir_node_unchecked(&'a self, hir_id: HirId) -> &'b HirNode {
        self.owning_node_unchecked(hir_id.owner)
            .get_hir_node(hir_id.id)
            .expect("HIRNode exists.")
    }

    /// Get a mutable reference to the [`HirNode`] with a specified [`HirId`] without checking for existence.
    /// # Panics
    /// Panics if the node does not exist.
    #[inline]
    #[must_use]
    fn get_hir_node_mut_unchecked(&'a mut self, hir_id: HirId) -> &'b mut HirNode {
        self.owning_node_mut_unchecked(hir_id.owner)
            .get_hir_node_mut(hir_id.id)
            .expect("HIRNode exists.")
    }

    /// Get a reference to the [`HirNode`] with a specified [`HirId`], casted to type `F`.
    #[inline]
    fn get_hir_node_as<F>(&'a self, id: HirId) -> Option<&'b F>
    where
        Option<&'b F>: From<&'b HirNode>,
    {
        self.get_hir_node(id).and_then(std::convert::Into::into)
    }

    /// Get a mutable reference to the [`HirNode`] with a specified [`HirId`], casted to type `F`.
    #[inline]
    fn get_hir_node_mut_as<F>(&'a mut self, id: HirId) -> Option<&'b mut F>
    where
        Option<&'b mut F>: From<&'b mut HirNode>,
    {
        self.get_hir_node_mut(id).and_then(std::convert::Into::into)
    }
}

impl<'a, 'b> HLIRExt<'a, 'b> for HLIR
where
    'a: 'b,
{
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn owner_node_count(&self) -> u32 {
        self.owner_nodes.len() as u32
    }

    #[inline]
    fn owning_node(&'a self, id: OwnerDefId) -> Option<&'b OwningNode> {
        self.owner_nodes.get(id.0 as usize)
    }

    #[inline]
    fn owning_node_mut(&'a mut self, id: OwnerDefId) -> Option<&'b mut OwningNode> {
        self.owner_nodes.get_mut(id.0 as usize)
    }

    #[inline]
    fn owning_node_unchecked(&'a self, id: OwnerDefId) -> &'b OwningNode {
        unsafe { self.owner_nodes.get_unchecked(id.0 as usize) }
    }

    #[inline]
    fn owning_node_mut_unchecked(&'a mut self, id: OwnerDefId) -> &'b mut OwningNode {
        unsafe { self.owner_nodes.get_unchecked_mut(id.0 as usize) }
    }
}

impl<'a> HLIRExt<'a, 'static> for HLIRDisjointMut<'_> {
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn owner_node_count(&self) -> u32 {
        self.hlir.owner_node_count()
    }

    #[inline]
    fn owning_node(&'a self, id: OwnerDefId) -> Option<&'static OwningNode> {
        let disjoint = Disjoint::<OwningNode>::from_ref(self.hlir.owning_node(id)?);
        Some(disjoint.value())
    }

    #[inline]
    fn owning_node_mut(&'a mut self, id: OwnerDefId) -> Option<&'static mut OwningNode> {
        let disjoint = DisjointMut::<OwningNode>::from_mut_ref(self.hlir.owning_node_mut(id)?);
        Some(disjoint.value_mut())
    }

    #[inline]
    fn owning_node_unchecked(&'a self, id: OwnerDefId) -> &'static OwningNode {
        let disjoint = Disjoint::<OwningNode>::from_ref(unsafe {
            self.hlir.owner_nodes.get_unchecked(id.0 as usize)
        });
        disjoint.value()
    }

    #[inline]
    fn owning_node_mut_unchecked(&'a mut self, id: OwnerDefId) -> &'static mut OwningNode {
        let disjoint = DisjointMut::<OwningNode>::from_mut_ref(unsafe {
            self.hlir.owner_nodes.get_unchecked_mut(id.0 as usize)
        });
        disjoint.value_mut()
    }
}
