use ::std::fmt::Debug;
use std::collections::HashMap;

use crate::intermediate::hir::nodes::{
    Block, Function, LetStatement, Parameter, RefPath, ResolvedID,
};
use crate::intermediate::hir::visitor::{HLIRDisjointMut, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId};
use crate::intermediate::resolver::errors::{
    ResolveResult, ResolverError, ResolverErrorKind, resolve_err,
};

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
    pub errors: Vec<ResolverError>,
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

    pub fn resolve(&mut self, hlir: &mut HLIR) -> ResolveResult<()> {
        self.walk_mut(hlir);

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ResolverErrorKind::ResolverErrors(self.errors.clone()).with_no_span())
        }
    }
}

impl HLIRVisitorMut<'_> for ScopeResolver {
    fn visit_block_mut(&mut self, block: &mut Block, hlir: &mut HLIRDisjointMut<'_>) {
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

pub fn resolve_scope_paths(hlir: &mut HLIR) -> ResolveResult<()> {
    ScopeResolver::new().resolve(hlir)
}
