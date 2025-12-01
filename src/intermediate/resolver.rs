use ::std::fmt::Debug;
use std::collections::HashMap;

use crate::ast::{IdentPath, IdentPathSegment, SpannedIdent, SpannedIdentPath, Visibility};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::nodes::{
    ExprKind, Function, HIRNode, LetStatement, Module, Parameter, RefPath, ResolvedID, StructField,
    Type,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitor, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use super::hir::nodes::ItemKind;
use super::types::KitTy;

#[derive(Debug, Clone, PartialEq)]
struct LocalScope {
    pub scope_ident: Option<String>,
    pub definitions: HashMap<String, ResolvedID>,
    pub parent: Option<Box<Self>>,
}

impl LocalScope {
    pub fn new(scope_ident: Option<String>) -> Self {
        Self::new_with_parent(scope_ident, None)
    }

    pub fn new_with_parent(scope_ident: Option<String>, parent: Option<Box<Self>>) -> Self {
        Self {
            scope_ident,
            definitions: HashMap::new(),
            parent,
        }
    }

    pub fn child_scope(&self, scope_ident: Option<String>) -> Self {
        Self::new_with_parent(scope_ident, Some(Box::new(self.clone())))
    }

    #[allow(dead_code)]
    pub fn is_root_scope(&self) -> bool {
        self.parent.is_none()
    }

    pub fn add_definition_unique(&mut self, name: &str, id: ResolvedID) -> bool {
        if self.definitions.contains_key(name) {
            return false;
        }

        self.definitions.insert(name.to_string(), id).is_none()
    }

    /// Add a new definition, redefining the value that was already there, if exists.
    pub fn add_definition_overwrite(&mut self, name: &str, id: ResolvedID) -> bool {
        self.definitions.insert(name.to_string(), id).is_some()
    }

    #[allow(dead_code)]
    pub fn add_definition_result(&mut self, name: &str, id: ResolvedID) -> LowerResult<()> {
        if self.add_definition_unique(name, id) {
            Ok(())
        } else {
            Err(LoweringError::new(
                LoweringErrorKind::VariableAlreadyDefined(name.to_string()),
            ))
        }
    }

    pub fn find_definition(&self, name: &str) -> Option<ResolvedID> {
        if let Some(id) = self.definitions.get(name) {
            Some(*id)
        } else if let Some(parent) = &self.parent {
            parent.find_definition(name)
        } else {
            None
        }
    }
}

struct ScopeResolver {
    pub scope: Vec<LocalScope>,
    pub errors: Vec<LoweringError>,
}

impl ScopeResolver {
    pub fn new() -> Self {
        Self {
            scope: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn push_scope(&mut self, local_scope: LocalScope) {
        self.scope.push(local_scope);
    }

    pub fn pop_scope(&mut self) {
        self.scope.pop();
    }

    pub fn current_scope(&self) -> &LocalScope {
        self.scope.last().expect("Must have valid scope.")
    }

    pub fn current_scope_mut(&mut self) -> &mut LocalScope {
        self.scope.last_mut().expect("Must have valid scope.")
    }

    pub fn push_child_scope(&mut self, scope_ident: Option<String>) {
        self.push_scope(self.current_scope().child_scope(scope_ident))
    }

    pub fn resolve(&mut self, hlir: &mut HLIR) -> LowerResult<()> {
        self.walk_mut(hlir);

        if self.errors.is_empty() {
            Ok(())
        } else {
            // FIXME: This is not good dx.
            Err(self.errors.first().expect("There is an error.").clone())
        }
    }
}

impl HLIRVisitorMut<'_> for ScopeResolver {
    fn visit_block_mut(
        &mut self,
        block: &mut super::hir::nodes::Block,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        if block.root_block {
            self.push_child_scope(None);
            self.super_block_mut(block, hlir);
            self.pop_scope();
        } else {
            self.super_block_mut(block, hlir);
        }
    }

    fn visit_function_param_mut(
        &mut self,
        parameter: &mut Parameter,
        _hlir: &mut HLIRDisjointMut<'_>,
    ) {
        if let Err(e) = self
            .current_scope_mut()
            .add_definition_result(parameter.ident.str(), parameter.id.into())
        {
            self.errors.push(e);
        }
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        let has_body = function.body.is_some();
        if has_body {
            self.push_scope(LocalScope::new(Some(function.ident.string())));
        }
        self.super_function_mut(function, hlir);

        if has_body {
            self.pop_scope();
        }
    }

    fn visit_let_statement_mut(
        &mut self,
        id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        self.current_scope_mut()
            .add_definition_overwrite(let_statement.ident.str(), id.into());

        self.super_let_statement_mut(id, let_statement, hlir);
    }

    fn visit_path_mut(
        &mut self,
        _hir_id: HirId,
        path: &mut RefPath,
        _hlir: &mut HLIRDisjointMut<'_>,
    ) {
        if !path.is_resolved() {
            let ident_to_find = path.ident_path().to_string();
            if let Some(id) = self.current_scope().find_definition(&ident_to_find) {
                path.resolve_to(id);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NamespaceKind {
    Module,
    Function,
    Constant,
    Struct(TypeID),
    Enum,
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
    pub fn is_module(&self) -> bool {
        self.kind == NamespaceKind::Module
    }

    #[allow(dead_code)]
    pub fn is_resolvable_type(&self) -> bool {
        self.kind.is_resolvable_type()
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

    /// Track backwards from the path, finding the "deepest" module of the path.
    pub fn find_previous_module(&self, path: &IdentPath) -> IdentPath {
        self.try_find_previous_module_impl(path)
            .unwrap_or_else(|| IdentPath::new_empty(true))
    }

    pub fn find_previous_module_from(&self, base: &IdentPath, path: &IdentPath) -> IdentPath {
        let Some(final_path) = path.rebase_from_path(base) else {
            return IdentPath::new_empty(true);
        };
        self.find_previous_module(&final_path)
    }

    pub fn find_definition_from_segments(&self, path: &[IdentPathSegment]) -> Option<&Namespace> {
        let mut curr_namespace = self;
        for segment in path {
            curr_namespace = curr_namespace.items.get(segment)?;
        }
        Some(curr_namespace)
    }

    pub fn find_definition(&self, path: &IdentPath) -> Option<&Namespace> {
        self.find_definition_from_segments(path.segments())
    }

    pub fn find_definition_from(&self, base: &IdentPath, path: &IdentPath) -> Option<&Namespace> {
        let final_path = path.rebase_from_path(base)?;
        self.find_definition_from_segments(final_path.segments())
    }
}

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

#[derive(Clone)]
pub struct ADTTypeInfo {
    pub owner_id: OwnerDefId,
    pub type_id: TypeID,
    pub kind: ADTKind,
    pub defined_in: IdentPath,
    pub type_ident: SpannedIdent,
    pub associated_defs: Vec<OwnerDefId>,
}

impl Debug for ADTTypeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ADTTypeInfo")
            .field("owner_id", &self.owner_id)
            .field("type_id", &self.type_id)
            .field("kind", &self.kind)
            .field("defined_in", &self.defined_in.to_string())
            .field("type_ident", &self.type_ident)
            .field("associated_defs", &self.associated_defs)
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
            associated_defs: Vec::new(),
        }
    }

    pub fn full_path(&self) -> IdentPath {
        self.defined_in.extend_ident(&self.type_ident.ident())
    }

    pub fn find_associated_def(&self, hlir: &HLIR, ident: &str) -> Option<OwnerDefId> {
        Some(
            self.associated_defs
                .iter()
                .filter_map(|id| hlir.owning_node(*id)?.item())
                .find(|i| match &i.kind {
                    ItemKind::Module(module) => module.ident.ident().str() == ident,
                    ItemKind::Function(function) => function.ident.str() == ident,
                    ItemKind::Constant(constant) => constant.ident.str() == ident,
                    _ => false,
                })?
                .owner_id,
        )
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

#[derive(Default, Clone, Debug)]
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
        self.store.get(id as usize)
    }

    pub fn get_from_type_id_mut(&mut self, id: TypeID) -> Option<&mut ADTTypeInfo> {
        self.store.get_mut(id as usize)
    }

    pub fn find_type_id_from_path(&self, path: &IdentPath) -> Option<TypeID> {
        self.lut.get(path).cloned()
    }

    pub fn adt_types(&self) -> &[ADTTypeInfo] {
        &self.store
    }
}

struct AssociatedReferenceMapper {
    pub path_stack: Vec<IdentPath>,
    pub impl_path_lut: Vec<(OwnerDefId, IdentPath)>,

    pub active_adt: Option<TypeID>,

    pub type_registry: TypeRegistry,

    pub root_namespace: Namespace,
    pub stage1_complete: bool,

    // TODO: Implement these errors!!
    pub errors: Vec<LoweringError>,
}

impl AssociatedReferenceMapper {
    pub fn new() -> Self {
        Self {
            path_stack: vec![IdentPath::from_segments(Vec::new(), true)],
            impl_path_lut: Vec::new(),
            active_adt: None,
            type_registry: TypeRegistry::default(),
            root_namespace: Namespace::default_root_definition(),
            stage1_complete: false,
            errors: Vec::new(),
        }
    }

    pub fn reset_path_to(&mut self, ident_path: IdentPath) {
        self.path_stack = vec![ident_path];
    }

    pub fn reset_path(&mut self) {
        self.reset_path_to(IdentPath::from_segments(Vec::new(), true));
    }

    pub fn map_references(&mut self, hlir: &mut HLIR) -> LowerResult<(Namespace, TypeRegistry)> {
        // Map all except impls..
        self.visit_root(hlir);
        self.stage1_complete = true;

        // Manually traverse impl items..
        let impls = self.impl_path_lut.clone();
        for (impl_id, impl_path) in impls {
            if let Some(node) = hlir.owning_node(impl_id) {
                if let Some(impl_info) = node.hir_impl_ref() {
                    self.reset_path_to(impl_path);
                    self.visit_impl(impl_info, hlir);
                }
            }
        }

        // Resolve references..
        self.reset_path();
        self.walk_mut(hlir);

        Ok((self.root_namespace.clone(), self.type_registry.clone()))
    }
}

impl AssociatedReferenceMapper {
    pub fn current_path(&self) -> &IdentPath {
        self.path_stack
            .last()
            .expect("Always has atleast 1 in stack.")
    }

    pub fn current_path_mut(&mut self) -> &mut IdentPath {
        self.path_stack
            .last_mut()
            .expect("Always has atleast 1 in stack.")
    }

    pub fn push_to_current_path(&mut self, s: &str) {
        self.current_path_mut().push(s);
    }

    pub fn pop_from_current_path(&mut self) {
        self.current_path_mut().pop();
    }

    pub fn push_stack(&mut self, path: IdentPath) {
        self.path_stack.push(path);
    }

    pub fn pop_stack(&mut self) {
        self.path_stack.pop();
    }
}

impl AssociatedReferenceMapper {
    fn should_be_local(&self, path: &IdentPath) -> bool {
        let Some(namespace) = self.get_namespace(path) else {
            return false;
        };
        namespace.local || namespace.kind == NamespaceKind::Function
    }

    fn get_namespace(&self, path: &IdentPath) -> Option<&Namespace> {
        let mut curr_namespace = &self.root_namespace;
        for segment in path.segments() {
            curr_namespace = curr_namespace.items.get(segment)?;
        }
        Some(curr_namespace)
    }

    fn get_namespace_mut(&mut self, path: &IdentPath) -> Option<&mut Namespace> {
        let mut curr_namespace = &mut self.root_namespace;
        for segment in path.segments() {
            curr_namespace = curr_namespace.items.get_mut(segment)?;
        }
        Some(curr_namespace)
    }
}

impl HLIRVisitor for AssociatedReferenceMapper {
    fn visit_module(&mut self, module: &Module, hlir: &HLIR) {
        if module.owner_id != OwnerDefId::ROOT_NODE {
            let current_path = self.current_path().clone();
            let module_ident = module.ident.ident().string();
            let local = self.should_be_local(&current_path);
            self.get_namespace_mut(&current_path)
                .expect("Namespace path exists.")
                .items
                .insert(module_ident.clone(), Namespace {
                    ident: module_ident.clone(),
                    kind: NamespaceKind::Module,
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(module.owner_id),
                    vis: if local {
                        Visibility::Private
                    } else {
                        Visibility::Public
                    },
                    local,
                });

            self.push_to_current_path(&module_ident);
            self.super_module(module, hlir);
            self.pop_from_current_path();
        } else {
            self.super_module(module, hlir);
        }
    }

    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let function_ident = function.ident.string();
        let local = self.should_be_local(&current_path);

        let namespace = self
            .get_namespace_mut(&current_path)
            .expect("Namespace path exists");

        if namespace.items.contains_key(&function_ident) {
            // error here.
            // TODO: REMOVE THESE TEMP PANICS EVERYWHERE
            panic!(
                "Namespace {} already contains item: {}",
                current_path, function_ident
            );
        }

        namespace.items.insert(function_ident.clone(), Namespace {
            ident: function_ident.clone(),
            kind: NamespaceKind::Function,
            items: HashMap::new(),
            id: ResolvedID::OwnerDef(function.owner_id),
            vis: if local {
                Visibility::Private
            } else {
                Visibility::Public
            },
            local,
        });

        if let Some(adt_impl_ty_id) = self.active_adt {
            if let Some(adt_info) = self.type_registry.get_from_type_id_mut(adt_impl_ty_id) {
                adt_info.associated_defs.push(function.owner_id);
            }
        }

        self.push_to_current_path(&function_ident);
        self.super_function(function, hlir);
        self.pop_from_current_path();
    }

    fn visit_struct(&mut self, structure: &super::hir::nodes::Struct, hlir: &HLIR) {
        fn get_field_info(node: Option<&HIRNode>) -> Option<StructField> {
            if let HIRNode::Field(a) = node? {
                Some(a.clone())
            } else {
                None
            }
        }

        let current_path = self.current_path().clone();
        let adt_fields: Option<Vec<StructField>> = structure
            .fields
            .iter()
            .map(|id| hlir.get_hir_node(*id))
            .map(get_field_info)
            .collect();

        let struct_type_id = if let Some(resolved_fields) = adt_fields {
            let fields = resolved_fields
                .iter()
                .map(|fi| ADTStructField {
                    ident: fi.ident.clone(),
                    ty: fi.ty.clone(),
                })
                .collect();

            self.type_registry.register_adt(ADTTypeInfo::new_struct(
                structure.owner_id,
                self.current_path().clone(),
                structure.ident.clone(),
                fields,
            ))
        } else {
            // idk..
            panic!("Failed to get some resolved struct fields.");
        };

        let local = self.should_be_local(&current_path);
        let struct_ident = structure.ident.string();
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(struct_ident.clone(), Namespace {
                ident: struct_ident,
                kind: NamespaceKind::Struct(struct_type_id),
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(structure.owner_id),
                vis: structure.vis,
                local,
            });
    }

    fn visit_enum(&mut self, enumeration: &super::hir::nodes::Enum, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let struct_ident = enumeration.ident.string();
        let local = self.should_be_local(&current_path);
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(struct_ident.clone(), Namespace {
                ident: struct_ident.clone(),
                kind: NamespaceKind::Enum,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(enumeration.owner_id),
                vis: enumeration.vis,
                local,
            });
    }

    fn visit_constant(&mut self, constant: &super::hir::nodes::Constant, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let const_ident = constant.ident.string();
        let local = self.should_be_local(&current_path);
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(const_ident.clone(), Namespace {
                ident: const_ident.clone(),
                kind: NamespaceKind::Constant,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(constant.owner_id),
                vis: constant.vis,
                local,
            });
    }

    fn visit_impl(&mut self, impl_info: &super::hir::nodes::Impl, hlir: &HLIR) {
        if !self.stage1_complete {
            let impl_path = if impl_info.self_ty.is_root_relative() {
                impl_info.self_ty.clone()
            } else {
                let mut ip = self.current_path().clone();
                ip.push_path(&impl_info.self_ty);
                ip
            };
            self.impl_path_lut.push((impl_info.owner_id, impl_path));
        } else {
            // do resolve.. add associated defs to type info..
            if let Some(impl_adt_id) = self
                .type_registry
                .find_type_id_from_path(self.current_path())
            {
                self.active_adt = Some(impl_adt_id);
                self.super_impl(impl_info, hlir);
                self.active_adt = None;
            } else {
                eprintln!("Failed to get impl adt id for '{}'", self.current_path());
            }
        }
    }
}

impl HLIRVisitorMut<'_> for AssociatedReferenceMapper {
    fn visit_module_mut(&mut self, module: &mut Module, hlir: &mut HLIRDisjointMut<'_>) {
        if module.owner_id != OwnerDefId::ROOT_NODE {
            self.push_to_current_path(module.ident.ident().str());
            self.super_module_mut(module, hlir);
            self.pop_from_current_path();
        } else {
            self.super_module_mut(module, hlir);
        }
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_to_current_path(function.ident.str());
        self.super_function_mut(function, hlir);
        self.pop_from_current_path();
    }

    fn visit_path_mut(&mut self, _id: HirId, path: &mut RefPath, _hlir: &mut HLIRDisjointMut<'_>) {
        if !path.is_resolved() {
            let ident_path = path.ident_path();
            let final_path = if ident_path.is_root_relative() {
                ident_path.clone()
            } else {
                ident_path
                    .rebase_from_path(
                        &self
                            .root_namespace
                            .find_previous_module(self.current_path()),
                    )
                    .expect("Not root relative.")
            };

            if let Some(def) = self.root_namespace.find_definition(&final_path) {
                path.resolve_to(def.id);
            }
        }
    }

    fn visit_impl_mut(
        &mut self,
        impl_info: &mut super::hir::nodes::Impl,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        let impl_path = if impl_info.self_ty.is_root_relative() {
            impl_info.self_ty.clone()
        } else {
            let mut ip = self.current_path().clone();
            ip.push_path(&impl_info.self_ty);
            ip
        };

        self.push_stack(impl_path);
        self.super_impl_mut(impl_info, hlir);
        self.pop_stack();
    }
}

#[derive(Clone, PartialEq)]
pub struct UnresolvedReference {
    pub path: SpannedIdentPath,
    pub id: HirId,
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

struct UnresolvedReferenceChecker {
    pub unresolved_references: Vec<UnresolvedReference>,
}

impl HLIRVisitor for UnresolvedReferenceChecker {
    fn visit_path(&mut self, id: HirId, path: &RefPath, hlir: &HLIR) {
        if let RefPath::Unresolved(ident_path) = path {
            self.unresolved_references.push(UnresolvedReference {
                path: ident_path.clone(),
                id,
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

struct TypeResolver<'a> {
    pub current_impl: Option<Type>,

    pub path_stack: Vec<IdentPath>,
    pub type_registry: TypeRegistry,

    pub root_namespace: &'a Namespace,

    // TODO: Implement these errors!!
    pub errors: Vec<LoweringError>,
}

impl<'a> TypeResolver<'a> {
    pub fn new(root_namespace: &'a Namespace, registry: TypeRegistry) -> Self {
        Self {
            current_impl: None,
            path_stack: vec![IdentPath::from_segments(Vec::new(), true)],
            type_registry: registry,
            root_namespace,
            errors: Vec::new(),
        }
    }
}

impl TypeResolver<'_> {
    pub fn resolve_types(&mut self, hlir: &mut HLIR) -> LowerResult<TypeRegistry> {
        self.walk_mut(hlir);

        Ok(self.type_registry.clone())
    }

    fn resolve_type(&self, path: &IdentPath) -> Option<TypeID> {
        let final_path = if path.is_root_relative() {
            path.clone()
        } else {
            path.rebase_from_path(
                &self
                    .root_namespace
                    .find_previous_module(self.current_path()),
            )
            .expect("Not root relative.")
        };

        let def = self.root_namespace.find_definition(&final_path)?;
        if let NamespaceKind::Struct(ty_id) = def.kind {
            Some(ty_id)
        } else {
            eprintln!(
                "Type path is not a struct: {path:?}, but instead: {:?}",
                def.kind
            );
            None
        }
        //self.type_registry.find_type_id_from_path(&type_path)
    }
}

impl TypeResolver<'_> {
    pub fn current_path(&self) -> &IdentPath {
        self.path_stack
            .last()
            .expect("Always has atleast 1 in stack.")
    }

    pub fn current_path_mut(&mut self) -> &mut IdentPath {
        self.path_stack
            .last_mut()
            .expect("Always has atleast 1 in stack.")
    }

    pub fn push_to_current_path(&mut self, s: &str) {
        self.current_path_mut().push(s);
    }

    pub fn pop_from_current_path(&mut self) {
        self.current_path_mut().pop();
    }

    #[allow(dead_code)]
    pub fn push_stack(&mut self, path: IdentPath) {
        self.path_stack.push(path);
    }

    #[allow(dead_code)]
    pub fn pop_stack(&mut self) {
        self.path_stack.pop();
    }
}

impl HLIRVisitorMut<'_> for TypeResolver<'_> {
    fn visit_expr_mut(
        &mut self,
        expr: &mut super::hir::nodes::Expr,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        match &mut expr.kind {
            ExprKind::StructInit(struct_init) => {
                if let RefPath::Unresolved(ty_path) = &struct_init.ty_path {
                    if let Some(type_id) = self.resolve_type(&ty_path.path) {
                        struct_init.ty_path =
                            RefPath::Resolved(ty_path.clone(), ResolvedID::TypeDef(type_id));
                    } else {
                        eprintln!("Failed to resolve struct init type!");
                    }
                }
            }
            ExprKind::MethodCall(tar, ident, args) => {}
            _ => {}
        }

        self.super_expr_mut(expr, hlir);
    }

    fn visit_module_mut(&mut self, module: &mut Module, hlir: &mut HLIRDisjointMut<'_>) {
        if module.owner_id != OwnerDefId::ROOT_NODE {
            self.push_to_current_path(module.ident.ident().str());
            self.super_module_mut(module, hlir);
            self.pop_from_current_path();
        } else {
            self.super_module_mut(module, hlir);
        }
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_to_current_path(function.ident.str());
        self.super_function_mut(function, hlir);

        for arg in &mut function.sig.parameters {
            match arg {
                Type::Unresolved(crate::ast::Ty::Type(type_path)) => {
                    if let Some(type_id) = self.resolve_type(type_path) {
                        *arg = Type::Resolved(KitTy::Abstract(type_id));
                    } else {
                        eprintln!("Failed to resolve function arg type path: {:?}", arg);
                    }
                }
                Type::Unresolved(crate::ast::Ty::This(_s)) => {
                    if let Some(impl_ty) = &self.current_impl {
                        *arg = impl_ty.clone();
                    } else {
                        eprintln!("Failed to resolve self function arg.");
                    }
                }
                _ => {}
            }
            if let Type::Unresolved(crate::ast::Ty::Type(type_path)) = arg {
                if let Some(type_id) = self.resolve_type(type_path) {
                    *arg = Type::Resolved(KitTy::Abstract(type_id));
                } else {
                    eprintln!("Failed to resolve function arg type path: {:?}", arg);
                }
            }
        }

        if let Type::Unresolved(crate::ast::Ty::Type(type_path)) = &function.sig.output {
            if let Some(type_id) = self.resolve_type(type_path) {
                function.sig.output = Type::Resolved(KitTy::Abstract(type_id));
            } else {
                eprintln!("Failed to resolve function type path");
            }
        }

        self.pop_from_current_path();
    }

    fn visit_struct_mut(
        &mut self,
        structure: &mut super::hir::nodes::Struct,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        self.push_to_current_path(structure.ident.str());
        let current_struct_path = self.current_path().clone();
        let Some(current_struct_type_id) = self.resolve_type(&current_struct_path) else {
            eprintln!("Failed to resolve struct: {:?} type id!", structure.ident);
            return;
        };
        for field_id in &structure.fields {
            if let Some(HIRNode::Field(field)) = hlir.get_hir_node_mut(*field_id) {
                let resolved_type =
                    if let Type::Unresolved(crate::ast::Ty::Type(type_path)) = &field.ty {
                        self.resolve_type(&type_path.path)
                    } else {
                        None
                    };

                if let Some(type_id) = resolved_type {
                    field.ty = Type::Resolved(KitTy::Abstract(type_id));

                    let struct_info = self
                        .type_registry
                        .get_from_type_id_mut(current_struct_type_id)
                        .expect("Has to exist.");
                    if let Some(field_info) = struct_info.get_field_by_ident_mut(field.ident.str())
                    {
                        field_info.ty = Type::Resolved(KitTy::Abstract(type_id));
                    } else {
                        eprintln!("Failed to find field info: {:?}", field.ident.str());
                    }
                }
            }
        }
        self.pop_from_current_path();
    }

    fn visit_impl_mut(
        &mut self,
        impl_info: &mut super::hir::nodes::Impl,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        let impl_path = if impl_info.self_ty.is_root_relative() {
            impl_info.self_ty.clone()
        } else {
            let mut ip = self.current_path().clone();
            ip.push_path(&impl_info.self_ty);
            ip
        };

        if let Some(type_id) = self.type_registry.find_type_id_from_path(&impl_path) {
            self.current_impl = Some(Type::Resolved(KitTy::Abstract(type_id)));
            self.super_impl_mut(impl_info, hlir);
            self.current_impl = None;
        } else {
            eprintln!("Failed to get type id for info: {:?}", impl_info.self_ty);
        }
    }
}

fn resolve_types(
    hlir: &mut HLIR,
    root_namespace: &Namespace,
    registry: TypeRegistry,
) -> LowerResult<TypeRegistry> {
    let mut resolver = TypeResolver::new(root_namespace, registry);
    resolver.resolve_types(hlir)
}

fn resolve_associated_references(hlir: &mut HLIR) -> LowerResult<(Namespace, TypeRegistry)> {
    let mut resolver = AssociatedReferenceMapper::new();
    let namespaces = resolver.map_references(hlir)?;

    Ok(namespaces)
}

fn resolve_scope_paths(hlir: &mut HLIR) -> LowerResult<()> {
    let mut resolver = ScopeResolver::new();
    resolver.resolve(hlir)
}

fn verify_references(hlir: &mut HLIR) -> LowerResult<()> {
    UnresolvedReferenceChecker::new().verify_references(hlir)
}

pub fn resolve_paths(hlir: &mut HLIR) -> LowerResult<(Namespace, TypeRegistry)> {
    resolve_scope_paths(hlir)?;
    let (namespace, mut registry) = resolve_associated_references(hlir)?;
    registry = resolve_types(hlir, &namespace, registry)?;

    // Verify all references have been resolved.
    verify_references(hlir)?;

    Ok((namespace, registry))
}
