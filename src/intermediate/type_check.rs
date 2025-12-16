use std::collections::HashMap;

use crate::ast::{IdentPath, Literal, SourceSpan};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::visitor::HLIRVisitorMut;
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::hir::nodes::{
    Block, Expr, ExprKind, HIRNode, RefPath, ResolvedID, Statement, StatementKind, Type,
};
use crate::intermediate::resolver::{Namespace, TypeRegistry};
use crate::intermediate::types::{KitFloat, KitInt, KitTy};

use super::hir::nodes::{Function, LetStatement};
use super::hir::visitor::HLIRDisjointMut;

use log::*;

macro_rules! type_fail {
    ($msg: expr) => {
        TypeCheckFail::new(SourceSpan::null_span(), $msg)
    };
    (on_span, $span: expr, $($arg:tt)*) => {
        TypeCheckFail::new($span, format!($($arg)*))
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        TypeCheckFail::new(crate::intermediate::hir::get_span_by_id($hlir.as_ref(), $id), format!($($arg)*))
    };
}

pub type TypeMap = HashMap<HirId, Type>;

// Type funcs
pub type TypeResult<T> = Result<T, TypeCheckFail>;

#[inline]
fn statement_mut_by_id(
    id: HirId,
    hlir: &mut HLIRDisjointMut<'_>,
) -> TypeResult<&'static mut Statement> {
    let node = hlir.get_hir_node_mut(id).ok_or_else(|| {
        TypeCheckFail::new(SourceSpan::null_span(), "Failed to get statement node.")
    })?;

    if let HIRNode::Statement(statement) = node {
        Ok(statement)
    } else {
        Err(TypeCheckFail::new(node.span(), "Node is not a statement"))
    }
}

#[derive(Debug)]
struct TypeChecker<'a> {
    pub type_registry: &'a TypeRegistry,
    pub namespace: &'a Namespace,
    pub type_map: TypeMap,

    pub should_infer: bool,

    pub errors: Vec<TypeCheckFail>,
    pub return_type_stack: Vec<Type>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(
        type_registry: &'a TypeRegistry,
        namespace: &'a Namespace,
        should_infer: bool,
    ) -> Self {
        Self {
            type_registry,
            namespace,
            type_map: TypeMap::new(),
            should_infer,
            errors: Vec::new(),
            return_type_stack: Vec::new(),
        }
    }
}

impl TypeChecker<'_> {
    pub fn push_current_return_type(&mut self, t: Type) {
        self.return_type_stack.push(t);
    }

    pub fn pop_current_return_type(&mut self) {
        self.return_type_stack.pop();
    }

    pub fn current_expected_return(&self) -> Option<Type> {
        self.return_type_stack.last().cloned()
    }

    pub fn try_type_name(&self, ty: impl Into<Type>) -> Option<String> {
        match ty.into() {
            Type::Unresolved(ty) => ty.get_type_ident(),
            Type::Resolved(KitTy::Abstract(ty_id)) => {
                let abs_ty = self.type_registry.get_from_type_id(ty_id)?;
                let type_path = abs_ty.defined_in.extend_ident(&abs_ty.type_ident.ident);
                Some(type_path.to_string())
            }
            Type::Resolved(kit_ty) => kit_ty.to_type_str(),
        }
    }

    pub fn type_name(&self, ty: impl Into<Type>) -> String {
        self.try_type_name(ty)
            .unwrap_or_else(|| String::from("UnknownType"))
    }

    // TODO: Make this consistent with ProgramMetaData.
    #[inline]
    fn find_ty_method_owner_def(&self, ty: KitTy, method_ident: &str) -> Option<OwnerDefId> {
        match ty {
            KitTy::Abstract(adt_id) => {
                let adt = self.type_registry.get_from_type_id(adt_id)?;
                self.namespace.find_method_owner_def(
                    &adt.defined_in,
                    adt.type_ident.str(),
                    method_ident,
                )
            }
            t => self.namespace.find_method_owner_def(
                &IdentPath::new_empty(true),
                &t.to_type_str()?,
                method_ident,
            ),
        }
    }
}

