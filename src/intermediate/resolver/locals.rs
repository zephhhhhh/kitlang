//! The purpose of this module is to manage local scopes during the resolution phase of the compiler.
//! A local scope is a scope in which local variables are defined and can be resolved.
//! This is usually within functions, blocks, or other constructs that introduce a new scope.
//!
//! This file is *NOT* responsible for managing scopes between modules, other functions, etc.
//! That is handled by the `associated_references` resolver.
//!
//! This module is exclusively focused on local variable resolution, and ensuring that variables are correctly
//! scoped and resolved within their respective local contexts. All other references are handled elsewhere.

use ::std::fmt::Debug;
use std::collections::HashMap;

use crate::intermediate::hir::nodes::{
    BindingKind, Block, Function, HirNode, LetStatement, Parameter, RefPath, ResolvedID,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId};
use crate::intermediate::resolver::errors::{
    ResolveResult, ResolverError, ResolverErrorKind, push_resolve_err, resolve_err,
};

/// Represents a local scope in the resolver, maintaining variable definitions and their resolved IDs.
///
/// These local scopes may be nested inside a parent scopes, forming a hierarchy of scopes.
/// Expressions inside scopes may only access variables defined in their own scope or in parent scopes,
/// not variables in child scopes.
#[derive(Debug, Clone, PartialEq)]
struct LocalScope {
    /// An optional identifier for the scope, useful for debugging or error messages.
    pub scope_ident: Option<String>,
    /// A mapping from variable names to their resolved IDs within this scope.
    pub definitions: HashMap<String, ResolvedID>,
    /// An optional parent scope, allowing for nested scopes.
    pub parent: Option<Box<Self>>,
}

impl LocalScope {
    /// Create a new local scope with an optional identifier.
    #[inline]
    #[must_use]
    pub fn new(scope_ident: Option<String>) -> Self {
        Self::new_with_parent(scope_ident, None)
    }

    /// Create a new local scope with an optional identifier and a parent scope.
    #[inline]
    #[must_use]
    pub fn new_with_parent(scope_ident: Option<String>, parent: Option<Box<Self>>) -> Self {
        Self {
            scope_ident,
            definitions: HashMap::new(),
            parent,
        }
    }

    /// Create a child scope of the current scope with an optional identifier.
    #[inline]
    #[must_use]
    pub fn child_scope(&self, scope_ident: Option<String>) -> Self {
        Self::new_with_parent(scope_ident, Some(Box::new(self.clone())))
    }

    /// Check if this scope is the root scope (i.e., has no parent).
    #[inline]
    #[allow(dead_code)]
    pub const fn is_root_scope(&self) -> bool {
        self.parent.is_none()
    }

    /// Add a new definition, ensuring that the name is unique within this scope.
    /// # Returns
    /// `true` if the definition was added successfully, `false` if the name already exists.
    #[inline]
    pub fn add_definition_unique(&mut self, name: &str, id: ResolvedID) -> bool {
        if self.definitions.contains_key(name) {
            return false;
        }

        self.definitions.insert(name.to_string(), id).is_none()
    }

    /// Add a new definition, redefining the value that was already there, if exists.
    /// # Returns
    /// `true` if the definition overwrote an existing one, `false` if it was newly added.
    #[inline]
    pub fn add_definition_overwrite(&mut self, name: &str, id: ResolvedID) -> bool {
        self.definitions.insert(name.to_string(), id).is_some()
    }

    /// Add a new definition, returning an error if the name already exists in this scope.
    /// # Returns
    /// * `Ok(())` if the definition was added successfully
    /// * `Err(ResolverError)` if the name already exists.
    #[allow(dead_code)]
    #[inline]
    pub fn add_definition_result(&mut self, name: &str, id: ResolvedID) -> ResolveResult<()> {
        if self.add_definition_unique(name, id) {
            Ok(())
        } else {
            Err(resolve_err!(
                no_span,
                "Variable `{}` is already defined!",
                name
            ))
        }
    }

    /// Find a definition in the current scope or any parent scopes.
    /// # Returns
    /// * `Some(ResolvedID)` if the definition is found.
    /// * `None` if the definition is not found.
    #[inline]
    #[must_use]
    pub fn find_definition(&self, name: &str) -> Option<ResolvedID> {
        self.definitions.get(name).map_or_else(
            || {
                self.parent
                    .as_ref()
                    .and_then(|parent| parent.find_definition(name))
            },
            |id| Some(*id),
        )
    }
}

/// A resolver that manages local scopes and resolves local variable references in the HLIR.
/// We keep track of resolver state in this struct, and use it to implement the [`HLIR`] visitor traits.
/// In this case, the scope resolver only requires one mutable pass over the [`HLIR`] to resolve local references.
#[derive(Default, Debug, Clone)]
struct ScopeResolver {
    pub scope: Vec<LocalScope>,
    /// Collected resolver errors during the resolution process.
    pub errors: Vec<ResolverError>,
}

