use std::collections::HashMap;

use crate::ast::{IdentPath, SpannedIdentPath, Visibility};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::nodes::{
    Constant, Enum, Function, HIRNode, Impl, Module, RefPath, ResolvedID, Struct, StructField,
    UsePath,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitor, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::resolver::errors::{
    ResolutionFailure, UnresolvedReference, UnresolvedReferences,
};
use crate::intermediate::resolver::{
    ADTStructField, ADTTypeInfo, Namespace, NamespaceKind, TypeID, TypeRegistry,
};

use log::*;

struct AssociatedReferenceMapper {
    pub global_functions: HashMap<String, OwnerDefId>,

    pub path_stack: Vec<IdentPath>,
    pub impl_path_lut: Vec<(OwnerDefId, IdentPath)>,

    pub type_registry: TypeRegistry,

    pub root_namespace: Namespace,
    pub stage1_complete: bool,

    // TODO: Implement these errors!!
    pub errors: Vec<LoweringError>,
    pub resolution_failures: Vec<(HirId, SpannedIdentPath, ResolutionFailure)>,
}

impl AssociatedReferenceMapper {
    pub fn new() -> Self {
        Self {
            global_functions: HashMap::new(),
            path_stack: vec![IdentPath::from_segments(Vec::new(), true)],
            impl_path_lut: Vec::new(),
            type_registry: TypeRegistry::default(),
            root_namespace: Namespace::default_root_definition(),
            stage1_complete: false,
            errors: Vec::new(),
            resolution_failures: Vec::new(),
        }
    }

    pub fn reset_path_to(&mut self, ident_path: IdentPath) {
        self.path_stack = vec![ident_path];
    }

    pub fn reset_path(&mut self) {
        self.reset_path_to(IdentPath::from_segments(Vec::new(), true));
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
            "f32", "f64", "bool", "string", "char",
        ];

        builtin_types
            .into_iter()
            .map(builtin_namespace)
            .for_each(|ns| {
                self.root_namespace.items.insert(ns.ident.clone(), ns);
            });
    }

    fn resolve_uses_in_namespace(
        root_namespace: &Namespace,
        namespace: &mut Namespace,
        current_path: IdentPath,
    ) {
        let item_idents: Vec<String> = namespace.items.keys().cloned().collect();

        for ident in item_idents {
            if let Some(item_namespace) = namespace.items.get_mut(&ident) {
                let mut next_path = current_path.clone();
                next_path.push(&ident);

                match &item_namespace.kind {
                    NamespaceKind::Use(use_path) => {
                        let target_path = if use_path.is_root_relative() {
                            use_path.clone()
                        } else {
                            let mut resolved_path = current_path.clone();
                            resolved_path.push_path(use_path);
                            resolved_path
                        };

                        if let Some(target_namespace) = root_namespace.find_definition(&target_path)
                        {
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
                            );
                        } else {
                            // TODO: surface diagnostic once resolver errors are implemented.
                            error!(
                                "Failed to resolve use path {:?} within namespace {:?}",
                                target_path, current_path
                            );
                        }
                    }
                    _ => {
                        Self::resolve_uses_in_namespace(root_namespace, item_namespace, next_path);
                    }
                }
            }
        }
    }

    pub fn map_references(&mut self, hlir: &mut HLIR) -> LowerResult<(Namespace, TypeRegistry)> {
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

        let root_namespace_clone = self.root_namespace.clone();
        Self::resolve_uses_in_namespace(
            &root_namespace_clone,
            &mut self.root_namespace,
            IdentPath::new_empty(true),
        );

        // Resolve references..
        self.reset_path();
        self.walk_mut(hlir);

        if !self.resolution_failures.is_empty() {
            return Err(LoweringError::new(LoweringErrorKind::UnresolvedReferences(
                UnresolvedReferences {
                    references: self
                        .resolution_failures
                        .iter()
                        .map(|(a, b, c)| UnresolvedReference {
                            id: *a,
                            path: b.clone(),
                            failure: *c,
                        })
                        .collect(),
                },
            )));
        }

        Ok((self.root_namespace.clone(), self.type_registry.clone()))
    }

    fn find_last_enclosing_path(&self, path: &IdentPath) -> Option<IdentPath> {
        if path.is_empty() {
            return Some(IdentPath::new_empty(true));
        }

        Some(self.root_namespace.find_previous_enclosing_scope(path))
    }

    fn search_backtracking_for_definition(
        &self,
        ident_path: &IdentPath,
        mut base_path: IdentPath,
    ) -> Option<(IdentPath, ResolvedID)> {
        let path = ident_path.rebase_from_path(&base_path);
        if let Some(def) = self.root_namespace.find_definition(&path) {
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

    pub fn matching_segment_count(path_a: &IdentPath, path_b: &IdentPath) -> usize {
        let len = path_a.len().min(path_b.len());
        for i in 0..len {
            if path_a.segments()[i] != path_b.segments()[i] {
                return i;
            }
        }
        len
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
                let segment_match = Self::matching_segment_count(&p, &prev_base_path);
                let matched_base_path = IdentPath::from_segments_slice(
                    prev_base_path
                        .segments()
                        .get(0..segment_match)
                        .unwrap_or(&[]),
                    true,
                );

                if segment_match >= p.len() {
                    // Fully matched, no need to check further.
                    return Ok(def);
                }

                let local_check = matched_base_path.extend(&p.segments()[segment_match]);
                if !self.validate_local_access(&local_check, &matched_base_path) {
                    error!("{p} Failed local access check");
                    error!(
                        "Checked path: `{}` from `{}`  ->  `{}`",
                        local_check, matched_base_path, ident_path
                    );
                    error!("Base path: `{}`", base_path);
                    return Err(ResolutionFailure::Inaccessible);
                }

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
                        .map(|n| n.vis == Visibility::Public || n.kind == NamespaceKind::Function)
                        .unwrap_or_default();
                    if !visible {
                        error!(
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
                .insert(
                    module_ident.clone(),
                    Namespace {
                        ident: module_ident.clone(),
                        kind: NamespaceKind::Module,
                        items: HashMap::new(),
                        id: ResolvedID::OwnerDef(module.owner_id),
                        vis: module.vis,
                        local,
                    },
                );

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
            self.errors
                .push(LoweringError::new(LoweringErrorKind::ItemAlreadyDefined(
                    function.ident.span,
                    function_ident.clone(),
                    current_path.to_string(),
                )));
        }

        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists")
            .items
            .insert(
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

        self.push_to_current_path(&function_ident);
        self.super_function(function, hlir);
        self.pop_from_current_path();
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

            self.type_registry.register_adt(ADTTypeInfo::new_struct(
                structure.owner_id,
                self.current_path().clone(),
                structure.ident.clone(),
                fields,
            ))
        } else {
            // idk..
            // TODO: REDO THESE ERRORS.
            self.errors
                .push(LoweringError::new(LoweringErrorKind::RemoveMeMessage(
                    "Failed to get some resolved struct fields.".to_string(),
                    Some(structure.ident.span),
                )));
            0
        };

        let local = self.should_be_local(&current_path);
        let struct_ident = structure.ident.string();
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(
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
    }

    fn visit_enum(&mut self, enumeration: &Enum, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let struct_ident = enumeration.ident.string();
        let local = self.should_be_local(&current_path);
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(
                struct_ident.clone(),
                Namespace {
                    ident: struct_ident.clone(),
                    kind: NamespaceKind::Enum,
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(enumeration.owner_id),
                    vis: enumeration.vis,
                    local,
                },
            );
    }

    fn visit_constant(&mut self, constant: &Constant, _hlir: &HLIR) {
        let current_path = self.current_path().clone();
        let const_ident = constant.ident.string();
        let local = self.should_be_local(&current_path);
        self.get_namespace_mut(&current_path)
            .expect("Namespace path exists.")
            .items
            .insert(
                const_ident.clone(),
                Namespace {
                    ident: const_ident.clone(),
                    kind: NamespaceKind::Constant,
                    items: HashMap::new(),
                    id: ResolvedID::OwnerDef(constant.owner_id),
                    vis: constant.vis,
                    local,
                },
            );
    }

    fn visit_impl(&mut self, impl_info: &Impl, hlir: &HLIR) {
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
            // Do the resolve.
            self.super_impl(impl_info, hlir);
        }
    }

    fn visit_use(&mut self, use_info: &UsePath, hlir: &HLIR) {
        let current_path = self.current_path().clone();

        for import in &use_info.imports {
            let import_ident = import.segments().last().expect("Atleast one segment.");
            self.get_namespace_mut(&current_path)
                .expect("Namespace path exists.")
                .items
                .insert(
                    import_ident.clone(),
                    Namespace {
                        ident: import_ident.clone(),
                        kind: NamespaceKind::Use(import.clone()),
                        items: HashMap::new(),
                        id: ResolvedID::OwnerDef(OwnerDefId(0)),
                        vis: use_info.vis,
                        local: use_info.vis == Visibility::Private,
                    },
                );
        }

        self.super_use(use_info, hlir);
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
        // TODO: Refactor this duplicated code everywhere to it's own method.
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

pub fn resolve_associated_references(hlir: &mut HLIR) -> LowerResult<(Namespace, TypeRegistry)> {
    AssociatedReferenceMapper::new().map_references(hlir)
}
