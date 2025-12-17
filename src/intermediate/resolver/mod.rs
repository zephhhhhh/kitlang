pub mod errors;

// Resolvers..
mod associated_references;
mod locals;
mod types;
mod verifier;

use std::collections::HashMap;
use std::fmt::Debug;

use crate::ast::{IdentPath, IdentPathSegment, SpannedIdent, Visibility};

use crate::intermediate::hir::nodes::{ResolvedID, Type};
use crate::intermediate::hir::{HLIR, OwnerDefId};
use crate::intermediate::resolver::errors::ResolveResult;
use crate::intermediate::types::KitTy;
use crate::intermediate::hir::ProgramMetaData;

// use crate::intermediate::types::KitTy;

#[derive(Clone, Debug, PartialEq)]
pub struct ADTStructField {
    pub ident: SpannedIdent,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ADTKind {
    Struct(Vec<ADTStructField>),
}

impl ADTKind {
    #[allow(dead_code)]
    pub fn is_struct(&self) -> bool {
        matches!(self, ADTKind::Struct(_))
    }
}

#[derive(Clone, PartialEq)]
pub struct ADTTypeInfo {
    pub owner_id: OwnerDefId,
    pub type_id: TypeID,
    pub kind: ADTKind,
    pub defined_in: IdentPath,
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
    pub fn new_struct(
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

    pub fn full_path(&self) -> IdentPath {
        self.defined_in.extend_ident(&self.type_ident.ident())
    }

    pub fn get_fields(&self) -> &[ADTStructField] {
        match &self.kind {
            ADTKind::Struct(fields) => fields,
        }
    }

    pub fn get_fields_mut(&mut self) -> &mut [ADTStructField] {
        match &mut self.kind {
            ADTKind::Struct(fields) => fields,
        }
    }

    pub fn get_field_count(&self) -> usize {
        self.get_fields().len()
    }

    pub fn find_field_index(&self, field_name: &str) -> Option<usize> {
        for (index, field) in self.get_fields().iter().enumerate() {
            if field.ident.str() == field_name {
                return Some(index);
            }
        }
        None
    }

    pub fn get_field_by_ident(&self, field_name: &str) -> Option<&ADTStructField> {
        self.get_fields()
            .iter()
            .find(|f| f.ident.str() == field_name)
    }

    pub fn get_field_by_ident_mut(&mut self, field_name: &str) -> Option<&mut ADTStructField> {
        self.get_fields_mut()
            .iter_mut()
            .find(|f| f.ident.str() == field_name)
    }
}

pub type TypeID = usize;
const PLACEHOLDER_TYPE_ID: TypeID = usize::MAX;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct TypeRegistry {
    all_paths: Vec<(IdentPath, TypeID)>,
    lut: HashMap<IdentPath, TypeID>,
    store: Vec<ADTTypeInfo>,
}

impl TypeRegistry {
    pub fn register_adt(&mut self, mut info: ADTTypeInfo) -> TypeID {
        let full_path = info.defined_in.extend_ident(&info.type_ident.ident());

        let type_id = self.store.len();
        info.type_id = type_id;
        self.store.push(info);

        self.lut.insert(full_path.clone(), type_id);
        self.all_paths.push((full_path, type_id));

        type_id
    }

    pub fn get_from_type_id(&self, id: TypeID) -> Option<&ADTTypeInfo> {
        self.store.get(id)
    }

    pub fn get_from_type_id_mut(&mut self, id: TypeID) -> Option<&mut ADTTypeInfo> {
        self.store.get_mut(id)
    }

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

    pub fn adt_types(&self) -> &[ADTTypeInfo] {
        &self.store
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceKind {
    Module,
    Function,
    Constant,
    Struct(TypeID),
    Builtin,
    Enum,
    Use(IdentPath),
}

impl NamespaceKind {
    #[allow(dead_code)]
    pub fn is_resolvable_type(&self) -> bool {
        !matches!(self, NamespaceKind::Module)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Namespace {
    pub ident: String,
    pub kind: NamespaceKind,
    pub items: HashMap<String, Namespace>,
    pub id: ResolvedID,
    pub vis: Visibility,
    pub local: bool,
}

impl Namespace {
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
    #[inline]
    pub fn get(&self, ident: &str) -> Option<&Namespace> {
        self.items.get(ident)
    }

    #[inline]
    pub fn get_mut(&mut self, ident: &str) -> Option<&mut Namespace> {
        self.items.get_mut(ident)
    }

    #[inline]
    pub fn insert(&mut self, namespace: Namespace) {
        self.items.insert(namespace.ident.clone(), namespace);
    }

    #[inline]
    pub fn is_module(&self) -> bool {
        self.kind == NamespaceKind::Module
    }

    #[allow(dead_code)]
    pub fn is_resolvable_type(&self) -> bool {
        self.kind.is_resolvable_type()
    }

    /// Track backwards from the path, finding the "deepest" enclosing scope of the path.
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
    pub fn find_previous_enclosing_scope(&self, path: &IdentPath) -> IdentPath {
        self.try_find_previous_enclosing_scope_impl(path)
            .unwrap_or_else(|| IdentPath::new_empty(true))
    }

    /// Track backwards from the path, finding the "deepest" module of the path.
    pub fn find_previous_module(&self, path: &IdentPath) -> IdentPath {
        self.try_find_previous_module_impl(path)
            .unwrap_or_else(|| IdentPath::new_empty(true))
    }

    pub fn find_previous_module_from(&self, base: &IdentPath, path: &IdentPath) -> IdentPath {
        self.find_previous_module(&path.rebase_from_path(base))
    }

    pub fn find_definition_from_segments(&self, path: &[IdentPathSegment]) -> Option<&Namespace> {
        let mut curr_namespace = self;
        for segment in path {
            curr_namespace = curr_namespace.get(segment)?;
        }
        Some(curr_namespace)
    }

    pub fn find_definition(&self, path: &IdentPath) -> Option<&Namespace> {
        self.find_definition_from_segments(path.segments())
    }

    pub fn find_definition_from(&self, base: &IdentPath, path: &IdentPath) -> Option<&Namespace> {
        let final_path = path.rebase_from_path(base);
        self.find_definition_from_segments(final_path.segments())
    }

    pub fn find_method(
        &self,
        defined_in: &IdentPath,
        data_type_ident: &str,
        method_ident: &str,
    ) -> Option<&Namespace> {
        self.find_definition(defined_in)?
            .items
            .get(data_type_ident)?
            .items
            .get(method_ident)
    }

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

pub fn resolve_paths(hlir: &mut HLIR, meta_data: &mut ProgramMetaData) -> ResolveResult<()> {
    locals::resolve_scope_paths(hlir)?;
    associated_references::resolve_associated_references(hlir, meta_data)?;

    types::resolve_types(hlir, meta_data)?;

    // Verify all references have been resolved.
    verifier::verify_references(hlir)?;

    Ok(())
}
