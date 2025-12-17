use crate::ast::{IdentPath, Ty};

use crate::intermediate::hir::nodes::{
    Expr, ExprKind, Function, Impl, Module, RefPath, ResolvedID, Struct, StructField, Type,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, OwnerDefId};

use crate::intermediate::hir::ProgramMetaData;
use crate::intermediate::resolver::errors::{
    ResolveResult, ResolverError, ResolverErrorKind, push_resolve_err, resolve_err,
};
use crate::intermediate::resolver::{NamespaceKind, TypeID};
use crate::intermediate::types::KitTy;

struct TypeResolver<'a> {
    pub current_impl: Option<Type>,
    pub path_stack: Vec<IdentPath>,

    pub meta: &'a mut ProgramMetaData,

    pub errors: Vec<ResolverError>,
}

impl<'a> TypeResolver<'a> {
    pub fn new(meta: &'a mut ProgramMetaData) -> Self {
        Self {
            current_impl: None,
            path_stack: vec![IdentPath::from_segments(Vec::new(), true)],
            meta,
            errors: Vec::new(),
        }
    }
}

impl TypeResolver<'_> {
    pub fn resolve_types(&mut self, hlir: &mut HLIR) -> ResolveResult<()> {
        self.walk_mut(hlir);

        if !self.errors.is_empty() {
            return Err(ResolverErrorKind::ResolverErrors(self.errors.clone()).with_no_span());
        }

        Ok(())
    }

    fn resolve_type(&self, path: &IdentPath) -> ResolveResult<TypeID> {
        let final_path = path.rebase_from_path(
            &self
                .meta
                .namespace
                .find_previous_module(self.current_path()),
        );

        let Some(def) = self.meta.namespace.find_definition(&final_path) else {
            return Err(resolve_err!(
                no_span,
                "Cannot find parent namespace while type checking for path `{}`",
                path
            ));
        };

        if let NamespaceKind::Struct(ty_id) = def.kind {
            Ok(ty_id)
        } else {
            Err(resolve_err!(
                no_span,
                "Expected ADT definition at `{}`, but instead found: `{:?}`!",
                path,
                def.kind
            ))
        }
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
        if let ExprKind::StructInit(struct_init) = &mut expr.kind
            && let RefPath::Unresolved(ty_path) = &struct_init.ty_path
        {
            match self.resolve_type(&ty_path.path) {
                Ok(type_id) => {
                    struct_init.ty_path =
                        RefPath::Resolved(ty_path.clone(), ResolvedID::TypeDef(type_id));
                }
                Err(e) => self.errors.push(e),
            }
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

        for (i, arg) in function.sig.parameters.iter_mut().enumerate() {
            match arg {
                Type::Unresolved(Ty::Type(type_path)) => match self.resolve_type(type_path) {
                    Ok(type_id) => {
                        *arg = Type::Resolved(KitTy::Abstract(type_id));
                    }
                    Err(e) => self.errors.push(e),
                },
                Type::Unresolved(Ty::This(sp)) => {
                    if let Some(impl_ty) = &self.current_impl {
                        *arg = impl_ty.clone();
                    } else {
                        push_resolve_err!(
                            self,
                            on_span,
                            *sp,
                            "Could not resolve type of `self` function argument in `{}`",
                            self.current_path()
                        );
                    }
                }
                _ => {}
            }
            if let Type::Unresolved(Ty::Type(type_path)) = arg {
                if let Ok(type_id) = self.resolve_type(type_path) {
                    *arg = Type::Resolved(KitTy::Abstract(type_id));
                } else {
                    push_resolve_err!(
                        self,
                        on_span,
                        type_path.span,
                        "Could not resolve type `{}` of function argument `{}` in `{}`",
                        type_path.path,
                        function
                            .sig
                            .parameter_idents
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| "??".to_string()),
                        self.current_path()
                    );
                }
            }
        }

        if let Type::Unresolved(Ty::Type(type_path)) = &function.sig.output {
            if let Ok(type_id) = self.resolve_type(type_path) {
                function.sig.output = Type::Resolved(KitTy::Abstract(type_id));
            } else {
                push_resolve_err!(
                    self,
                    on_span,
                    type_path.span,
                    "Could not resolve type `{}` in function return type of `{}`",
                    type_path.path,
                    self.current_path()
                );
            }
        }

        self.pop_from_current_path();
    }

    fn visit_let_statement_mut(
        &mut self,
        id: crate::intermediate::hir::HirId,
        let_statement: &mut crate::intermediate::hir::nodes::LetStatement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        self.super_let_statement_mut(id, let_statement, hlir);
        if let Type::Unresolved(Ty::Type(type_path)) = &let_statement.ty {
            match self.resolve_type(&type_path.path) {
                Ok(type_id) => {
                    let_statement.ty = Type::Resolved(KitTy::Abstract(type_id));
                }
                Err(e) => {
                    self.errors.push(e);
                }
            }
        }
    }

    fn visit_struct_mut(&mut self, structure: &mut Struct, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_to_current_path(structure.ident.str());
        let current_struct_path = self.current_path().clone();
        let current_struct_type = match self.resolve_type(&current_struct_path) {
            Ok(t) => t,
            Err(e) => {
                self.errors.push(e);
                self.pop_from_current_path();
                return;
            }
        };

        for field_id in &structure.fields {
            if let Some(field) = hlir.get_hir_node_mut_as::<StructField>(*field_id) {
                let resolved_type = if let Type::Unresolved(Ty::Type(type_path)) = &field.ty {
                    match self.resolve_type(&type_path.path) {
                        Ok(type_id) => Some(type_id),
                        Err(e) => {
                            self.errors.push(e);
                            None
                        }
                    }
                } else {
                    None
                };

                if let Some(type_id) = resolved_type {
                    field.ty = Type::Resolved(KitTy::Abstract(type_id));

                    let struct_info = self
                        .meta
                        .type_registry
                        .get_from_type_id_mut(current_struct_type)
                        .expect("Has to exist.");
                    if let Some(field_info) = struct_info.get_field_by_ident_mut(field.ident.str())
                    {
                        field_info.ty = Type::Resolved(KitTy::Abstract(type_id));
                    } else {
                        push_resolve_err!(
                            self,
                            no_span,
                            "Failed to find struct field `{}` in struct `{}`",
                            field.ident.string(),
                            current_struct_path
                        );
                    }
                }
            }
        }
        self.pop_from_current_path();
    }

    fn visit_impl_mut(&mut self, impl_info: &mut Impl, hlir: &mut HLIRDisjointMut<'_>) {
        let impl_path = impl_info.self_ty.rebase_from_path(self.current_path());

        if let Some(t) = self.meta.type_registry.find_type_from_path(&impl_path) {
            self.current_impl = Some(Type::Resolved(t));
            self.super_impl_mut(impl_info, hlir);
            self.current_impl = None;
        } else {
            push_resolve_err!(
                self,
                no_span,
                "Could not find type for impl at path: `{}`",
                impl_path
            );
        }
    }
}

pub fn resolve_types(hlir: &mut HLIR, meta: &mut ProgramMetaData) -> ResolveResult<()> {
    TypeResolver::new(meta).resolve_types(hlir)
}
