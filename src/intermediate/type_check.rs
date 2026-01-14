//! The purpose of this module is to perform type checking on the resolved [`HLIR`] after reference resolution.
//! It validates that all type rules are followed, ensures type compatibility across operations, and infers
//! types where explicit annotations are absent.
//!
//! This pass walks the [`HLIR`] mutably, checking expressions, statements, and function signatures for type
//! correctness. It validates binary and unary operations, function calls, struct initialization, field access,
//! and control flow constructs (if, while, return). Type errors are collected and reported together rather than
//! failing immediately, to ensure multiple errors can be corrected at once, rather than having to fix an error
//! one at a time and recompiling to test it.
//!
//! The type checker maintains a [`TypeMap`] that associates each [`HirId`] with its inferred or validated [`Type`],
//! and validates that each function adheres to its declared signature.
//!
//! This module is exclusively focused on type checking and type inference. Reference resolution and namespace
//! management must be completed before this pass runs.

use std::collections::HashMap;

use crate::ast::{Literal, Mutability, SourceSpan};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::visitor::HLIRVisitorMut;
use crate::intermediate::hir::{HLIR, HLIRExt, HirId, OwnerDefId};

use crate::intermediate::hir::ProgramMetaData;
use crate::intermediate::hir::nodes::{
    BindingKind, Block, Expr, ExprKind, HirNode, RefPath, ResolvedID, Statement, StatementKind,
    Type,
};
use crate::intermediate::types::{KitFloat, KitInt, KitTy};

use super::hir::HLIRDisjointMut;
use super::hir::nodes::{Function, LetStatement};

macro_rules! type_fail {
    (no_span, $($arg:tt)*) => {
        TypeCheckFail::new($crate::ast::SourceSpan::null_span(), format!($($arg)*))
    };
    (on_span, $span: expr, $($arg:tt)*) => {
        TypeCheckFail::new($span, format!($($arg)*))
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        TypeCheckFail::new($crate::intermediate::hir::get_span_by_id($hlir.as_ref(), $id), format!($($arg)*))
    };
}

/// Side channel data that maps a [`HirId`] to its inferred or validated [`Type`].
pub type TypeMap = HashMap<HirId, Type>;

// Type funcs
pub type TypeResult<T> = Result<T, TypeCheckFail>;

/// Retrieves a mutable reference to a statement node by its [`HirId`].
#[inline]
fn statement_mut_by_id(
    id: HirId,
    hlir: &mut HLIRDisjointMut<'_>,
) -> TypeResult<&'static mut Statement> {
    let node = hlir.get_hir_node_mut(id).ok_or_else(|| {
        TypeCheckFail::new(SourceSpan::null_span(), "Failed to get statement node.")
    })?;

    if let HirNode::Statement(statement) = node {
        Ok(statement)
    } else {
        Err(type_fail!(
            on_span,
            node.span(),
            "Node is not a statement but rather: {:?}",
            node
        ))
    }
}

/// Type checker that validates types in the [`HLIR`] after resolution.
#[derive(Debug)]
struct TypeChecker<'a> {
    pub meta: &'a mut ProgramMetaData,

    /// Whether to perform type inference where types are not explicitly provided.
    pub should_infer: bool,

    /// Collected type checking failures.
    pub errors: Vec<TypeCheckFail>,
    /// Stack to track the expected return types in nested function contexts.
    pub return_type_stack: Vec<Type>,
}

impl<'a> TypeChecker<'a> {
    pub const fn new(meta_data: &'a mut ProgramMetaData, should_infer: bool) -> Self {
        Self {
            meta: meta_data,
            should_infer,
            errors: Vec::new(),
            return_type_stack: Vec::new(),
        }
    }
}

impl TypeChecker<'_> {
    /// Pushes the current expected return type onto the stack.
    #[inline]
    pub fn push_current_return_type(&mut self, t: Type) {
        self.return_type_stack.push(t);
    }

    /// Pops the current expected return type from the stack.
    #[inline]
    pub fn pop_current_return_type(&mut self) {
        self.return_type_stack.pop();
    }

    /// Retrieves the current expected return type from the top of the stack, if it is set.
    #[inline]
    pub fn current_expected_return(&self) -> Option<Type> {
        self.return_type_stack.last().cloned()
    }

    /// Retrieves the binding name for a given binding [`HirId`], or "Unkvar" if not found.
    #[inline]
    pub fn binding_name(hlir: &mut HLIRDisjointMut<'_>, binding_id: HirId) -> String {
        let Some(node) = hlir.get_hir_node_mut(binding_id) else {
            return "Unkvar".to_string();
        };
        if let HirNode::Binding(binding_pattern) = node {
            match &binding_pattern.kind {
                BindingKind::Ident(i) => i.str().to_string(),
                BindingKind::Tuple(bindings) => format!(
                    "({})",
                    bindings
                        .iter()
                        .map(|b| Self::binding_name(hlir, *b))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        } else {
            "Unkvar".to_string()
        }
    }

    /// Gets a human-readable type name for the given [`Type`], defaulting to `"??"` if it cannot be determined.
    #[inline]
    #[must_use]
    pub fn type_name(&self, ty: impl Into<Type>) -> String {
        self.meta.type_registry.type_name(ty)
    }
}

impl TypeChecker<'_> {
    /// Ensures that the given type is resolved.
    /// # Returns
    /// * `Ok(KitTy)` if the type is resolved.
    /// * `Err(TypeCheckFail)` if the type is unresolved.
    #[inline]
    fn resolved_type(&self, id: HirId, t: &Type, hlir: &HLIRDisjointMut<'_>) -> TypeResult<KitTy> {
        t.resolved()
            .ok_or_else(|| {
                type_fail!(
                    hlir.as_ref(),
                    id,
                    "Failed to resolve expression type, result: `{}`",
                    self.type_name(t.clone())
                )
            })
            .cloned()
    }

    /// Dereferences the given type recursively and ensures it is resolved.
    /// # Returns
    /// * `Ok(KitTy)` if the type was dereferenced and is resolved.
    /// * `Err(TypeCheckFail)` if the type could not be dereferenced unresolved.
    #[inline]
    fn derefed_resolved_type(
        &self,
        id: HirId,
        t: &Type,
        hlir: &HLIRDisjointMut<'_>,
    ) -> TypeResult<KitTy> {
        t.recursive_derefed()
            .ok_or_else(|| {
                type_fail!(
                    hlir.as_ref(),
                    id,
                    "Failed to deref to resolve expression type for `{}`",
                    self.type_name(t.clone())
                )
            })
            .cloned()
    }
}

