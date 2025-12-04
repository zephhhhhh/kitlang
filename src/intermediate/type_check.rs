use std::collections::HashMap;

use crate::ast::{Literal, SourceSpan};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::visitor::HLIRVisitorMut;
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::hir::nodes::{
    Block, Expr, ExprKind, HIRNode, RefPath, ResolvedID, Statement, StatementKind, Type,
};
use crate::intermediate::resolver::TypeRegistry;
use crate::intermediate::types::{KitFloat, KitInt, KitTy};

use super::hir::nodes::{Function, LetStatement};
use super::hir::visitor::HLIRDisjointMut;

macro_rules! type_fail {
    ($msg: expr) => {
        TypeCheckFail::new(SourceSpan::null_span(), $msg)
    };
    ($span: expr, $msg: expr) => {
        TypeCheckFail::new($span, $msg)
    };
    (span, $span: expr, $($arg:tt)*) => {
        TypeCheckFail::new($span, format!($($arg)*))
    };
    ($hlir: expr, $id: expr, $msg: literal) => {
        TypeCheckFail::new(get_span_by_id($id, $hlir.as_ref()), $msg)
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        TypeCheckFail::new(get_span_by_id($id, $hlir.as_ref()), format!($($arg)*))
    };
}

pub type TypeMap = HashMap<HirId, Type>;

// Type funcs
pub type TypeResult<T> = Result<T, TypeCheckFail>;

