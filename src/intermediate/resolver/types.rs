//! The purpose of this module is to resolve type references during the resolution phase of the compiler.
//! A type reference is any place where a type is referenced by its path, such as in function parameters,
//! return types, let bindings, or struct field definitions.
//!
//! This module walks the [`HLIR`] and converts unresolved type paths into resolved type IDs that can be
//! looked up in the type registry. This includes resolving types within structs, functions, let statements,
//! and impl blocks, as well as handling `this` type annotations within impl contexts.
//!
//! This module is exclusively focused on type reference resolution. Other forms of reference resolution,
//! such as local variable resolution or module/definition resolution, are handled elsewhere.

#[cfg(doc)]
use crate::intermediate::resolver::{Namespace, TypeRegistry};

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

/// A resolver that walks the [`HLIR`] and resolves type references.
/// This struct maintains state about the current path context and any errors encountered during resolution.
///
/// This stage runs after all ADTs have been registered in the [`TypeRegistry`], allowing it to resolve types to
/// their corresponding [`TypeID`]s.
///
/// # Example
/// ```ignore
/// struct ExampleStruct { ... }
///
/// fn main() {
///     let s = ExampleStruct { ... };
/// }
///
/// fn example_function(param: ExampleStruct) -> ExampleStruct { ... }
/// ```
/// In the above example, the [`TypeResolver`] would resolve the type reference `ExampleStruct` in the `let` statement
/// to the corresponding [`TypeID`] in the type registry.
///
/// It would also resolve the parameter and return types of `example_function` to the appropriate [`TypeID`].
struct TypeResolver<'a> {
    /// The current implementation type context, if within an `impl` block.
    /// This is used to resolve `self` type annotations.
    pub current_impl: Option<Type>,
    /// The stack of identifier paths representing the current path of where we are in the [`HLIR`].
    pub current_path: IdentPath,

    pub meta: &'a mut ProgramMetaData,

    /// A collection of resolver errors encountered during type resolution.
    /// These errors are accumulated and reported after the resolution process.
    /// If there are any errors in here after the process is complete, the resolution is considered to have failed.
    pub errors: Vec<ResolverError>,
}

impl<'a> TypeResolver<'a> {
    pub fn new(meta: &'a mut ProgramMetaData) -> Self {
        Self {
            current_impl: None,
            current_path: IdentPath::ROOT,
            meta,
            errors: Vec::new(),
        }
    }
}

impl TypeResolver<'_> {
    /// `Entrypoint` for resolving types in the given [`HLIR`].
    /// This function performs a mutable walk over the [`HLIR`], resolving type references.
    /// # Returns
    /// * `Ok(())` if all type references were resolved successfully.
    /// * `Err(ResolverError)` if there were any resolution errors.
    /// # Errors
    /// This function will return an error if any type references could not be resolved.
    /// The returned error may contain multiple resolution errors if there were multiple failures.
    pub fn resolve_types(&mut self, hlir: &mut HLIR) -> ResolveResult<()> {
        self.walk_mut(hlir);

        if !self.errors.is_empty() {
            return Err(ResolverErrorKind::ResolverErrors(self.errors.clone()).with_no_span());
        }

        Ok(())
    }

    /// Resolve a type from the given identifier path.
    /// This function attempts to find the type definition in the current [`Namespace`] context.
    /// If the path is root relative, it will be resolved from the root namespace, otherwise it will be
    /// resolved relative to the current module path.
    /// # Returns
    /// * `Ok(TypeID)` if the type was resolved successfully.
    /// * `Err(ResolverError)` if the type could not be resolved.
    /// # Errors
    /// This function will return an error if the type definition could not be found, describing the failure reason.
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
    /// Get a reference to the current identifier path from the top of the path stack.
    /// This represents the current module or path during the resolution process.
    #[inline]
    #[must_use]
    pub fn current_path(&self) -> &IdentPath {
        &self.current_path
    }

    /// Get a mutable reference to the current identifier path from the top of the path stack.
    /// This represents the current module or path during the resolution process.
    #[inline]
    #[must_use]
    pub fn current_path_mut(&mut self) -> &mut IdentPath {
        &mut self.current_path
    }

    /// Push a new segment onto the end of the current identifier path.
    /// # Example
    /// If our current path is `::example::inner_module` and we push `MyStruct`, our new path will be
    /// `::example::inner_module::MyStruct`.
    #[inline]
    pub fn push_to_current_path(&mut self, s: &str) {
        self.current_path_mut().push(s);
    }

    /// Pop the last segment from the current identifier path.
    /// # Example
    /// If our current path is `::example::inner_module::MyStruct` and we pop, our new path will be
    /// `::example::inner_module`.
    #[inline]
    pub fn pop_from_current_path(&mut self) {
        self.current_path_mut().pop();
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
        if module.owner_id == OwnerDefId::ROOT_NODE {
            self.super_module_mut(module, hlir);
        } else {
            self.push_to_current_path(module.ident.ident().str());
            self.super_module_mut(module, hlir);
            self.pop_from_current_path();
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

/// Resolves all types referenced by a path to a [`TypeID`] in the given [`HLIR`].
/// This function performs a mutable walk over the [`HLIR`], resolving type references.
/// # Returns
/// * `Ok(())` if all type references were resolved successfully.
/// * `Err(ResolverError)` if there were any resolution errors.
/// # Errors
/// This function will return an error if any type references could not be resolved.
/// The returned error may contain multiple resolution errors if there were multiple failures.
pub fn resolve_types(hlir: &mut HLIR, meta: &mut ProgramMetaData) -> ResolveResult<()> {
    TypeResolver::new(meta).resolve_types(hlir)
}
