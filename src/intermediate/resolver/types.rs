use crate::ast::IdentPath;

use crate::intermediate::hir::errors::{LowerResult, LoweringError};
use crate::intermediate::hir::nodes::{
    Expr, ExprKind, Function, HIRNode, Impl, Module, RefPath, ResolvedID, Struct, Type,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, OwnerDefId};

use crate::intermediate::resolver::{Namespace, NamespaceKind, TypeID, TypeRegistry};
use crate::intermediate::types::KitTy;

use log::*;

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
        let final_path = path.rebase_from_path(
            &self
                .root_namespace
                .find_previous_module(self.current_path()),
        );

        let def = self.root_namespace.find_definition(&final_path)?;
        if let NamespaceKind::Struct(ty_id) = def.kind {
            Some(ty_id)
        } else {
            error!(
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
    fn visit_expr_mut(&mut self, expr: &mut Expr, hlir: &mut HLIRDisjointMut<'_>) {
        match &mut expr.kind {
            ExprKind::StructInit(struct_init) => {
                if let RefPath::Unresolved(ty_path) = &struct_init.ty_path {
                    if let Some(type_id) = self.resolve_type(&ty_path.path) {
                        struct_init.ty_path =
                            RefPath::Resolved(ty_path.clone(), ResolvedID::TypeDef(type_id));
                    } else {
                        error!("Failed to resolve struct init type!");
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
                        error!("Failed to resolve function arg type path: {:?}", arg);
                    }
                }
                Type::Unresolved(crate::ast::Ty::This(_s)) => {
                    if let Some(impl_ty) = &self.current_impl {
                        *arg = impl_ty.clone();
                    } else {
                        error!("Failed to resolve self function arg.");
                    }
                }
                _ => {}
            }
            if let Type::Unresolved(crate::ast::Ty::Type(type_path)) = arg {
                if let Some(type_id) = self.resolve_type(type_path) {
                    *arg = Type::Resolved(KitTy::Abstract(type_id));
                } else {
                    error!("Failed to resolve function arg type path: {:?}", arg);
                }
            }
        }

        if let Type::Unresolved(crate::ast::Ty::Type(type_path)) = &function.sig.output {
            if let Some(type_id) = self.resolve_type(type_path) {
                function.sig.output = Type::Resolved(KitTy::Abstract(type_id));
            } else {
                error!("Failed to resolve function type path");
            }
        }

        self.pop_from_current_path();
    }

    fn visit_struct_mut(&mut self, structure: &mut Struct, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_to_current_path(structure.ident.str());
        let current_struct_path = self.current_path().clone();
        let Some(current_struct_type_id) = self.resolve_type(&current_struct_path) else {
            error!("Failed to resolve struct: {:?} type id!", structure.ident);
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
                        error!("Failed to find field info: {:?}", field.ident.str());
                    }
                }
            }
        }
        self.pop_from_current_path();
    }

    fn visit_impl_mut(&mut self, impl_info: &mut Impl, hlir: &mut HLIRDisjointMut<'_>) {
        let impl_path = impl_info.self_ty.rebase_from_path(self.current_path());

        if let Some(t) = self.type_registry.find_type_from_path(&impl_path) {
            self.current_impl = Some(Type::Resolved(t));
            self.super_impl_mut(impl_info, hlir);
            self.current_impl = None;
        } else {
            error!("Failed to get type id for info: {:?}", impl_info.self_ty);
        }
    }
}

pub fn resolve_types(
    hlir: &mut HLIR,
    root_namespace: &Namespace,
    registry: TypeRegistry,
) -> LowerResult<TypeRegistry> {
    TypeResolver::new(root_namespace, registry).resolve_types(hlir)
}
