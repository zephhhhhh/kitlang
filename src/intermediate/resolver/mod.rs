//! The resolver module contains multiple resolvers that are each responsible for resolving different kinds
//! of references within a Kitlang program.
//!
//! Primarily this consists of resolving some sort of `path` (e.g., a `x`, or `some_function`) to an `ID`
//! into the [`HLIR`] node structure.
//!
//! There are seperate resolvers for:
//! * Resolving type references (e.g., struct types, builtin types, etc), from a [`RefPath`] to a [`TypeID`].
//! * Resolving local variable references within functions, from a [`RefPath`] to a [`HirId`].
//! * Resolving associated references, such as method calls and field accesses, from a [`RefPath`] to a [`HirId`].
//!
//! After all resolvers have ran, there is a verification path which checks all `path` references in the [`HLIR`]
//! have been resolved to an ID, and produces errors for any that remain unresolved.
//!
//! Any errors that occur during resolution are aggregated and returned as one error containing all resolution failures.

pub mod errors;

use itertools::Itertools;

#[cfg(doc)]
use crate::intermediate::hir::HirId;
#[cfg(doc)]
use crate::intermediate::hir::nodes::RefPath;

// Resolvers..
mod associated_references;
mod locals;
mod types;
mod verifier;

use std::collections::HashMap;
use std::fmt::Debug;

use crate::ast::{IdentPath, IdentPathSegment, SpannedIdent, Visibility};

use crate::intermediate::hir::ProgramMetaData;
use crate::intermediate::hir::nodes::{ResolvedID, Type};
use crate::intermediate::hir::{HLIR, OwnerDefId};
use crate::intermediate::resolver::errors::ResolveResult;
use crate::intermediate::types::KitTy;

/// Represents a field in a struct ADT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ADTStructField {
    /// The identifier and span of the field definition.
    pub ident: SpannedIdent,
    /// The [`Type`] of the field.
    pub ty: Type,
}

/// Represents the kind of an Abstract Data Type (ADT) / user-defined type.
/// Currently, only structs are supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ADTKind {
    Struct(Vec<ADTStructField>),
}

impl ADTKind {
    /// Checks if the ADT kind is a struct.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn is_struct(&self) -> bool {
        matches!(self, Self::Struct(_))
    }
}

/// Represents information about an Abstract Data Type (ADT) / user-defined type.
#[derive(Clone, PartialEq, Eq)]
pub struct ADTTypeInfo {
    /// The owner definition ID of the ADT.
    pub owner_id: OwnerDefId,
    /// The unique [`TypeID`] of the ADT.
    pub type_id: TypeID,
    /// The kind of the ADT (e.g., struct).
    pub kind: ADTKind,
    /// The path where the ADT is defined, excluding the type identifier.
    /// # Example
    /// ```ignore
    /// mod example {
    ///    struct ExampleStruct { ... }
    /// }
    /// ```
    /// Would be defined in `::example`
    pub defined_in: IdentPath,
    /// The identifier and span of the ADT type.
    /// # Example
    /// ```ignore
    /// mod example {
    ///    struct ExampleStruct { ... }
    /// }
    /// ```
    /// Would be defined in have the `type_ident` of `ExampleStruct`
    pub type_ident: SpannedIdent,
}

impl Debug for ADTTypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ADTTypeInfo")
            .field("owner_id", &self.owner_id)
            .field("type_id", &self.type_id)
            .field("kind", &self.kind)
            .field("defined_in", &self.defined_in.to_string())
            .field("type_ident", &self.type_ident)
            .field("full_path", &self.full_path().to_string())
            .finish()
    }
}

impl ADTTypeInfo {
    /// Creates a new [`ADTTypeInfo`] for a struct type.
    /// # Parameters
    /// - `owner_id`: The owner definition ID of the ADT.
    /// - `defined_in`: The path where the ADT is defined, excluding the type identifier.
    /// - `type_ident`: The identifier and span of the ADT type.
    /// - `fields`: The fields of the struct.
    /// # Returns
    /// A new instance of [`ADTTypeInfo`] representing the struct type.
    #[inline]
    #[must_use]
    pub const fn new_struct(
        owner_id: OwnerDefId,
        defined_in: IdentPath,
        type_ident: SpannedIdent,
        fields: Vec<ADTStructField>,
    ) -> Self {
        Self {
            owner_id,
            type_id: PLACEHOLDER_TYPE_ID,
            kind: ADTKind::Struct(fields),
            defined_in,
            type_ident,
        }
    }

