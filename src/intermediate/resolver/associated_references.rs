use std::collections::HashMap;

use crate::ast::{IdentPath, SpannedIdentPath, Visibility};

use crate::intermediate::hir::nodes::{
    Constant, Enum, Function, HIRNode, Impl, Module, RefPath, ResolvedID, Struct, StructField,
    UsePath,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitor, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::hir::ProgramMetaData;
use crate::intermediate::resolver::errors::{
    ResolutionFailure, ResolveResult, ResolverError, ResolverErrorKind, UnresolvedReference,
    UnresolvedReferences, push_resolve_err, resolve_err,
};
use crate::intermediate::resolver::{ADTStructField, ADTTypeInfo, Namespace, NamespaceKind};

use log::*;

/// Builds and maintains the namespace scope tree by walking the HIR, and
/// resolves associated references. I.e. paths from types, modules, etc.
/// This is also where visibility and 'local' access control is enforced.
/// ```ignore
/// Struct Foo;
///
/// impl Foo {
///    fn bar() {}
/// }
///
/// fn main() {
///   Foo::bar();
/// }
/// ```
/// In this example, `Foo::bar()` is an associated reference that this mapper
/// will resolve to it's [`OwnerDefId`].
///
/// This also keeps track of defined global/native functions, as those also function like
/// associated types in that they can be referenced from anywhere, and are not 'locals'.
///
/// Builtin types are also defined in the root namespace for resolution,
/// and implementation, although the underlying layout and type info is defined
/// within the compiler, hence there is no `pub struct i32 ...` declaration or similar to
/// be found anywhere in the code, as it is handled and defined here.
///
/// # Technical details
///
/// The 'entrypoint' for this resolving pass is the `map_references` function.
/// This mapper works in multiple stages:
/// - First the builtin types are injected into the root namespace.
/// - Then the HIR is walked to build the namespace tree, ignoring `impl` items while storing
///   their paths for later processing. This is because `impl` items may refer to types that are defined
///   below them and thus do not yet have a namespace associated with them.
///   This pass is done immutably as no modifications need to be made.
/// - In the next stage, we manually visit all the `impl` items that were deferred and add them to the namespace
///   now that all types and modules should have been defined, this way we know if it is not defined at this point,
///   then this constitutes an error. This stage is also done immutably.
/// - After that, we resolve all `use` items by essentially copying the definitions from the target to the current namespace.
/// - Now that the namespace tree is fully built and all items are defined, we do a final mutable pass to resolve `Path`
///   expressions in the tree.
///
/// Any errors that occured during the process are collected and returned after all stages have been attempted,
/// this makes it possible to report multiple errors in one go instead of failing fast on the first error encountered.
/// As a developer this makes it much easier to fix multiple errors at once, instead of having to re-run the compiler
/// multiple times to fix each error one by one.
struct AssociatedReferenceMapper<'a> {
    pub meta: &'a mut ProgramMetaData,

    pub global_functions: HashMap<String, OwnerDefId>,

    pub path_stack: Vec<IdentPath>,
    pub impl_path_lut: Vec<(OwnerDefId, IdentPath)>,

    pub stage1_complete: bool,

    pub errors: Vec<ResolverError>,
    pub resolution_failures: Vec<(HirId, SpannedIdentPath, ResolutionFailure)>,
}

impl<'a> AssociatedReferenceMapper<'a> {
    pub fn new(meta: &'a mut ProgramMetaData) -> Self {
        Self {
            global_functions: HashMap::new(),
            meta,
            path_stack: vec![IdentPath::from_segments(Vec::new(), true)],
            impl_path_lut: Vec::new(),
            stage1_complete: false,
            errors: Vec::new(),
            resolution_failures: Vec::new(),
        }
    }

    #[inline]
    pub fn reset_path_to(&mut self, ident_path: IdentPath) {
        self.path_stack = vec![ident_path];
    }

    #[inline]
    pub fn reset_path(&mut self) {
        self.reset_path_to(IdentPath::from_segments(Vec::new(), true));
    }