#[inline]
fn get_span_by_id(id: HirId, hlir: &HLIR) -> SourceSpan {
    hlir.span_by_hir_id(id)
        .unwrap_or_else(SourceSpan::null_span)
}

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

    pub type_map: TypeMap,

    pub should_infer: bool,

    pub errors: Vec<TypeCheckFail>,
    pub return_type_stack: Vec<Type>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(type_registry: &'a TypeRegistry, should_infer: bool) -> Self {
        Self {
            type_registry,
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
}

impl TypeChecker<'_> {
    fn resolved_type(id: HirId, t: &Type, hlir: &mut HLIRDisjointMut<'_>) -> TypeResult<KitTy> {
        t.resolved()
            .ok_or_else(|| type_fail!(hlir.as_ref(), id, "Failed to resolve expression type."))
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
            Err(type_fail!(node.span(), "Node is not an expression."))
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
                let lhs_r = Self::resolved_type(*hir_id, &lhs, hlir)?;
                let rhs_r = Self::resolved_type(*hir_id1, &rhs, hlir)?;

                if let Some(resulting_type) = lhs_r.binary_op_result_type(&rhs_r, *binary_op_kind) {
                    Ok(Type::Resolved(resulting_type))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Failed to determine binary operation result type! {:?} {:?} {:?}",
                        lhs,
                        binary_op_kind,
                        rhs
                    ))
                }
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let rhs = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let rhs_r = Self::resolved_type(*hir_id, &rhs, hlir)?;

                if let Some(resulting_type) = rhs_r.unary_op_result_type(*unary_op_kind) {
                    Ok(Type::Resolved(resulting_type))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Failed to determine unary op result type: {:?}",
                        unary_op_kind
                    ))
                }
            }
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                let condition_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let condition_ty_r = Self::resolved_type(*hir_id, &condition_ty, hlir)?;
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
                            "If block and else expression types do not match. If block: {:?}, else: {:?}",
                            if_block_ty,
                            else_block_ty
                        ))
                    }
                } else {
                    Ok(if_block_ty)
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                let condition_ty = self.eval_expr_type_by_id(*hir_id, hlir)?;
                let condition_ty_r = Self::resolved_type(*hir_id, &condition_ty, hlir)?;

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
                let assign_target = Self::resolved_type(*hir_id, &lhs, hlir)?;
                let value_type = Self::resolved_type(*hir_id1, &rhs, hlir)?;

                if assign_target == value_type {
                    Ok(Type::Resolved(assign_target))
                } else {
                    Err(type_fail!(
                        hlir,
                        expr.id,
                        "Assign type mismatch! {:?} = {:?}",
                        assign_target,
                        value_type
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
                        "Function argument count mismatch. Expected: {}, Found: {}",
                        func_args.len(),
                        user_params.len()
                    ));
                }

                for (expected, (provided, prov_id)) in func_args.iter().zip(user_params.iter()) {
                    if expected != provided {
                        return Err(type_fail!(
                            hlir,
                            *prov_id,
                            "Function parameter type mismatch. Expected: {}, Found: {}",
                            expected,
                            provided
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
                    Type::Resolved(KitTy::Abstract(type_id)) => {
                        let type_info = self
                            .type_registry
                            .get_from_type_id(type_id)
                            .expect("Type exists");
                        let Some(assoc_def) =
                            type_info.find_associated_def(hlir.nonmut_ref(), ident.str())
                        else {
                            return Err(type_fail!(
                                hlir,
                                *hir_id,
                                "Can't find associated def '{:?}'",
                                ident
                            ));
                        };

                        let node = hlir.nonmut_ref().owning_node(assoc_def).expect("Exists");
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
                                "Method called with incorrect number of arguments! Expected: {}, Supplied: {}",
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
                                    "Method parameter type mismatch. Expected: {}, Found: {}",
                                    expected,
                                    provided
                                ));
                            }
                        }

                        Ok(func.sig.output.clone())
                    }
                    Type::Unresolved(t) => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Method access type unresolved. {:?}",
                        t
                    )),
                    Type::Resolved(t) => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Can't access methods of type: {:?}",
                        t
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
                        t
                    )),
                    Type::Resolved(t) => Err(type_fail!(
                        hlir,
                        *hir_id,
                        "Can't access fields of type: {:?}",
                        t
                    )),
                }
            }
            ExprKind::StructInit(struct_initialisation) => {
                if let RefPath::Resolved(_, r) = struct_initialisation.ty_path {
                    if let ResolvedID::TypeDef(type_id) = r {
                        struct_initialisation
                            .fields
                            .iter()
                            .map(|si| Ok((self.eval_expr_type_by_id(si.expr, hlir)?, si.expr)))
                            .collect::<TypeResult<Vec<(Type, HirId)>>>()?;

                        Ok(Type::Resolved(KitTy::Abstract(type_id)))
                    } else {
                        Err(type_fail!(
                            struct_initialisation.ty_path.span(),
                            "Incorrect struct initialisation resolved type?"
                        ))
                    }
                } else {
                    Err(type_fail!(
                        struct_initialisation.ty_path.span(),
                        "Struct initialisation type not resolved?"
                    ))
                }
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
                            expected_return
                        ))
                    } else {
                        Ok(Type::unit())
                    }
                }
            }
            ExprKind::Continue | ExprKind::Break => Ok(Type::unit()),
            unk => {
                eprintln!("Error unknown expression type: {:?}", unk);
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
                let_statement.ty,
                init_ty
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
            StatementKind::Item(owner_def_id) => self.eval_owner_def(*owner_def_id, hlir),
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
            Err(type_fail!(node.span(), "Node is not a block."))
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
                        if block.root_block {
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
    ) -> TypeResult<Type> {
        let Some(owning_node) = hlir.get_owning_node_mut(owner_def_id) else {
            let span = hlir
                .nonmut_ref()
                .span_by_owner_id(owner_def_id)
                .expect("Owning span exist.");
            return Err(type_fail!(
                span,
                span,
                "Eval owner def, failed to get owning node? {:?}",
                owner_def_id
            ));
        };
        let Some(owning_node_item) = owning_node.item_mut() else {
            let span = owning_node.span().expect("Owning node span exists.");
            return Err(type_fail!(
                span,
                span,
                "Failed to get owning node item? Should be impossible."
            ));
        };
        match &mut owning_node_item.kind {
            super::hir::nodes::ItemKind::Function(function) => {
                self.visit_function_mut(function, hlir);
                Ok(Type::unit())
            }
            super::hir::nodes::ItemKind::Use(_use_path) => Ok(Type::unit()),
            _not_eval => {
                // These items are not evaluated by the type checker.
                Ok(Type::unit())
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
pub fn run_type_checker(hlir: &mut HLIR, type_registry: &TypeRegistry) -> LowerResult<TypeMap> {
    let mut checker = TypeChecker::new(type_registry, true);

    checker.walk_mut(hlir);

    if !checker.errors.is_empty() {
        Err(LoweringError::new(LoweringErrorKind::TypeCheckFail(
            checker.errors,
        )))
    } else {
        Ok(checker.type_map)
    }
}
