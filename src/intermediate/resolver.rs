use ::std::fmt::Debug;
use std::collections::HashMap;

use crate::ast::{IdentPath, IdentPathSegment, SpannedIdentPath, Visibility};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::nodes::{
    Function, LetStatement, Module, Parameter, RefPath, ResolvedID,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitor, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

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
        if block.id.id.0 > 0 {
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
    Struct,
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

struct AssociatedReferenceMapper {
    pub path_stack: Vec<IdentPath>,

    pub impl_path_lut: Vec<(OwnerDefId, IdentPath)>,

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

    pub fn map_references(&mut self, hlir: &mut HLIR) -> LowerResult<Namespace> {
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

        Ok(self.root_namespace.clone())
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
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(function_ident.clone(), Namespace {
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

        self.push_to_current_path(&function_ident);
        self.super_function(function, hlir);
        self.pop_from_current_path();
    }

    fn visit_struct(&mut self, structure: &super::hir::nodes::Struct, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let struct_ident = structure.ident.string();
        let local = self.should_be_local(&current_path);
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(struct_ident.clone(), Namespace {
                ident: struct_ident.clone(),
                kind: NamespaceKind::Struct,
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
            self.super_impl(impl_info, hlir);
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

fn resolve_associated_references(hlir: &mut HLIR) -> LowerResult<Namespace> {
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

pub fn resolve_paths(hlir: &mut HLIR) -> LowerResult<Namespace> {
    resolve_scope_paths(hlir)?;
    let namespaces = resolve_associated_references(hlir)?;

    // Verify all references have been resolved.
    verify_references(hlir)?;

    Ok(namespaces)
}