    /// Executes `f` with `segment` temporarily pushed onto the current path.
    /// Ensures the segment is popped after `f` completes.
    #[inline]
    fn with_pushed_segment<F>(&mut self, segment: &str, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.push_to_current_path(segment);
        f(self);
        self.pop_from_current_path();
    }

    /// Executes `f` with `path` temporarily pushed onto the path stack.
    /// Ensures the path is popped after `f` completes.
    #[inline]
    fn with_pushed_path<F>(&mut self, path: IdentPath, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.push_stack(path);
        f(self);
        self.pop_stack();
    }

    fn inject_builtin_definitions(&mut self) {
        fn builtin_namespace(name: &str) -> Namespace {
            Namespace {
                ident: name.to_string(),
                kind: NamespaceKind::Builtin,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(OwnerDefId::ROOT_NODE),
                vis: Visibility::Public,
                local: false,
            }
        }

        let builtin_types = vec![
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "string", "char", "()",
        ];

        builtin_types
            .into_iter()
            .map(builtin_namespace)
            .for_each(|ns| {
                self.meta.namespace.insert(ns);
            });
    }

    fn resolve_uses_in_namespace(
        root_namespace: &Namespace,
        namespace: &mut Namespace,
        current_path: IdentPath,
        // so bad...
        errors: &mut Vec<ResolverError>,
    ) {
        let item_idents: Vec<String> = namespace.items.keys().cloned().collect();

        for ident in item_idents {
            let Some(item_namespace) = namespace.get_mut(&ident) else {
                continue;
            };
            let mut next_path = current_path.clone();
            next_path.push(&ident);
            if let NamespaceKind::Use(use_path) = &item_namespace.kind {
                let target_path = use_path.rebase_from_path(&current_path);

                if let Some(target_namespace) = root_namespace.find_definition(&target_path) {
                    let mut cloned = target_namespace.clone();
                    cloned.ident = item_namespace.ident.clone();
                    cloned.vis = item_namespace.vis;
                    cloned.local = item_namespace.local;
                    cloned.id = target_namespace.id;
                    *item_namespace = cloned;

                    Self::resolve_uses_in_namespace(
                        root_namespace,
                        item_namespace,
                        next_path,
                        errors,
                    );
                } else {
                    errors.push(resolve_err!(
                        no_span,
                        "Failed to resolve use path `{:?}` within namespace `{:?}`",
                        target_path,
                        current_path
                    ));
                }
            } else {
                Self::resolve_uses_in_namespace(root_namespace, item_namespace, next_path, errors);
            }
        }
    }

    pub fn map_references(&mut self, hlir: &mut HLIR) -> ResolveResult<()> {
        self.inject_builtin_definitions();

        // Map all except impls..
        self.visit_root(hlir);
        self.stage1_complete = true;

        // Manually traverse impl items..
        let impls = self.impl_path_lut.clone();
        for (impl_id, impl_path) in impls {
            if let Some(node) = hlir.owning_node(impl_id)
                && let Some(impl_info) = node.hir_impl_ref()
            {
                self.reset_path_to(impl_path);
                self.visit_impl(impl_info, hlir);
            }
        }

        let root_namespace_clone = self.meta.namespace.clone();
        let mut resolve_use_errors = Vec::new();
        Self::resolve_uses_in_namespace(
            &root_namespace_clone,
            &mut self.meta.namespace,
            IdentPath::new_empty(true),
            &mut resolve_use_errors,
        );

        if !resolve_use_errors.is_empty() {
            self.errors.extend(resolve_use_errors);
        }

        // Resolve references..
        self.reset_path();
        self.walk_mut(hlir);

        if !self.resolution_failures.is_empty() {
            return Err(
                ResolverErrorKind::UnresolvedReferences(UnresolvedReferences {
                    references: self
                        .resolution_failures
                        .iter()
                        .map(|(a, b, c)| UnresolvedReference {
                            id: *a,
                            path: b.clone(),
                            failure: *c,
                        })
                        .collect(),
                })
                .with_no_span(),
            );
        }

        if !self.errors.is_empty() {
            return Err(ResolverErrorKind::ResolverErrors(self.errors.clone()).with_no_span());
        }

        Ok(())
    }