impl TypeChecker<'_> {
    /// Retrieves the function signature (return type and parameter types) for a function call expression by its [`HirId`].
    /// # Returns
    /// * `Ok((Type, Vec<Type>))` containing the return type and parameter types if successful.
    /// * `Err(TypeCheckFail)` if there was an error retrieving the function signature.
    fn get_func_sig_by_call_expr_id(
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<(Type, Vec<Type>)> {
        let Some(node) = hlir.get_hir_node_mut(id) else {
            return Err(type_fail!(
                hlir,
                id,
                "Function signature, target {:?} is not a node?",
                id
            ));
        };
        if let HirNode::Expr(expr) = node {
            let ExprKind::Path(ref_path) = &expr.kind else {
                return Err(type_fail!(
                    hlir,
                    id,
                    "Function signature, target expression is not a path.",
                ));
            };
            let Some(ResolvedID::OwnerDef(fn_def_id)) = ref_path.resolved_id() else {
                return Err(type_fail!(
                    hlir,
                    id,
                    "Function signature, target path {:?} is not resolved to an owner?",
                    ref_path
                ));
            };
            let Some(fn_node) = hlir.owning_node_mut(fn_def_id) else {
                return Err(type_fail!(
                    hlir,
                    id,
                    "Function signature, resolved target {:?} is not an owning node?",
                    fn_def_id
                ));
            };
            let Some(func) = fn_node.hir_function_ref() else {
                return Err(type_fail!(
                    hlir,
                    id,
                    "Function signature, resolved owning node {:?} is not a function?",
                    fn_node
                ));
            };

            Ok((func.sig.output.clone(), func.sig.parameters.clone()))
        } else {
            Err(type_fail!(
                hlir,
                id,
                "Function signature, target {:?} is not an expression.",
                id
            ))
        }
    }

    /// Retrieves the type of a function parameter by its function [`OwnerDefId`] and parameter index.
    /// Also takes the expression [`HirId`] for error reporting.
    /// # Returns
    /// * `Ok(Type)` if the parameter type is found.
    /// * `Err(TypeCheckFail)` if there was an error retrieving the parameter type.
    fn get_type_of_function_param(
        func_id: OwnerDefId,
        expr_id: HirId,
        param_index: u32,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let Some(func) = hlir.owning_node_item_as::<Function>(func_id) else {
            return Err(type_fail!(
                hlir,
                expr_id,
                "Function parameter, target function id {:?} is not a function?",
                func_id
            ));
        };

        func.sig
            .parameters
            .get(param_index as usize)
            .ok_or_else(|| {
                type_fail!(
                    hlir,
                    expr_id,
                    "Function parameter, index '{}' is out of bounds!",
                    param_index
                )
            })
            .cloned()
    }
}