    /// Gets the full path of the ADT by extending the defined-in path with the type identifier.
    #[inline]
    #[must_use]
    pub fn full_path(&self) -> IdentPath {
        self.defined_in.extend_ident(&self.type_ident.ident())
    }

    /// Gets a slice of the fields of the struct ADT.
    #[inline]
    #[must_use]
    pub fn get_fields(&self) -> &[ADTStructField] {
        match &self.kind {
            ADTKind::Struct(fields) => fields,
        }
    }

    /// Gets a mutable slice of the fields of the struct ADT.
    #[inline]
    #[must_use]
    pub fn get_fields_mut(&mut self) -> &mut [ADTStructField] {
        match &mut self.kind {
            ADTKind::Struct(fields) => fields,
        }
    }

    /// Gets the number of fields in the struct ADT.
    #[inline]
    #[must_use]
    pub fn get_field_count(&self) -> usize {
        self.get_fields().len()
    }

    /// Finds the index of a field by its name.
    #[inline]
    #[must_use]
    pub fn find_field_index(&self, field_name: &str) -> Option<usize> {
        for (index, field) in self.get_fields().iter().enumerate() {
            if field.ident.str() == field_name {
                return Some(index);
            }
        }
        None
    }

    /// Gets a reference to a field by its index.
    #[inline]
    #[must_use]
    pub fn get_field(&self, index: usize) -> Option<&ADTStructField> {
        self.get_fields().get(index)
    }

    /// Gets a mutable reference to a field by its index.
    #[inline]
    #[must_use]
    pub fn get_field_mut(&mut self, index: usize) -> Option<&mut ADTStructField> {
        self.get_fields_mut().get_mut(index)
    }

    /// Gets a reference to a field by its name.
    #[inline]
    #[must_use]
    pub fn get_field_by_ident(&self, field_name: &str) -> Option<&ADTStructField> {
        self.get_fields()
            .iter()
            .find(|f| f.ident.str() == field_name)
    }

    /// Gets a mutable reference to a field by its name.
    #[inline]
    #[must_use]
    pub fn get_field_by_ident_mut(&mut self, field_name: &str) -> Option<&mut ADTStructField> {
        self.get_fields_mut()
            .iter_mut()
            .find(|f| f.ident.str() == field_name)
    }
}

/// A unique identifier for a type in the [`TypeRegistry`].
/// This is simply an index into the [`TypeRegistry`]'s internal storage.
pub type TypeID = usize;

/// A placeholder [`TypeID`] used during type registration.
const PLACEHOLDER_TYPE_ID: TypeID = usize::MAX;

/// A registry for managing Abstract Data Types (ADTs) / user-defined types.
/// This manages all [`ADTTypeInfo`] instances and provides lookup functionality based on paths.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct TypeRegistry {
    /// A mapping between an [`IdentPath`] and a [`TypeID`].
    lut: HashMap<IdentPath, TypeID>,
    /// All the registered ADT type information.
    /// The index in this vector corresponds to the [`TypeID`].
    store: Vec<ADTTypeInfo>,
}

impl TypeRegistry {
    /// Registers a new ADT type in the registry.
    /// # Returns
    /// The assigned [`TypeID`] for the registered ADT type.
    #[inline]
    #[must_use]
    pub fn register_adt(&mut self, mut info: ADTTypeInfo) -> TypeID {
        let full_path = info.defined_in.extend_ident(&info.type_ident.ident());

        let type_id = self.store.len();
        info.type_id = type_id;
        self.store.push(info);

        self.lut.insert(full_path.clone(), type_id);

        type_id
    }

    /// Get a reference to the ADT type information by its [`TypeID`], if it exists.
    #[inline]
    #[must_use]
    pub fn get_from_type_id(&self, id: TypeID) -> Option<&ADTTypeInfo> {
        self.store.get(id)
    }

    /// Get a mutable reference to the ADT type information by its [`TypeID`], if it exists.
    #[inline]
    #[must_use]
    pub fn get_from_type_id_mut(&mut self, id: TypeID) -> Option<&mut ADTTypeInfo> {
        self.store.get_mut(id)
    }