    #[inline]
    fn find_last_enclosing_path(&self, path: &IdentPath) -> Option<IdentPath> {
        if path.is_empty() {
            return Some(IdentPath::new_empty(true));
        }

        Some(self.meta.namespace.find_previous_enclosing_scope(path))
    }

    #[inline]
    fn search_backtracking_for_definition(
        &self,
        ident_path: &IdentPath,
        mut base_path: IdentPath,
    ) -> Option<(IdentPath, ResolvedID)> {
        let path = ident_path.rebase_from_path(&base_path);
        if let Some(def) = self.meta.namespace.find_definition(&path) {
            return Some((path, def.id));
        }

        let namespace = self.get_namespace(&base_path)?;
        if namespace.kind == NamespaceKind::Module {
            None
        } else {
            base_path.pop();
            self.search_backtracking_for_definition(ident_path, base_path)
        }
    }

    #[inline]
    fn validate_local_access(&self, target_path: &IdentPath, base_path: &IdentPath) -> bool {
        let Some(target_namespace) = self.get_namespace(target_path) else {
            return false;
        };
        if target_namespace.local {
            let Some(enclosing_path) = self.find_last_enclosing_path(target_path) else {
                return false;
            };
            if !base_path.is_subpath_of(&enclosing_path) {
                return false;
            }
        }
        true
    }

    pub fn search_for_definition(
        &self,
        ident_path: &IdentPath,
        base_path: &IdentPath,
    ) -> Result<ResolvedID, ResolutionFailure> {
        if ident_path.len() == 1
            && let Some(id) = self.global_functions.get(ident_path.path_stem())
        {
            Ok(ResolvedID::OwnerDef(*id))
        } else if let Some((p, def)) =
            self.search_backtracking_for_definition(ident_path, base_path.clone())
        {
            if !self.validate_local_access(&p, base_path) {
                return Err(ResolutionFailure::Inaccessible);
            }

            if let Some(prev_base_path) = self.find_last_enclosing_path(base_path) {
                let segment_match = p.matching_segment_count(&prev_base_path);
                let matched_base_path = prev_base_path.subpath(0, segment_match);

                if segment_match >= p.len() {
                    // Fully matched, no need to check further.
                    return Ok(def);
                }

                let local_check = matched_base_path.extend(&p.segments()[segment_match]);
                if !self.validate_local_access(&local_check, &matched_base_path) {
                    debug!("{p} Failed local access check");
                    debug!(
                        "Checked path: `{}` from `{}`  ->  `{}`",
                        local_check, matched_base_path, ident_path
                    );
                    debug!("Base path: `{}`", base_path);
                    return Err(ResolutionFailure::Inaccessible);
                }

                let inside_func = self
                    .get_namespace(&local_check)
                    .is_some_and(|ns| ns.kind == NamespaceKind::Function);

                if segment_match.saturating_add(1) >= p.len() {
                    // Fully matched, no need to check further.
                    return Ok(def);
                }

                let len_to_check = p.len().saturating_sub(segment_match);

                for i in 1..len_to_check {
                    let to_check = prev_base_path.extend_path(&IdentPath::from_segments_slice(
                        &p.segments()[segment_match..=segment_match + i],
                        true,
                    ));

                    let visible = self
                        .get_namespace(&to_check)
                        .map(|n| {
                            n.vis == Visibility::Public
                                || (n.kind == NamespaceKind::Function && inside_func)
                        })
                        .unwrap_or_default();
                    if !visible {
                        debug!(
                            "{p} Failed check at: `{to_check}` from `{base_path}`  ->  `{ident_path}`"
                        );
                        return Err(ResolutionFailure::Inaccessible);
                    }
                }
            }

            Ok(def)
        } else {
            Err(ResolutionFailure::NotFound)
        }
    }
}