impl TypeChecker<'_> {
    /// Validates that the given return type matches the expected return type for the current function context.
    /// This is called when a root block is being type checked on it's final expression.
    /// # Returns
    /// * `Ok(Type)` if the return type matches the expected type.
    /// * `Err(TypeCheckFail)` if there was a type mismatch.
    fn validate_return_value(
        &self,
        id: HirId,
        t: Type,
        hlir: &HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let expected_return = self
            .current_expected_return()
            .expect("Not inside function?");
        if t == expected_return {
            Ok(t)
        } else {
            Err(type_fail!(
                hlir,
                id,
                "Function return type mismatch. Expected: {}, Found: {}",
                expected_return,
                t
            ))
        }
    }

    /// Validates that the return type of the expression with the given [`HirId`] matches the expected return type.
    /// # Returns
    /// * `Ok(Type)` if the return type matches the expected type.
    /// * `Err(TypeCheckFail)` if there was a type mismatch, or other error while evaluating the expression type.
    #[inline]
    fn validate_return_value_by_id(
        &mut self,
        expr_id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let expected_return = self.current_expected_return();
        let return_type =
            self.eval_expr_type_by_id_continued(expr_id, hlir, expected_return.as_ref())?;
        self.validate_return_value(expr_id, return_type, hlir)
    }

    /// Evaluates the type of a non-expression [`HLIR`] node (e.g., parameter or statement) by its [`HirId`].
    /// # Returns
    /// * `Ok(Type)` if the non-expression type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    fn eval_non_expr_hir_id(&self, id: HirId, hlir: &mut HLIRDisjointMut<'_>) -> TypeResult<Type> {
        let Some(node) = hlir.get_hir_node_mut(id) else {
            return Err(type_fail!(
                hlir,
                id,
                "Failed to get non expression node: {:?}",
                id
            ));
        };
        match node {
            HirNode::Param(parameter) => {
                Self::get_type_of_function_param(parameter.fn_id, id, id.id.0, hlir)
            }
            HirNode::Statement(statement) => match &statement.kind {
                StatementKind::Let(let_statement) => Ok(let_statement.ty.clone()),
                StatementKind::Item(_owner_def_id) => Err(type_fail!(
                    hlir,
                    id,
                    "eval non-expression of item statement not implemented."
                )),
                StatementKind::Expr(_hir_id) => Err(type_fail!(
                    hlir,
                    id,
                    "eval non-expression of item expr not implemented."
                )),
                StatementKind::Semi(_hir_id) => Err(type_fail!(
                    hlir,
                    id,
                    "eval non-expression of item semi-expr not implemented."
                )),
            },
            HirNode::Binding(a) => self.meta.type_map.get(&a.id).cloned().ok_or_else(|| {
                type_fail!(
                    hlir,
                    id,
                    "Failed to determine variable type of binding `{}`",
                    a.string_repr(hlir.nonmut_ref())
                )
            }),
            invalid_for_non_expr => Err(type_fail!(
                hlir,
                id,
                "Node is not valid for non-expression type eval: {:?}",
                invalid_for_non_expr
            )),
        }
    }

    /// Evaluates the type of an expression by its [`HirId`], updating the type map accordingly.
    /// # Returns
    /// * `Ok(Type)` if the expression type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    fn eval_expr_type_by_id(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!(no_span, "Failed to get expr node."))?;

        if let HirNode::Expr(expr) = node {
            let expr_ty = self.eval_expr_type(expr, hlir, None)?;
            self.meta.type_map.insert(id, expr_ty.clone());
            Ok(expr_ty)
        } else {
            Err(type_fail!(
                on_span,
                node.span(),
                "Node is not an expression, but rather `{:?}`",
                node
            ))
        }
    }

    /// Evaluates the type of an expression by its [`HirId`], updating the type map accordingly.
    /// Accepts an expected type to guide type inference.
    /// # Returns
    /// * `Ok((Type, is_literal))` if the expression type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    fn eval_expr_type_by_id_expected(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
        expected: &Type,
    ) -> TypeResult<Type> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!(no_span, "Failed to get expr node."))?;

        if let HirNode::Expr(expr) = node {
            let expr_ty = self.eval_expr_type(expr, hlir, Some(expected))?;
            let final_expr_ty = Self::check_for_coercion(&expr_ty, expected);
            self.meta.type_map.insert(id, final_expr_ty.clone());
            Ok(final_expr_ty.clone())
        } else {
            Err(type_fail!(
                on_span,
                node.span(),
                "Node is not an expression, but rather `{:?}`",
                node
            ))
        }
    }

    /// Evaluates the type of an expression by its [`HirId`], updating the type map accordingly.
    /// Optionally, accepts an expected type to guide type inference.
    /// # Returns
    /// * `Ok((Type, is_literal))` if the expression type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    #[inline]
    fn eval_expr_type_by_id_continued(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
        expected: Option<&Type>,
    ) -> TypeResult<Type> {
        if let Some(expected) = expected {
            self.eval_expr_type_by_id_expected(id, hlir, expected)
        } else {
            self.eval_expr_type_by_id(id, hlir)
        }
    }

    // This *has* to have too many lines..
    /// Evaluates the type of an expression node.
    /// # Returns
    /// * `Ok(Type)` if the expression type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    #[allow(clippy::too_many_lines)]
    fn eval_expr_type(
        &mut self,
        expr: &Expr,
        hlir: &mut HLIRDisjointMut<'_>,
        expected: Option<&Type>,
    ) -> TypeResult<Type> {
        match &expr.kind {
            ExprKind::Block(block_id) => self.eval_block_type_by_id(*block_id, expected, hlir),
            ExprKind::Literal(literal) => {
                let expected_kit = expected.and_then(|e| e.resolved());
                match literal {
                    Literal::String(_) => Ok(KitTy::String.into()),
                    Literal::Float(_) => match expected_kit {
                        Some(KitTy::Float(f)) => Ok(KitTy::Float(*f).into()),
                        _ => Ok(KitTy::Float(KitFloat::F32).into()),
                    },
                    Literal::Integer(_) => match expected_kit {
                        Some(KitTy::Int(i)) => Ok(KitTy::Int(*i).into()),
                        Some(KitTy::UInt(u)) => Ok(KitTy::UInt(*u).into()),
                        _ => Ok(KitTy::Int(KitInt::I32).into()),
                    },
                    Literal::Boolean(_) => Ok(KitTy::Boolean.into()),
                }
            }
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                let lhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs = self.eval_expr_type_by_id_expected(*hir_id1, hlir, &lhs)?;
                let lhs_r = self.resolved_type(*hir_id, &lhs, hlir)?;
                let rhs_r = self.resolved_type(*hir_id1, &rhs, hlir)?;

                lhs_r
                    .binary_op_result_type(&rhs_r, *binary_op_kind)
                    .map_or_else(
                        || {
                            Err(type_fail!(
                                hlir,
                                expr.id,
                                "Failed to determine binary operation result type! `{}` {} `{}`",
                                self.type_name(lhs),
                                binary_op_kind.symbols(),
                                self.type_name(rhs)
                            ))
                        },
                        |resulting_type| Ok(Type::Resolved(resulting_type)),
                    )
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let rhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs_r = self.resolved_type(*hir_id, &rhs, hlir)?;

                rhs_r.unary_op_result_type(*unary_op_kind).map_or_else(
                    || {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "Failed to determine unary op result type: {} `{}`",
                            unary_op_kind.symbols(),
                            self.type_name(rhs)
                        ))
                    },
                    |resulting_type| Ok(Type::Resolved(resulting_type)),
                )
            }
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                let condition_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let condition_ty_r = self.resolved_type(*hir_id, &condition_ty, hlir)?;
                if condition_ty_r != KitTy::Boolean {
                    return Err(type_fail!(
                        hlir,
                        *hir_id,
                        "If condition must be a boolean expression.",
                    ));
                }

                let if_block_ty = self.eval_block_type_by_id(*hir_id1, expected, hlir)?;

                if let Some(else_block_id) = hir_id2 {
                    let else_block_ty =
                        self.eval_expr_type_by_id_expected(*else_block_id, hlir, &if_block_ty)?;
                    if else_block_ty == if_block_ty {
                        Ok(if_block_ty)
                    } else {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "If block and else expression types do not match. If block: `{}`, else: `{}`",
                            self.type_name(if_block_ty),
                            self.type_name(else_block_ty)
                        ))
                    }
                } else {
                    Ok(if_block_ty)
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                let condition_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let condition_ty_r = self.resolved_type(*hir_id, &condition_ty, hlir)?;

                if condition_ty_r != KitTy::Boolean {
                    return Err(type_fail!(
                        hlir,
                        *hir_id,
                        "While condition must be a boolean expression."
                    ));
                }

                self.eval_block_type_by_id(*hir_id1, None, hlir)?;

                Ok(Type::unit())
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                let lhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs = self.eval_expr_type_by_id_expected(*hir_id1, hlir, &lhs)?;
                let assign_target = self.resolved_type(*hir_id, &lhs, hlir)?;
                let value_type = self.resolved_type(*hir_id1, &rhs, hlir)?;

                if assign_target == value_type {
                    Ok(Type::Resolved(assign_target))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Assign type mismatch! `{}` = `{}`",
                        self.type_name(assign_target),
                        self.type_name(value_type)
                    ))
                }
            }
            ExprKind::Call(hir_id, arg_ids) => {
                let (func_return_type, func_args) =
                    Self::get_func_sig_by_call_expr_id(*hir_id, hlir)?;

                if arg_ids.len() != func_args.len() {
                    return Err(type_fail!(
                        hlir,
                        expr.id,
                        "Function argument count mismatch. Expected: `{}`, supplied: `{}`",
                        func_args.len(),
                        arg_ids.len()
                    ));
                }

                let user_params = arg_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        Ok((
                            self.eval_expr_type_by_id_expected(*id, hlir, &func_args[i])?,
                            *id,
                        ))
                    })
                    .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                for (expected, (prov_ty, prov_id)) in func_args.iter().zip(user_params.iter()) {
                    if expected != prov_ty {
                        // TODO: Provide better error messages with suggestions in the case that
                        // the user provides for example an `i32` where a `&i32` is expected,
                        // suggesting the user to add a reference operator.
                        return Err(type_fail!(
                            hlir,
                            *prov_id,
                            "Function parameter type mismatch. Expected `{}`, found: `{}`",
                            self.type_name(expected.clone()),
                            self.type_name(prov_ty.clone())
                        ));
                    }
                }

                Ok(func_return_type)
            }
            ExprKind::MethodCall(hir_id, ident, arg_ids) => {
                let expr_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let derefed_ty = self.derefed_resolved_type(*hir_id, &expr_ty, hlir)?;

                let Some(method_def) = self
                    .meta
                    .find_ty_method_owner_def(&Type::Resolved(derefed_ty.clone()), ident.str())
                else {
                    return Err(type_fail!(
                        hlir,
                        expr.id,
                        "Unable to resolve method '{}' for type {}",
                        ident.str(),
                        self.type_name(derefed_ty)
                    ));
                };

                let (func_params, func_output) = {
                    let Some(func) = hlir
                        .nonmut_ref()
                        .owning_node(method_def)
                        .expect("Exists")
                        .hir_function_ref()
                    else {
                        return Err(type_fail!(
                            hlir,
                            *hir_id,
                            "Can't find func def '{:?}'",
                            ident
                        ));
                    };
                    assert!(func.is_method, "Func is not a method?");
                    (func.sig.parameters.clone(), func.sig.output.clone())
                };

                if func_params.len() != arg_ids.len().wrapping_add(1) {
                    return Err(type_fail!(
                        on_span,
                        ident.span,
                        "Method called with incorrect number of arguments! Expected: `{}`, supplied: `{}`",
                        func_params.len().saturating_sub(1),
                        arg_ids.len(),
                    ));
                }

                let user_params = arg_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        Ok((
                            self.eval_expr_type_by_id_expected(
                                *id,
                                hlir,
                                &func_params[i.wrapping_add(1)],
                            )?,
                            *id,
                        ))
                    })
                    .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                let arg_compare_iter = func_params.iter().skip(1).zip(user_params.iter());
                for (expected, (provided, prov_id)) in arg_compare_iter {
                    if expected != provided {
                        return Err(type_fail!(
                            hlir,
                            *prov_id,
                            "Method parameter type mismatch. Expected `{}`, found `{}`",
                            self.type_name(expected.clone()),
                            self.type_name(provided.clone())
                        ));
                    }
                }

                Ok(func_output.clone())
            }
            // ExprKind::Index(hir_id, hir_id1) => {},
            ExprKind::FieldAccess(hir_id, ident) => {
                let expr_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let derefed_ty = self.derefed_resolved_type(*hir_id, &expr_ty, hlir)?;

                match derefed_ty {
                    KitTy::Abstract(type_id) => {
                        let type_info = self
                            .meta
                            .type_registry
                            .get_from_type_id(type_id)
                            .expect("Type exists");
                        type_info.get_field_by_ident(ident.str()).map_or_else(
                            || Err(type_fail!(hlir, *hir_id, "Can't find field '{:?}'", ident)),
                            |field| Ok(field.ty.clone()),
                        )
                    }
                    KitTy::Tuple(tys) => {
                        let index: usize = ident.str().parse().map_err(|_| {
                            type_fail!(
                                hlir,
                                *hir_id,
                                "Tuple field access must be by index, found: '{}'",
                                ident.str()
                            )
                        })?;
                        tys.get(index).map_or_else(
                            || {
                                Err(type_fail!(
                                    hlir,
                                    *hir_id,
                                    "Tuple index out of bounds. Index: '{}', length: '{}'",
                                    index,
                                    tys.len()
                                ))
                            },
                            |ty| Ok(Type::Resolved(ty.clone())),
                        )
                    }
                    t => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Can't access fields of type: {:?}",
                        self.type_name(t)
                    )),
                }
            }
            ExprKind::StructInit(struct_initialisation) => {
                let RefPath::Resolved(_, r) = &struct_initialisation.ty_path else {
                    return Err(type_fail!(
                        on_span,
                        struct_initialisation.ty_path.span(),
                        "Struct initialisation type not resolved?"
                    ));
                };
                let ResolvedID::TypeDef(type_id) = r else {
                    return Err(type_fail!(
                        on_span,
                        struct_initialisation.ty_path.span(),
                        "Incorrect struct initialisation resolved type?"
                    ));
                };
                let struct_type = Type::Resolved(KitTy::Abstract(*type_id));
                let struct_adt = self
                    .meta
                    .type_registry
                    .get_from_type_id(*type_id)
                    .ok_or_else(|| {
                        type_fail!(
                            on_span,
                            struct_initialisation.ty_path.span(),
                            "Struct initialisation type not found?"
                        )
                    })?
                    .clone();

                struct_initialisation
                    .fields
                    .iter()
                    .map(|si| {
                        let expected_type = struct_adt
                            .get_field_by_ident(si.ident.str())
                            .ok_or_else(|| {
                                type_fail!(
                                    hlir,
                                    si.expr,
                                    "Struct `{}` has no field named `{}`",
                                    self.type_name(struct_type.clone()),
                                    si.ident.str()
                                )
                            })?
                            .ty
                            .clone();
                        let init_type =
                            self.eval_expr_type_by_id_expected(si.expr, hlir, &expected_type)?;

                        if init_type != expected_type {
                            return Err(type_fail!(
                                hlir,
                                si.expr,
                                "Type mismatch for field `{}` on `{}`: expected `{}`, found `{}`",
                                si.ident.str(),
                                self.type_name(struct_type.clone()),
                                self.type_name(expected_type),
                                self.type_name(init_type)
                            ));
                        }

                        Ok((init_type, si.expr))
                    })
                    .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                Ok(Type::Resolved(KitTy::Abstract(*type_id)))
            }
            ExprKind::Path(ref_path) => {
                if let Some(resolved_id) = ref_path.resolved_id() {
                    match resolved_id {
                        ResolvedID::Hir(hir_id) => self.eval_non_expr_hir_id(hir_id, hlir),
                        ResolvedID::Def(_def_id) => {
                            Err(type_fail!(hlir, expr.id, "Def resolution not implemented."))
                        }
                        ResolvedID::OwnerDef(_owner_def_id) => Err(type_fail!(
                            hlir,
                            expr.id,
                            "Owner def resolution not implemented."
                        )),
                        ResolvedID::TypeDef(_type_id) => Err(type_fail!(
                            hlir,
                            expr.id,
                            "Type def resolution not implemented."
                        )),
                    }
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Path not resolved: {:?}",
                        ref_path
                    ))
                }
            }
            ExprKind::Return(hir_id) => {
                if let Some(return_expr_id) = hir_id {
                    self.validate_return_value_by_id(*return_expr_id, hlir)
                } else {
                    let expected_return = self.current_expected_return().expect("Not in function?");
                    if expected_return.is_unit() {
                        Ok(Type::unit())
                    } else {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "Return with no value, when function expected to return: {}",
                            self.type_name(expected_return)
                        ))
                    }
                }
            }
            ExprKind::Break => {
                if let Some(expected_t) = expected {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Cannot break from loop with expected type: `{}`",
                        self.type_name(expected_t.clone())
                    ))
                } else {
                    Ok(Type::unit())
                }
            }
            ExprKind::Continue => Ok(Type::unit()),
            ExprKind::Cast(hir_id, target_type) => {
                let expr_type = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let expr_type_r = self.resolved_type(*hir_id, &expr_type, hlir)?;

                let target_type_r = target_type.resolved().ok_or_else(|| {
                    type_fail!(
                        hlir,
                        expr.id,
                        "Failed to resolve cast target type: `{}`",
                        self.type_name(target_type.clone())
                    )
                })?;

                expr_type_r.cast_result_type(target_type_r).map_or_else(
                    || {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "Cannot cast type `{}` to `{}`",
                            self.type_name(expr_type),
                            self.type_name(target_type.clone())
                        ))
                    },
                    |cast_result_type| Ok(Type::Resolved(cast_result_type)),
                )
            }
            ExprKind::Range(min_id, max_id, _) => {
                let min_type = self.eval_expr_type_by_id(*min_id, hlir)?;
                let max_type = self.eval_expr_type_by_id(*max_id, hlir)?;

                if min_type != max_type {
                    return Err(type_fail!(
                        hlir,
                        expr.id,
                        "Range bounds type mismatch. Min: `{}`, Max: `{}`",
                        self.type_name(min_type),
                        self.type_name(max_type)
                    ));
                }

                // TODO: This should have it's own type..
                Ok(min_type)
            }
            ExprKind::Loop(body_id) => {
                self.eval_block_type_by_id(*body_id, expected, hlir)?;
                if let Some(expected_t) = expected
                    && expected_t.is_resolved()
                {
                    Ok(expected_t.clone())
                } else {
                    Ok(Type::unit())
                }
            }
            ExprKind::For(_, binding_id, iterable_id, loop_block_id) => {
                let iterable_type = self.eval_expr_type_by_id(*iterable_id, hlir)?;
                let binding_statement = statement_mut_by_id(*binding_id, hlir)?;
                let binding_type = match binding_statement.kind {
                    StatementKind::Let(ref mut let_statement) => {
                        self.eval_and_infer_let_statement(*binding_id, let_statement, hlir)?
                    }
                    _ => {
                        return Err(type_fail!(
                            hlir,
                            expr.id,
                            "For loop binding is not a let statement."
                        ));
                    }
                };

                if iterable_type != binding_type {
                    return Err(type_fail!(
                        hlir,
                        expr.id,
                        "For loop iterable and binding type mismatch. Iterable: `{}`, Binding: `{}`",
                        self.type_name(iterable_type),
                        self.type_name(binding_type)
                    ));
                }

                self.eval_block_type_by_id(*loop_block_id, None, hlir)?;

                Ok(Type::unit())
            }
            ExprKind::Tuple(element_ids) => {
                let mut element_types = Vec::new();
                for element_id in element_ids {
                    match self.eval_expr_type_by_id(*element_id, hlir)? {
                        Type::Unresolved(t) => {
                            return Err(type_fail!(
                                hlir,
                                *element_id,
                                "Tuple element has unresolved type: {}",
                                self.type_name(t)
                            ));
                        }
                        Type::Resolved(t) => element_types.push(t),
                    }
                }
                // TODO: Create a proper tuple type representation
                Ok(Type::Resolved(KitTy::Tuple(element_types)))
            }
            ExprKind::Index(target_expr_id, index_expr_id) => {
                let target_type = self.eval_expr_type_by_id(*target_expr_id, hlir)?;
                let index_type = self.eval_expr_type_by_id(*index_expr_id, hlir)?;
                let target_type_r = self.resolved_type(*target_expr_id, &target_type, hlir)?;
                let index_type_r = self.resolved_type(*index_expr_id, &index_type, hlir)?;

                if !index_type_r.is_int() && !index_type_r.is_uint() {
                    return Err(type_fail!(
                        hlir,
                        *index_expr_id,
                        "Index expression must be of integer type, found: `{}`",
                        self.type_name(index_type)
                    ));
                }

                match target_type_r {
                    KitTy::Array(inner_ty, _size) => Ok(Type::Resolved(*inner_ty.clone())),
                    KitTy::Slice(inner_ty) => Ok(Type::Resolved(*inner_ty.clone())),
                    KitTy::Ref(reffed_ty) | KitTy::RefMut(reffed_ty) => match *reffed_ty {
                        KitTy::Array(inner_ty, _size) => Ok(Type::Resolved(*inner_ty.clone())),
                        KitTy::Slice(inner_ty) => Ok(Type::Resolved(*inner_ty.clone())),
                        _ => Err(type_fail!(
                            hlir,
                            *target_expr_id,
                            "Type `{}` is not indexable.",
                            self.type_name(target_type)
                        )),
                    },
                    // TODO: String indexing to char..
                    _ => Err(type_fail!(
                        hlir,
                        *target_expr_id,
                        "Type `{}` is not indexable.",
                        self.type_name(target_type)
                    )),
                }
            }
            ExprKind::ArrayInit(elems) => {
                let expected_kit = expected.and_then(|e| e.resolved());
                let mut expected_inner_ty = match expected_kit {
                    Some(KitTy::Array(inner_ty, _) | KitTy::Slice(inner_ty)) => {
                        Some(Type::Resolved(*inner_ty.clone()))
                    }
                    _ => None,
                };

                if elems.is_empty() {
                    return if let Some(expected) = expected {
                        Ok(expected.clone())
                    } else {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "Cannot infer type of empty array literal without expected type."
                        ))
                    };
                }

                for elem_id in elems {
                    let elem_type = self.eval_expr_type_by_id_continued(
                        *elem_id,
                        hlir,
                        expected_inner_ty.as_ref(),
                    )?;
                    if let Some(expected_inner) = &expected_inner_ty {
                        if &elem_type != expected_inner {
                            return Err(type_fail!(
                                hlir,
                                *elem_id,
                                "Array element type mismatch. Expected: `{}`, found: `{}`",
                                self.type_name(expected_inner.clone()),
                                self.type_name(elem_type)
                            ));
                        }
                    } else {
                        expected_inner_ty = Some(elem_type);
                    }
                }

                let elem_inner_ty = expected_inner_ty
                    .as_ref()
                    .and_then(|i| i.resolved())
                    .ok_or_else(|| {
                        type_fail!(hlir, expr.id, "Failed to resolve array element type.")
                    })?;

                match expected_kit {
                    Some(KitTy::Slice(..)) => Ok(Type::Resolved(KitTy::Slice(Box::new(
                        elem_inner_ty.clone(),
                    )))),
                    _ => Ok(Type::Resolved(KitTy::Array(
                        Box::new(elem_inner_ty.clone()),
                        elems.len(),
                    ))),
                }
            }
            ExprKind::Reference(id, mutable) => {
                let referred_type = self.eval_expr_type_by_id(*id, hlir)?;
                let referred_type_r = self.resolved_type(*id, &referred_type, hlir)?;

                Ok(Type::Resolved(match mutable {
                    Mutability::Mutable => KitTy::RefMut(Box::new(referred_type_r)),
                    Mutability::Immutable => KitTy::Ref(Box::new(referred_type_r)),
                }))
            }
        }
    }

    /// Evaluates and infers the type of a let statement, updating the statement's type if necessary.
    /// # Returns
    /// * `Ok(Type)` if the let statement type is successfully evaluated or inferred.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation or inference.
    fn eval_and_infer_let_statement(
        &mut self,
        id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let Some(init_expr_id) = let_statement.initial_value else {
            return if let_statement.ty.is_infer() {
                Err(type_fail!(
                    hlir,
                    id,
                    "Local with no initialiser must have a specified type."
                ))
            } else {
                Ok(let_statement.ty.clone())
            };
        };

        let is_inferring = self.should_infer && let_statement.ty.is_infer();

        let init_ty = if is_inferring {
            self.eval_expr_type_by_id(init_expr_id, hlir).map_err(|e| {
                self.errors.push(e.clone());
                type_fail!(
                    hlir,
                    let_statement.binding,
                    "Failed to infer type of: `{}`. {}",
                    Self::binding_name(hlir, let_statement.binding),
                    e.reason
                )
            })?
        } else {
            self.eval_expr_type_by_id_expected(init_expr_id, hlir, &let_statement.ty)?
        };

        if is_inferring {
            let_statement.ty = init_ty;
            self.store_binding_type(hlir.nonmut_ref(), let_statement.binding, &let_statement.ty);
            Ok(let_statement.ty.clone())
        } else if let_statement.ty.is_infer() {
            Err(type_fail!(
                hlir,
                id,
                "Type of local `{}` could not be deduced.",
                Self::binding_name(hlir, let_statement.binding)
            ))
        } else if init_ty != let_statement.ty {
            Err(type_fail!(
                hlir,
                id,
                "Let statement type mismatch. Tried to assign {}: {} = {}",
                Self::binding_name(hlir, let_statement.binding),
                self.type_name(let_statement.ty.clone()),
                self.type_name(init_ty)
            ))
        } else {
            self.store_binding_type(hlir.nonmut_ref(), let_statement.binding, &init_ty);
            Ok(init_ty)
        }
    }

    fn store_binding_type(&mut self, hlir: &HLIR, binding_id: HirId, ty: &Type) {
        self.meta.type_map.insert(binding_id, ty.clone());
        let Some(binding) = hlir.binding_by_id(binding_id) else {
            self.errors.push(type_fail!(
                hlir,
                binding_id,
                "Failed to find variable binding `{:?}`",
                binding_id
            ));
            return;
        };

        let Some(resolved_ty) = ty.resolved() else {
            self.errors.push(type_fail!(
                hlir,
                binding_id,
                "Type not resolved on variable binding `{}`, type not resolved: {:?}",
                binding.string_repr(hlir),
                ty
            ));
            return;
        };

        match (&binding.kind, resolved_ty) {
            (BindingKind::Ident(_), _) => {}
            (BindingKind::Tuple(binding_ids), KitTy::Tuple(element_tys)) => {
                if binding_ids.len() != element_tys.len() {
                    self.errors.push(type_fail!(
                        hlir,
                        binding_id,
                        "Tuple destructuring element count mismatch. Binding has `{}` elements, but type has `{}` elements.",
                        binding_ids.len(),
                        element_tys.len()
                    ));
                }

                for (b_id, e_ty) in binding_ids.iter().zip(element_tys.iter()) {
                    self.store_binding_type(hlir, *b_id, &Type::Resolved(e_ty.clone()));
                }
            }
            _ => {
                self.errors.push(type_fail!(
                    hlir,
                    binding_id,
                    "Variable type mismatch. Binding: {:?}, Type: {:?}",
                    binding.kind,
                    self.type_name(ty.clone())
                ));
            }
        }
    }

    /// Evaluates the type of a statement within a block.
    /// # Returns
    /// * `Ok(Type)` if the statement type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    fn eval_statement_type(
        &mut self,
        statement: &mut Statement,
        _parent_block: &Block,
        expected: Option<&Type>,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        match &mut statement.kind {
            StatementKind::Let(let_statement) => {
                self.eval_and_infer_let_statement(statement.id, let_statement, hlir)?;
                Ok(Type::unit())
            }
            StatementKind::Item(owner_def_id) => {
                self.eval_owner_def(*owner_def_id, hlir)?;
                Ok(Type::unit())
            }
            StatementKind::Expr(expr_id) => {
                self.eval_expr_type_by_id_continued(*expr_id, hlir, expected)
            }
            StatementKind::Semi(expr_id) => {
                self.eval_expr_type_by_id(*expr_id, hlir)?;
                Ok(Type::unit())
            }
        }
    }

    /// Evaluates the type of a block by its [`HirId`].
    /// # Returns
    /// * `Ok(Type)` if the block type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    #[inline]
    fn eval_block_type_by_id(
        &mut self,
        id: HirId,
        expected: Option<&Type>,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!(hlir, id, "Failed to get block node."))?;

        if let HirNode::Block(block) = node {
            self.eval_block_type(block, expected, hlir)
        } else {
            Err(type_fail!(
                on_span,
                node.span(),
                "Node is not a block, but rather: {:?}",
                node
            ))
        }
    }

    /// Checks if the statement with the given [`HirId`] is a return statement.
    /// # Returns
    /// * `Ok(bool)` indicating whether the statement is a return statement.
    /// * `Err(TypeCheckFail)` if there was an error retrieving the statement.
    #[inline]
    fn is_id_return_statement(id: HirId, hlir: &mut HLIRDisjointMut<'_>) -> TypeResult<bool> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!(hlir, id, "Failed to get statement node."))?;

        match node {
            HirNode::Statement(statement) => Ok(
                matches!(statement.kind, StatementKind::Expr(expr_id) | StatementKind::Semi(expr_id) if {
                    let expr_node = hlir
                        .get_hir_node_mut(expr_id)
                        .ok_or_else(|| type_fail!(hlir, id, "Failed to get expr node."))?;
                    if let HirNode::Expr(expr) = expr_node {
                        matches!(expr.kind, ExprKind::Return(..))
                    } else {
                        false
                    }
                }),
            ),
            _ => Ok(false),
        }
    }

    /// Evaluates the type of a block.
    /// # Returns
    /// * `Ok(Type)` if the block type is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during type evaluation.
    #[inline]
    fn eval_block_type(
        &mut self,
        block: &Block,
        expected: Option<&Type>,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let statement_count = block.statements.len();
        for (i, statement_id) in block.statements.iter().enumerate() {
            let last_statement = i.saturating_add(1) == statement_count;
            let is_return_stmt = Self::is_id_return_statement(*statement_id, hlir)?;
            let statement = statement_mut_by_id(*statement_id, hlir)?;
            let should_validate = block.root_block && !is_return_stmt;

            let expected_ty = if last_statement && should_validate {
                expected
            } else {
                None
            };

            match self.eval_statement_type(statement, block, expected_ty, hlir) {
                Ok(final_ty) => {
                    if last_statement {
                        return if should_validate {
                            self.validate_return_value(*statement_id, final_ty, hlir)
                        } else {
                            Ok(final_ty)
                        };
                    }
                }
                Err(e) => self.errors.push(e),
            }
        }
        Ok(Type::unit())
    }

    /// Evaluates an owner definition (e.g., function, use statement).
    /// This is a dispatch for type checking the internals of various owner definitions.
    /// # Returns
    /// * `Ok(())` if the owner definition is successfully evaluated.
    /// * `Err(TypeCheckFail)` if there was an error during evaluation.
    fn eval_owner_def(
        &mut self,
        owner_def_id: OwnerDefId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<()> {
        let Some(owning_node) = hlir.owning_node_mut(owner_def_id) else {
            let span = hlir
                .nonmut_ref()
                .span_by_owner_id(owner_def_id)
                .expect("Owning span exist.");
            return Err(type_fail!(
                on_span,
                span,
                "Eval owner def, failed to get owning node? {:?}",
                owner_def_id
            ));
        };
        let owning_node_item = owning_node.item_mut();
        match &mut owning_node_item.kind {
            super::hir::nodes::ItemKind::Function(function) => {
                self.visit_function_mut(function, hlir);
                Ok(())
            }
            super::hir::nodes::ItemKind::Use(_use_path) => Ok(()),
            _not_eval => {
                // These items are not evaluated by the type checker.
                Ok(())
            }
        }
    }
}