    /// Finds a type in the registry based on its path.
    /// This will return primitive types if the path corresponds to a primitive type.
    #[inline]
    #[must_use]
    pub fn find_type_from_path(&self, path: &IdentPath) -> Option<KitTy> {
        if path.len() == 1
            && path.is_root_relative()
            && let Some(t) = KitTy::from_primitive_ty_str(&path.segments()[0])
        {
            Some(t)
        } else {
            Some(KitTy::Abstract(*self.lut.get(path)?))
        }
    }

    /// Gets a slice of all registered ADT type information.
    #[inline]
    #[must_use]
    pub fn adt_types(&self) -> &[ADTTypeInfo] {
        &self.store
    }

    /// Attempts to get a human-readable type name for the given [`Type`].
    #[inline]
    #[must_use]
    pub fn try_type_name(&self, ty: impl Into<Type>) -> Option<String> {
        match ty.into() {
            Type::Unresolved(ty) => Some(ty.get_type_ident()),
            Type::Resolved(KitTy::Abstract(ty_id)) => {
                let abs_ty = self.get_from_type_id(ty_id)?;
                let type_path = abs_ty.defined_in.extend_ident(&abs_ty.type_ident.ident);
                Some(type_path.to_string())
            }
            Type::Resolved(KitTy::Tuple(ts)) => Some(format!(
                "({})",
                ts.iter()
                    .map(|t| self.type_name(Type::Resolved(t.clone())))
                    .join(", ")
            )),
            Type::Resolved(KitTy::Array(ty, size)) => Some(format!(
                "[{}; {}]",
                self.type_name(Type::Resolved(*ty)),
                size
            )),
            Type::Resolved(KitTy::Slice(ty)) => {
                Some(format!("[{}]", self.type_name(Type::Resolved(*ty))))
            }
            Type::Resolved(kit_ty) => kit_ty.to_type_str(),
        }
    }

    /// Gets a human-readable type name for the given [`Type`], defaulting to `"??"` if it cannot be determined.
    #[inline]
    #[must_use]
    pub fn type_name(&self, ty: impl Into<Type>) -> String {
        self.try_type_name(ty).unwrap_or_else(|| String::from("??"))
    }
}

/// Represents the kind of a namespace item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceKind {
    /// A module namespace, child [`Namespace`]s are items that are defined within the module.
    Module,
    /// A function namespace, child [`Namespace`]s are items that are defined within the function.
    Function,
    /// A constant namespace, this should have no child [`Namespace`]s.
    Constant,
    /// A structure namespace, child [`Namespace`]s are fields defined within the struct.
    Struct(TypeID),
    /// A [`Namespace`] for builtin types, child [`Namespace`]s are methods defined on the builtin type.
    Builtin,
    /// An enumeration namespace, child [`Namespace`]s are variants defined within the enum.
    Enum,
    /// A type alias namespace.
    Use(IdentPath),
}

impl NamespaceKind {
    /// Checks if the [`NamespaceKind`] represents a resolvable type.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn is_resolvable_type(&self) -> bool {
        !matches!(self, Self::Module)
    }

    /// Checks if the namespace kind is a module.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }

    /// Checks if the namespace kind is a function.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_function(&self) -> bool {
        matches!(self, Self::Function)
    }

    /// Checks if the namespace kind is a constant.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Constant)
    }

    /// Checks if the namespace kind is a struct.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_struct(&self) -> bool {
        matches!(self, Self::Struct(..))
    }

    /// Checks if the namespace kind is a builtin type.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(self, Self::Builtin)
    }

    /// Checks if the namespace kind is an enum.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum)
    }

    /// Checks if the namespace kind is a use/import statement.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_use(&self) -> bool {
        matches!(self, Self::Use(..))
    }
}