impl AssociatedReferenceMapper<'_> {
    #[inline]
    pub fn current_path(&self) -> &IdentPath {
        self.path_stack
            .last()
            .expect("Always has atleast 1 in stack.")
    }

    #[inline]
    pub fn current_path_mut(&mut self) -> &mut IdentPath {
        self.path_stack
            .last_mut()
            .expect("Always has atleast 1 in stack.")
    }

    #[inline]
    pub fn push_to_current_path(&mut self, s: &str) {
        self.current_path_mut().push(s);
    }

    #[inline]
    pub fn pop_from_current_path(&mut self) {
        self.current_path_mut().pop();
    }

    #[inline]
    pub fn push_stack(&mut self, path: IdentPath) {
        self.path_stack.push(path);
    }

    #[inline]
    pub fn pop_stack(&mut self) {
        self.path_stack.pop();
    }
}

impl AssociatedReferenceMapper<'_> {
    #[inline]
    fn should_be_local(&self, path: &IdentPath) -> bool {
        let Some(namespace) = self.get_namespace(path) else {
            return false;
        };
        namespace.local || namespace.kind == NamespaceKind::Function
    }

    #[inline]
    fn get_namespace(&self, path: &IdentPath) -> Option<&Namespace> {
        let mut curr_namespace = &self.meta.namespace;
        for segment in path.segments() {
            curr_namespace = curr_namespace.get(segment)?;
        }
        Some(curr_namespace)
    }

    #[inline]
    fn get_namespace_mut(&mut self, path: &IdentPath) -> Option<&mut Namespace> {
        let mut curr_namespace = &mut self.meta.namespace;
        for segment in path.segments() {
            curr_namespace = curr_namespace.get_mut(segment)?;
        }
        Some(curr_namespace)
    }
}