impl TypeChecker<'_> {
    fn check_for_array_to_slice_coercion<'a>(resulted: &'a KitTy, expected: &'a KitTy) -> bool {
        match (resulted, expected) {
            (KitTy::Array(resulted_inner, _), KitTy::Slice(expected_inner)) => {
                *resulted_inner == *expected_inner
            }
            _ => false,
        }
    }

    fn ref_mutability_matches(resulted: &KitTy, expected: &KitTy) -> bool {
        resulted.is_any_ref()
            && expected.is_any_ref()
            && resulted.ref_mutability() == expected.ref_mutability()
    }

    fn check_for_coercion<'a>(resulted: &'a Type, expected: &'a Type) -> &'a Type {
        let Some(expected_r) = expected.resolved() else {
            return resulted;
        };
        let Some(resulted_r) = resulted.resolved() else {
            return resulted;
        };

        if Self::ref_mutability_matches(resulted_r, expected_r)
            && let Some(expected_inner) = expected_r.inner_type()
            && let Some(resulted_inner) = resulted_r.inner_type()
            && Self::check_for_array_to_slice_coercion(resulted_inner, expected_inner)
        {
            return expected;
        }

        resulted
    }
}

impl HLIRVisitorMut<'_> for TypeChecker<'_> {
    fn visit_block_mut(&mut self, _block: &mut Block, _hlir: &mut HLIRDisjointMut<'_>) {
        // Note: Routing the visit block call like this stops all visit_expr_, etc.. from being
        // called.
        log::debug!("Visiting block.");
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_current_return_type(function.sig.output.clone());
        if let Some(func_body) = &function.body {
            for (index, param_id) in func_body.params.iter().enumerate() {
                if let Some(HirNode::Param(param)) = hlir.get_hir_node_mut(*param_id) {
                    let Some(param_type) = function.sig.parameters.get(index) else {
                        self.errors.push(type_fail!(
                            hlir,
                            param.id,
                            "Function parameter index out of bounds. Index: `{}`, total parameters: `{}`",
                            index,
                            function.sig.parameters.len()
                        ));
                        continue;
                    };
                    self.store_binding_type(hlir.nonmut_ref(), param.binding, param_type);
                    self.visit_function_param_mut(param, hlir);
                }
            }
            if let Some(HirNode::Block(block)) = hlir.get_hir_node_mut(func_body.block) {
                let expected_return = self.current_expected_return().expect("Not in function?");
                if let Err(e) = self.eval_block_type(block, Some(&expected_return), hlir) {
                    self.errors.push(e);
                }
            }
        }
        self.pop_current_return_type();
    }
}