/// Represents a [`Namespace`] in the intermediate representation.
/// A [`Namespace`] can represent modules, functions, structs, enums, etc.
/// It contains child [`Namespace`]s for items defined within it.
/// This is used for resolving paths and symbol lookups.
///
/// This is still structured as a tree, but each node contains a [`ResolvedID`],
/// which may be any kind of Hir ID (e.g., [`OwnerDefId`], etc) to the
/// definition that it represents.
///
/// This is essentially for mapping an [`IdentPath`] to a definition ID in the HIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// The identifier of the namespace item.
    pub ident: String,
    /// The kind of the namespace item.
    pub kind: NamespaceKind,
    /// Child namespace items defined within this namespace.
    pub items: HashMap<String, Self>,
    /// The resolved ID of the definition this namespace represents.
    pub id: ResolvedID,
    /// The visibility of the namespace item, I.e., if it is marked `pub` or not.
    pub vis: Visibility,
    /// This should keep track of whether this namespace is defined locally,
    /// I.e., a function defined inside of another function, etc.
    pub local: bool,
}

impl Namespace {
    /// Creates a default root namespace definition,
    /// This is not a `default` implementation, as a `Namespace` should not 'default' to a root definition.
    /// Instead you should prefer to explicitly create `Namespace` with struct initialisation syntax when needed.
    #[inline]
    #[must_use]
    pub fn default_root_definition() -> Self {
        Self {
            ident: "::".to_string(),
            kind: NamespaceKind::Module,
            items: HashMap::new(),
            id: ResolvedID::OwnerDef(OwnerDefId::ROOT_NODE),
            vis: Visibility::Public,
            local: false,
        }
    }
}

impl Namespace {
    /// Get a reference to a child namespace by its identifier, if it exists.
    #[inline]
    #[must_use]
    pub fn get(&self, ident: &str) -> Option<&Self> {
        self.items.get(ident)
    }