impl HLIRVisitor for AssociatedReferenceMapper<'_> {
    fn visit_module(&mut self, module: &Module, hlir: &HLIR) {
        if module.owner_id == OwnerDefId::ROOT_NODE {
            self.super_module(module, hlir);
            return;
        }

        let current_path = self.current_path().clone();
        let module_ident = module.ident.ident().string();
        let local = self.should_be_local(&current_path);
        if let Some(ns) = self.get_namespace_mut(&current_path) {
            ns.insert(Namespace {
                ident: module_ident.clone(),
                kind: NamespaceKind::Module,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(module.owner_id),
                vis: module.vis,
                local,
            });
            self.with_pushed_segment(&module_ident, |this| this.super_module(module, hlir));
        } else {
            push_resolve_err!(
                self,
                on_span,
                module.ident.span(),
                "Cannot find parent namespace `{}`",
                current_path
            );
        }
    }

    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let function_ident = function.ident.string();
        let local = self.should_be_local(&current_path);

        if function.is_global {
            self.global_functions
                .insert(function_ident.clone(), function.owner_id);
        }

        if self
            .get_namespace_mut(&current_path)
            .expect("Namespace path exists")
            .items
            .contains_key(&function_ident)
        {
            push_resolve_err!(
                self,
                on_span,
                function.ident.span,
                "Item already defined: `{}`",
                current_path
            );
        }
        if let Some(ns) = self.get_namespace_mut(&current_path) {
            ns.items.insert(
                function_ident.clone(),
                Namespace {
                    ident: function_ident.clone(),
                    kind: NamespaceKind::Function,
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(function.owner_id),
                    vis: function.vis,
                    local,
                },
            );
            self.with_pushed_segment(&function_ident, |this| this.super_function(function, hlir));
        } else {
            push_resolve_err!(
                self,
                on_span,
                function.ident.span,
                "Cannot find parent namespace: `{}`",
                current_path
            );
        }
    }

    fn visit_struct(&mut self, structure: &Struct, hlir: &HLIR) {
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

            self.meta
                .type_registry
                .register_adt(ADTTypeInfo::new_struct(
                    structure.owner_id,
                    self.current_path().clone(),
                    structure.ident.clone(),
                    fields,
                ))
        } else {
            push_resolve_err!(
                self,
                on_span,
                structure.ident.span,
                "Cannot resolve struct fields for `{}`",
                current_path
            );
            0
        };

        let local = self.should_be_local(&current_path);
        let struct_ident = structure.ident.string();
        if let Some(ns) = self.get_namespace_mut(&current_path) {
            ns.items.insert(
                struct_ident.clone(),
                Namespace {
                    ident: struct_ident,
                    kind: NamespaceKind::Struct(struct_type_id),
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(structure.owner_id),
                    vis: structure.vis,
                    local,
                },
            );
        } else {
            push_resolve_err!(
                self,
                on_span,
                structure.ident.span,
                "Cannot find parent namespace: `{}`",
                current_path
            );
        }
    }

    fn visit_enum(&mut self, enumeration: &Enum, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let struct_ident = enumeration.ident.string();
        let local = self.should_be_local(&current_path);
        if let Some(ns) = self.get_namespace_mut(&current_path) {
            ns.insert(Namespace {
                ident: struct_ident,
                kind: NamespaceKind::Enum,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(enumeration.owner_id),
                vis: enumeration.vis,
                local,
            });
        } else {
            push_resolve_err!(
                self,
                on_span,
                enumeration.ident.span,
                "Cannot find parent namespace: `{}`",
                current_path
            );
        }
    }

    fn visit_constant(&mut self, constant: &Constant, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let const_ident = constant.ident.string();
        let local = self.should_be_local(&current_path);
        if let Some(ns) = self.get_namespace_mut(&current_path) {
            ns.insert(Namespace {
                ident: const_ident,
                kind: NamespaceKind::Constant,
                items: HashMap::new(),
                id: ResolvedID::OwnerDef(constant.owner_id),
                vis: constant.vis,
                local,
            });
        } else {
            push_resolve_err!(
                self,
                on_span,
                constant.ident.span,
                "Cannot find parent namespace: `{}`",
                current_path
            );
        }
    }

    fn visit_impl(&mut self, impl_info: &Impl, hlir: &HLIR) {
        if !self.stage1_complete {
            let impl_path = impl_info.self_ty.rebase_from_path(self.current_path());
            self.impl_path_lut.push((impl_info.owner_id, impl_path));
        } else {
            // Do the resolve.
            self.super_impl(impl_info, hlir);
        }
    }

    fn visit_use(&mut self, use_info: &UsePath, hlir: &HLIR) {
        let current_path = self.current_path().clone();

        for import in &use_info.imports {
            let import_ident = import.segments().last().expect("Atleast one segment.");
            if let Some(ns) = self.get_namespace_mut(&current_path) {
                ns.insert(Namespace {
                    ident: import_ident.clone(),
                    kind: NamespaceKind::Use(import.clone()),
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(OwnerDefId(0)),
                    vis: use_info.vis,
                    local: use_info.vis == Visibility::Private,
                });
            } else {
                push_resolve_err!(
                    self,
                    on_span,
                    use_info.span,
                    "Cannot find parent namespace: `{}`",
                    current_path
                );
            }
        }

        self.super_use(use_info, hlir);
    }
}

impl HLIRVisitorMut<'_> for AssociatedReferenceMapper<'_> {
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

    fn visit_path_mut(&mut self, id: HirId, path: &mut RefPath, _hlir: &mut HLIRDisjointMut<'_>) {
        if !path.is_resolved() {
            let ident_path = path.ident_path();
            match self.search_for_definition(ident_path, self.current_path()) {
                Ok(resolved) => {
                    path.resolve_to(resolved);
                }
                Err(ResolutionFailure::Inaccessible) => {
                    self.resolution_failures.push((
                        id,
                        path.spanned_ident_path().clone(),
                        ResolutionFailure::Inaccessible,
                    ));
                }
                _ => {}
            }
        }
    }

    fn visit_impl_mut(&mut self, impl_info: &mut Impl, hlir: &mut HLIRDisjointMut<'_>) {
        let impl_path = impl_info.self_ty.rebase_from_path(self.current_path());
        self.with_pushed_path(impl_path, |this| this.super_impl_mut(impl_info, hlir));
    }
}

pub fn resolve_associated_references(
    hlir: &mut HLIR,
    meta_data: &mut ProgramMetaData,
) -> ResolveResult<()> {
    AssociatedReferenceMapper::new(meta_data).map_references(hlir)
}