/// Represents a type checking failure, containing the source span and reason for the failure.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeCheckFail {
    pub span: SourceSpan,
    pub reason: String,
}

impl TypeCheckFail {
    pub fn new(span: SourceSpan, reason: impl AsRef<str>) -> Self {
        Self {
            span,
            reason: reason.as_ref().to_string(),
        }
    }
}

/// This function will run the type checker stage on the resolved HIR output.
/// This will validate that all type rules are followed, as well as fill in any types that must be
/// inferred from context.
/// # Errors
/// This function will return an error if any part of the HIR fails to pass type checking.
/// The returned error will contain a diagnostic message indicating the nature and location of any failures
/// or incompatibilities as well as where in the source code they occurred.
/// This function can return multiple diagnostics messages/errors at once, to provide a more comprehensive
/// overview of all type checking issues in the HIR.
/// # Returns
/// * `Ok(())` if the type checking passes with no errors.
/// * `Err(LoweringError)` if there are type checking failures, containing details of the failures.
pub fn run_type_checker(hlir: &mut HLIR, meta: &mut ProgramMetaData) -> LowerResult<()> {
    let mut checker = TypeChecker::new(meta, true);

    checker.walk_mut(hlir);

    if checker.errors.is_empty() {
        Ok(())
    } else {
        Err(LoweringError::new(
            LoweringErrorKind::TypeCheckFail(checker.errors),
            SourceSpan::null_span(),
        ))
    }
}