    /// Get a mutable reference to a child namespace by its identifier, if it exists.
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, ident: &str) -> Option<&mut Self> {
        self.items.get_mut(ident)
    }

    /// Inserts a child namespace into this namespace.
    #[inline]
    pub fn insert(&mut self, namespace: Self) {
        self.items.insert(namespace.ident.clone(), namespace);
    }

    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub const fn is_resolvable_type(&self) -> bool {
        self.kind.is_resolvable_type()
    }

    /// Track backwards from the path, finding the "deepest" enclosing scope of the path.
    /// Where an enclosing scope is something like a module or function that defines a 'border' of accessibility.
    #[must_use]
    pub fn try_find_previous_enclosing_scope_impl(&self, path: &IdentPath) -> Option<IdentPath> {
        if !path.is_root_relative() {
            return None;
        }

        let path_segments = path.segments();
        for i in (0..path.len()).rev() {
            let new_segs = &path_segments[0..i];
            if matches!(
                self.find_definition_from_segments(new_segs)?.kind,
                NamespaceKind::Module | NamespaceKind::Function
            ) {
                return Some(IdentPath::from_segments_slice(new_segs, true));
            }
        }

        None
    }

    /// Track backwards from the path, finding the "deepest" module of the path.
    #[must_use]
    pub fn try_find_previous_module_impl(&self, path: &IdentPath) -> Option<IdentPath> {
        if !path.is_root_relative() {
            return None;
        }

        let path_segments = path.segments();
        for i in (0..path.len()).rev() {
            let new_segs = &path_segments[0..i];
            if self.find_definition_from_segments(new_segs)?.is_module() {
                return Some(IdentPath::from_segments_slice(new_segs, true));
            }
        }

        None
    }

    /// Track backwards from the path, finding the "deepest" enclosing scope of the path.
    /// Where an enclosing scope is something like a module or function that defines a 'border' of accessibility.
    #[inline]
    #[must_use]
    pub fn find_previous_enclosing_scope(&self, path: &IdentPath) -> IdentPath {
        self.try_find_previous_enclosing_scope_impl(path)
            .unwrap_or_else(|| IdentPath::new_empty(true))
    }

    /// Track backwards from the path, finding the "deepest" module of the path.
    #[inline]
    #[must_use]
    pub fn find_previous_module(&self, path: &IdentPath) -> IdentPath {
        self.try_find_previous_module_impl(path)
            .unwrap_or_else(|| IdentPath::new_empty(true))
    }

    /// Track backwards from the path, finding the "deepest" module of the path.
    /// The path is rebased from the given base path first.
    #[inline]
    #[must_use]
    pub fn find_previous_module_from(&self, base: &IdentPath, path: &IdentPath) -> IdentPath {
        self.find_previous_module(&path.rebase_from_path(base))
    }

    /// Finds a definition in the namespace from a sequence of path segments.
    /// Each segment is looked up in order, starting from the root namespace,
    /// where the next segment searched from the result of the previous segment.
    #[inline]
    #[must_use]
    pub fn find_definition_from_segments(&self, path: &[IdentPathSegment]) -> Option<&Self> {
        let mut curr_namespace = self;
        for segment in path {
            curr_namespace = curr_namespace.get(segment)?;
        }
        Some(curr_namespace)
    }

    /// Finds a definition in the namespace from a full path.
    /// Each segment within the [`IdentPath`] is looked up in order, starting from the root namespace,
    /// where the next segment searched from the result of the previous segment.
    #[inline]
    #[must_use]
    pub fn find_definition(&self, path: &IdentPath) -> Option<&Self> {
        self.find_definition_from_segments(path.segments())
    }

    /// Finds a definition in the namespace from a full path.
    /// Each segment within the [`IdentPath`] is looked up in order, starting from the root namespace,
    /// where the next segment searched from the result of the previous segment.
    /// The path is rebased from the given base path first.
    #[inline]
    #[must_use]
    pub fn find_definition_from(&self, base: &IdentPath, path: &IdentPath) -> Option<&Self> {
        let final_path = path.rebase_from_path(base);
        self.find_definition_from_segments(final_path.segments())
    }

    /// Finds a method in the namespace given its defining path, data type identifier, and method identifier.
    /// # Example
    /// ```ignore
    /// mod example {
    ///     struct ExampleStruct { ... }
    ///
    ///     impl ExampleStruct {
    ///        fn example_method(&self) { ... }
    ///     }
    /// }
    /// ```
    /// To find `example_method`, our parameters would be:
    /// * `defined_in`: `::example`
    /// * `data_type_ident`: `ExampleStruct`
    /// * `method_ident`: `example_method`
    /// # Returns
    /// The method namespace if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_method(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<&Self> {
        self.find_definition(defined_in)?
            .items
            .get(data_type_ident)?
            .items
            .get(method_ident)
    }

    /// Finds a method in the namespace given its defining path, data type identifier, and method identifier.
    /// # Example
    /// ```ignore
    /// mod example {
    ///     struct ExampleStruct { ... }
    ///
    ///     impl ExampleStruct {
    ///        fn example_method(&self) { ... }
    ///     }
    /// }
    /// ```
    /// To find `example_method`, our parameters would be:
    /// * `defined_in`: `::example`
    /// * `data_type_ident`: `ExampleStruct`
    /// * `method_ident`: `example_method`
    /// # Returns
    /// The method [`OwnerDefId`] if found, otherwise `None`.
    #[inline]
    #[must_use]
    pub fn find_method_owner_def(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<OwnerDefId> {
        self.find_method(defined_in, data_type_ident, method_ident)?
            .id
            .owner_def_id()
    }
}

// Bool checks..
impl Namespace {
    /// Checks if the namespace kind is a module.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_module(&self) -> bool {
        self.kind.is_module()
    }

    /// Checks if the namespace kind is a function.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_function(&self) -> bool {
        self.kind.is_function()
    }

    /// Checks if the namespace kind is a constant.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.kind.is_constant()
    }

    /// Checks if the namespace kind is a struct.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_struct(&self) -> bool {
        self.kind.is_struct()
    }

    /// Checks if the namespace kind is a builtin type.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        self.kind.is_builtin()
    }

    /// Checks if the namespace kind is an enum.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_enum(&self) -> bool {
        self.kind.is_enum()
    }

    /// Checks if the namespace kind is a use/import statement.
    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_use(&self) -> bool {
        self.kind.is_use()
    }
}

/// # Errors
/// This function will return an error if any part of the HIR cannot be lowered properly.
/// The returned error will contain a diagnostic message indicating what can't be resolved,
/// why it can't be resolved and where it occured.
pub fn resolve_paths(hlir: &mut HLIR, meta_data: &mut ProgramMetaData) -> ResolveResult<()> {
    locals::resolve_scope_paths(hlir)?;
    associated_references::resolve_associated_references(hlir, meta_data)?;

    types::resolve_types(hlir, meta_data)?;

    // Verify all references have been resolved.
    verifier::verify_references(hlir)?;

    Ok(())
}