impl ScopeResolver {
    /// Push a new local scope onto the scope stack.
    #[inline]
    pub fn push_scope(&mut self, local_scope: LocalScope) {
        self.scope.push(local_scope);
    }

    /// Pop the current local scope from the scope stack.
    #[inline]
    pub fn pop_scope(&mut self) {
        self.scope.pop();
    }

    /// Get a reference to the current local scope.
    #[inline]
    pub fn current_scope(&self) -> &LocalScope {
        self.scope.last().expect("Must have valid scope.")
    }

    /// Get a mutable reference to the current local scope.
    #[inline]
    #[must_use]
    pub fn current_scope_mut(&mut self) -> &mut LocalScope {
        self.scope.last_mut().expect("Must have valid scope.")
    }

    /// Create a new child scope of the current scope, with an optional identifier, and push it onto the scope stack.
    #[inline]
    pub fn push_child_scope(&mut self, scope_ident: Option<String>) {
        self.push_scope(self.current_scope().child_scope(scope_ident));
    }

    /// Executes `f` within a new local scope with an optional identifier.
    /// The new scope is pushed onto the scope stack before executing `f`,
    /// and popped off the stack after `f` completes.
    #[inline]
    fn with_scope<F>(&mut self, scope_ident: Option<String>, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.push_scope(LocalScope::new(scope_ident));
        f(self);
        self.pop_scope();
    }

    /// Executes `f` within a new child scope of the current scope with an optional identifier.
    /// The new scope is pushed onto the scope stack before executing `f`,
    /// and popped off the stack after `f` completes.
    #[inline]
    fn with_child_scope<F>(&mut self, scope_ident: Option<String>, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.push_child_scope(scope_ident);
        f(self);
        self.pop_scope();
    }

    /// `Entrypoint` to resolve local variable references in the given HLIR.
    /// This function performs a mutable walk over the HLIR, resolving local variable references.
    /// # Returns
    /// * `Ok(())` if all local variable references were resolved successfully.
    /// * `Err(ResolverError)` if there were any resolution errors.
    /// # Errors
    /// This function will return an error if any local variable references could not be resolved.
    /// The returned error may contain multiple resolution errors if there were multiple failures.
    #[inline]
    pub fn resolve(&mut self, hlir: &mut HLIR) -> ResolveResult<()> {
        self.walk_mut(hlir);

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ResolverErrorKind::ResolverErrors(self.errors.clone()).with_no_span())
        }
    }
}

impl ScopeResolver {
    fn handle_pattern_from_id(&mut self, hlir: &HLIR, id: HirId, allow_dupes: bool) {
        let Some(HirNode::Binding(pattern)) = hlir.get_hir_node(id) else {
            push_resolve_err!(self, no_span, "Expected binding pattern for let statement.");
            return;
        };

        match &pattern.kind {
            BindingKind::Ident(ident) => {
                if allow_dupes {
                    self.current_scope_mut()
                        .add_definition_overwrite(ident.str(), pattern.id.into());
                } else if let Err(e) = self
                    .current_scope_mut()
                    .add_definition_result(ident.str(), pattern.id.into())
                {
                    self.errors.push(e);
                }
            }
            BindingKind::Tuple(sub_ids) => {
                for sub_id in sub_ids {
                    self.handle_pattern_from_id(hlir, *sub_id, allow_dupes);
                }
            }
        }
    }
}

impl HLIRVisitorMut<'_> for ScopeResolver {
    fn visit_block_mut(&mut self, block: &mut Block, hlir: &mut HLIRDisjointMut<'_>) {
        if block.root_block {
            self.super_block_mut(block, hlir);
        } else {
            self.with_child_scope(None, |r| {
                r.super_block_mut(block, hlir);
            });
        }
    }

    fn visit_function_param_mut(
        &mut self,
        parameter: &mut Parameter,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        self.handle_pattern_from_id(hlir.nonmut_ref(), parameter.binding, false);
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        let has_body = function.body.is_some();
        if has_body {
            self.with_scope(Some(function.ident.string()), |r| {
                r.super_function_mut(function, hlir);
            });
        } else {
            self.super_function_mut(function, hlir);
        }
    }

    fn visit_let_statement_mut(
        &mut self,
        id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        self.handle_pattern_from_id(hlir.nonmut_ref(), let_statement.binding, true);

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

/// Resolves local variable references in the given HLIR.
/// This function performs a mutable walk over the HLIR, resolving local variable references.
/// # Returns
/// * `Ok(())` if all local variable references were resolved successfully.
/// * `Err(ResolverError)` if there were any resolution errors.
/// # Errors
/// This function will return an error if any local variable references could not be resolved.
/// The returned error may contain multiple resolution errors if there were multiple failures.
pub fn resolve_scope_paths(hlir: &mut HLIR) -> ResolveResult<()> {
    ScopeResolver::default().resolve(hlir)
}