impl TypeChecker<'_> {
    fn resolved_type(
        &self,
        id: HirId,
        t: &Type,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<KitTy> {
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
}

impl TypeChecker<'_> {
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
        if let HIRNode::Expr(expr) = node {
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
            let Some(fn_node) = hlir.get_owning_node_mut(fn_def_id) else {
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

    fn get_type_of_function_param(
        func_id: OwnerDefId,
        expr_id: HirId,
        param_index: u32,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let Some(fn_node) = hlir.get_owning_node_mut(func_id) else {
            return Err(type_fail!(
                hlir,
                expr_id,
                "Function parameter, target function id {:?} is not a node?",
                func_id
            ));
        };
        let Some(func) = fn_node.hir_function_ref() else {
            return Err(type_fail!(
                hlir,
                expr_id,
                "Function parameter, resolved function owning node {:?} is not a function?",
                fn_node
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
    fn validate_return_value(
        &mut self,
        id: HirId,
        t: Type,
        hlir: &mut HLIRDisjointMut<'_>,
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

    fn validate_return_value_by_id(
        &mut self,
        expr_id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let return_type = self.eval_expr_type_by_id(expr_id, hlir)?;
        self.validate_return_value(expr_id, return_type, hlir)
    }

    fn eval_non_expr_hir_id(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let Some(node) = hlir.get_hir_node_mut(id) else {
            return Err(type_fail!(
                hlir,
                id,
                "Failed to get non expression node: {:?}",
                id
            ));
        };
        match node {
            HIRNode::Param(parameter) => {
                Self::get_type_of_function_param(parameter.fn_id, id, id.id.0, hlir)
            }
            HIRNode::Statement(statement) => match &statement.kind {
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
            invalid_for_non_expr => Err(type_fail!(
                hlir,
                id,
                "Node is not valid for non-expression type eval: {:?}",
                invalid_for_non_expr
            )),
        }
    }

    fn eval_expr_type_by_id(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!("Failed to get expr node."))?;

        if let HIRNode::Expr(expr) = node {
            let expr_ty = self.eval_expr_type(expr, hlir)?;
            self.type_map.insert(id, expr_ty.clone());
            Ok(expr_ty)
        } else {
            Err(type_fail!(
                on_span,
                node.span(),
                "Node is not an expression."
            ))
        }
    }

    fn eval_expr_type(&mut self, expr: &Expr, hlir: &mut HLIRDisjointMut<'_>) -> TypeResult<Type> {
        match &expr.kind {
            ExprKind::Block(block_id) => self.eval_block_type_by_id(*block_id, hlir),
            ExprKind::Literal(literal) => match literal {
                Literal::String(_) => Ok(KitTy::String.into()),
                Literal::Float(_) => Ok(KitTy::Float(KitFloat::F32).into()),
                Literal::Integer(_) => Ok(KitTy::Int(KitInt::I32).into()),
                Literal::Boolean(_) => Ok(KitTy::Boolean.into()),
            },
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                let lhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs = self.eval_expr_type_by_id(*hir_id1, hlir)?;
                let lhs_r = self.resolved_type(*hir_id, &lhs, hlir)?;
                let rhs_r = self.resolved_type(*hir_id1, &rhs, hlir)?;

                if let Some(resulting_type) = lhs_r.binary_op_result_type(&rhs_r, *binary_op_kind) {
                    Ok(Type::Resolved(resulting_type))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Failed to determine binary operation result type! `{}` {} `{}`",
                        self.type_name(lhs),
                        binary_op_kind.symbols(),
                        self.type_name(rhs)
                    ))
                }
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let rhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs_r = self.resolved_type(*hir_id, &rhs, hlir)?;

                if let Some(resulting_type) = rhs_r.unary_op_result_type(*unary_op_kind) {
                    Ok(Type::Resolved(resulting_type))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Failed to determine unary op result type: {} `{}`",
                        unary_op_kind.symbols(),
                        self.type_name(rhs)
                    ))
                }
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

                let if_block_ty = self.eval_block_type_by_id(*hir_id1, hlir)?;

                if let Some(else_block_id) = hir_id2 {
                    let else_block_ty = self.eval_expr_type_by_id(*else_block_id, hlir)?;
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

                self.eval_block_type_by_id(*hir_id1, hlir)?;

                Ok(Type::unit())
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                let lhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs = self.eval_expr_type_by_id(*hir_id1, hlir)?;
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
            ExprKind::Call(hir_id, hir_ids) => {
                let (func_return_type, func_args) =
                    Self::get_func_sig_by_call_expr_id(*hir_id, hlir)?;
                let user_params = hir_ids
                    .iter()
                    .map(|id| Ok((self.eval_expr_type_by_id(*id, hlir)?, *id)))
                    .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                if user_params.len() != func_args.len() {
                    return Err(type_fail!(
                        hlir,
                        expr.id,
                        "Function argument count mismatch. Expected: {}, supplied: {}",
                        func_args.len(),
                        user_params.len()
                    ));
                }

                for (expected, (provided, prov_id)) in func_args.iter().zip(user_params.iter()) {
                    if expected != provided {
                        return Err(type_fail!(
                            hlir,
                            *prov_id,
                            "Function parameter type mismatch. Expected `{}`, found: `{}`",
                            self.type_name(expected.clone()),
                            self.type_name(provided.clone())
                        ));
                    }
                }

                Ok(func_return_type)
            }
            ExprKind::MethodCall(hir_id, ident, arg_ids) => {
                let expr_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let user_params = arg_ids
                    .iter()
                    .map(|id| Ok((self.eval_expr_type_by_id(*id, hlir)?, *id)))
                    .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                match expr_ty {
                    Type::Resolved(t) => {
                        let Some(method_def) = self.find_ty_method_owner_def(t, ident.str()) else {
                            return Err(type_fail!(
                                hlir,
                                expr.id,
                                "Unable to resolve method '{}' for type {}",
                                ident.str(),
                                self.type_name(t)
                            ));
                        };

                        let node = hlir.nonmut_ref().owning_node(method_def).expect("Exists");
                        let Some(func) = node.hir_function_ref() else {
                            return Err(type_fail!(
                                hlir,
                                *hir_id,
                                "Can't find func def '{:?}'",
                                ident
                            ));
                        };

                        assert!(func.is_method, "Func is not a method?");

                        if func.sig.parameters.len() != user_params.len().wrapping_add(1) {
                            return Err(type_fail!(
                                hlir,
                                *hir_id,
                                "Method called with incorrect number of arguments! Expected: {}, supplied: {}",
                                func.sig.parameters.len().saturating_sub(1),
                                user_params.len(),
                            ));
                        }

                        let arg_compare_iter =
                            func.sig.parameters.iter().skip(1).zip(user_params.iter());
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

                        Ok(func.sig.output.clone())
                    }
                    Type::Unresolved(t) => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Method access type unresolved. {:?}",
                        self.type_name(t)
                    )),
                }
            }
            // ExprKind::Index(hir_id, hir_id1) => {},
            ExprKind::FieldAccess(hir_id, ident) => {
                let expr_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                match expr_ty {
                    Type::Resolved(KitTy::Abstract(type_id)) => {
                        let type_info = self
                            .type_registry
                            .get_from_type_id(type_id)
                            .expect("Type exists");
                        if let Some(field) = type_info.get_field_by_ident(ident.str()) {
                            Ok(field.ty.clone())
                        } else {
                            Err(type_fail!(hlir, *hir_id, "Can't find field '{:?}'", ident))
                        }
                    }
                    Type::Unresolved(t) => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Field access type unresolved. {:?}",
                        self.type_name(t)
                    )),
                    Type::Resolved(t) => Err(type_fail!(
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
                let struct_adt =
                    self.type_registry
                        .get_from_type_id(*type_id)
                        .ok_or_else(|| {
                            type_fail!(
                                on_span,
                                struct_initialisation.ty_path.span(),
                                "Struct initialisation type not found?"
                            )
                        })?;

                struct_initialisation
                    .fields
                    .iter()
                    .map(|si| {
                        let init_type = self.eval_expr_type_by_id(si.expr, hlir)?;
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
                    if !expected_return.is_unit() {
                        Err(type_fail!(
                            hlir,
                            expr.id,
                            "Return with no value, when function expected to return: {}",
                            self.type_name(expected_return)
                        ))
                    } else {
                        Ok(Type::unit())
                    }
                }
            }
            ExprKind::Continue | ExprKind::Break => Ok(Type::unit()),
            ExprKind::Cast(hir_id, target_type) => {
                let expr_type = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let expr_type_r = self.resolved_type(*hir_id, &expr_type, hlir)?;

                let target_type_r = *target_type.resolved().ok_or_else(|| {
                    type_fail!(
                        hlir,
                        expr.id,
                        "Failed to resolve cast target type: `{}`",
                        self.type_name(target_type.clone())
                    )
                })?;

                if let Some(cast_result_type) = expr_type_r.cast_result_type(&target_type_r) {
                    Ok(Type::Resolved(cast_result_type))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Cannot cast type `{}` to `{}`",
                        self.type_name(expr_type),
                        self.type_name(target_type.clone())
                    ))
                }
            }
            unk => {
                error!("Error unknown expression type: {:?}", unk);
                Ok(Type::unit())
            }
        }
    }

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

        let init_ty = self.eval_expr_type_by_id(init_expr_id, hlir).map_err(|e| {
            if is_inferring {
                type_fail!(
                    hlir,
                    id,
                    "Failed to infer type of: '{}'. {}",
                    let_statement.ident.str(),
                    e.reason
                )
            } else {
                e
            }
        })?;

        if is_inferring {
            let_statement.ty = init_ty;
            Ok(let_statement.ty.clone())
        } else if let_statement.ty.is_infer() {
            Err(type_fail!(
                hlir,
                id,
                "Type of local '{}' could not be deduced.",
                let_statement.ident.str()
            ))
        } else if init_ty != let_statement.ty {
            Err(type_fail!(
                hlir,
                id,
                "Let statement type mismatch. Tried to assign {}: {} = {}",
                let_statement.ident.str(),
                self.type_name(let_statement.ty.clone()),
                self.type_name(init_ty.clone())
            ))
        } else {
            Ok(init_ty.clone())
        }
    }

    fn eval_statement_type(
        &mut self,
        statement: &mut Statement,
        _parent_block: &Block,
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
            StatementKind::Expr(expr_id) => self.eval_expr_type_by_id(*expr_id, hlir),
            StatementKind::Semi(expr_id) => {
                self.eval_expr_type_by_id(*expr_id, hlir)?;
                Ok(Type::unit())
            }
        }
    }

    fn eval_block_type_by_id(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!("Failed to get block node."))?;

        if let HIRNode::Block(block) = node {
            self.eval_block_type(block, hlir)
        } else {
            Err(type_fail!(on_span, node.span(), "Node is not a block."))
        }
    }

    fn is_id_return_statement(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<bool> {
        let node = hlir
            .get_hir_node_mut(id)
            .ok_or_else(|| type_fail!("Failed to get statement node."))?;

        match node {
            HIRNode::Statement(statement) => Ok(
                matches!(statement.kind, StatementKind::Expr(expr_id) | StatementKind::Semi(expr_id) if {
                    let expr_node = hlir
                        .get_hir_node_mut(expr_id)
                        .ok_or_else(|| type_fail!("Failed to get expr node."))?;
                    if let HIRNode::Expr(expr) = expr_node {
                        matches!(expr.kind, ExprKind::Return(..))
                    } else {
                        false
                    }
                }),
            ),
            _ => Ok(false),
        }
    }

    fn eval_block_type(
        &mut self,
        block: &mut Block,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<Type> {
        let statement_count = block.statements.len();
        for (i, statement_id) in block.statements.iter().enumerate() {
            let statement = statement_mut_by_id(*statement_id, hlir)?;
            match self.eval_statement_type(statement, block, hlir) {
                Ok(final_ty) => {
                    if i.saturating_add(1) == statement_count {
                        // Validate..
                        let is_return_stmt = self.is_id_return_statement(*statement_id, hlir)?;
                        if block.root_block && !is_return_stmt {
                            return self.validate_return_value(*statement_id, final_ty, hlir);
                        } else {
                            return Ok(final_ty);
                        }
                    }
                }
                Err(e) => self.errors.push(e),
            }
        }
        Ok(Type::unit())
    }

    fn eval_owner_def(
        &mut self,
        owner_def_id: OwnerDefId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> TypeResult<()> {
        let Some(owning_node) = hlir.get_owning_node_mut(owner_def_id) else {
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
        let Some(owning_node_item) = owning_node.item_mut() else {
            let span = owning_node.span().expect("Owning node span exists.");
            return Err(type_fail!(
                on_span,
                span,
                "Failed to get owning node item? Should be impossible."
            ));
        };
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

impl HLIRVisitorMut<'_> for TypeChecker<'_> {
    fn visit_block_mut(&mut self, block: &mut Block, hlir: &mut HLIRDisjointMut<'_>) {
        // Note: Routing the visit block call like this stops all visit_expr_, etc.. from being
        // called.
        if let Err(e) = self.eval_block_type(block, hlir) {
            self.errors.push(e);
        }
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_current_return_type(function.sig.output.clone());
        self.super_function_mut(function, hlir);
        self.pop_current_return_type();
    }
}

#[derive(Debug, Clone, PartialEq)]
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
pub fn run_type_checker(
    hlir: &mut HLIR,
    type_registry: &TypeRegistry,
    namespace: &Namespace,
) -> LowerResult<TypeMap> {
    let mut checker = TypeChecker::new(type_registry, namespace, true);

    checker.walk_mut(hlir);

    if !checker.errors.is_empty() {
        Err(LoweringError::new(LoweringErrorKind::TypeCheckFail(
            checker.errors,
        )))
    } else {
        Ok(checker.type_map)
    }
}
