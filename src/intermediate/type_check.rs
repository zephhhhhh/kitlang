use crate::ast::{Literal, SourceSpan};

use crate::intermediate::hir::errors::{LowerResult, LoweringError, LoweringErrorKind};
use crate::intermediate::hir::visitor::{HLIRVisitor, HLIRVisitorMut};
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::hir::nodes::{
    Block, Expr, ExprKind, HIRNode, ResolvedID, Statement, StatementKind, Type,
};
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
    ($hlir: expr, $id: expr, $msg: literal) => {
        TypeCheckFail::new(get_span_by_id($id, $hlir.as_ref()), $msg)
    };
    ($hlir: expr, $id: expr, $($arg:tt)*) => {
        TypeCheckFail::new(get_span_by_id($id, $hlir.as_ref()), format!($($arg)*))
    };
}

// Type funcs
pub type TypeResult<T> = Result<T, TypeCheckFail>;

#[inline]
fn get_span_by_id(id: HirId, hlir: &HLIR) -> SourceSpan {
    hlir.span_by_hir_id(id)
        .unwrap_or_else(SourceSpan::null_span)
}

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

// Impl..

#[derive(Debug, Default)]
struct TypeValidator {
    pub failed_to_resolve: Vec<(HirId, String)>,

    pub return_type_stack: Vec<Type>,
}

impl TypeValidator {
    pub fn failed_check(&mut self, id: HirId, reason: impl AsRef<str>) -> Option<Type> {
        self.failed_to_resolve
            .push((id, reason.as_ref().to_string()));
        None
    }

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

impl TypeValidator {
    fn validate_return_value(&mut self, id: HirId, t: &Type) -> bool {
        let expected_return = self
            .current_expected_return()
            .expect("Not inside function?");
        if *t == expected_return {
            true
        } else {
            self.failed_check(
                id,
                format!(
                    "Function return type mismatch. Expected: {}, Found: {}",
                    expected_return, t
                ),
            );
            false
        }
    }

    fn validate_return_value_by_id(&mut self, expr_id: HirId, hlir: &HLIR) -> Option<Type> {
        if let Some(return_value_type) = self.get_type_of_expr_by_id(expr_id, hlir) {
            if self.validate_return_value(expr_id, &return_value_type) {
                Some(return_value_type)
            } else {
                None
            }
        } else {
            self.failed_check(expr_id, "Failed to determine type of return value.")
        }
    }

    fn get_type_of_function_param(
        func_id: OwnerDefId,
        param_index: u32,
        hlir: &HLIR,
    ) -> Option<Type> {
        hlir.owning_node(func_id)?
            .hir_function_ref()?
            .sig
            .parameters
            .get(param_index as usize)
            .cloned()
    }

    fn get_type_of_statement_by_id(&mut self, id: HirId, hlir: &HLIR) -> Option<Type> {
        if let Some(node) = hlir.get_hir_node(id) {
            if let HIRNode::Statement(s) = node {
                self.get_type_of_statement(s, hlir)
            } else {
                self.failed_check(id, format!("Expected statement but found: {:?}", node))
            }
        } else {
            self.failed_check(id, "Failed to get HIR to check")
        }
    }

    fn get_type_of_statement(&mut self, statement: &Statement, hlir: &HLIR) -> Option<Type> {
        match &statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement(statement.id, let_statement, hlir);
                Some(Type::unit())
            }
            StatementKind::Item(_owner_def_id) => {
                todo!("TODO ITEM!");
            }
            StatementKind::Expr(expr_id) => self.get_type_of_expr_by_id(*expr_id, hlir),
            StatementKind::Semi(expr_id) => {
                self.visit_expr_by_id(*expr_id, hlir);
                Some(Type::unit())
            }
        }
    }

    fn get_type_of_block(&mut self, block: &Block, hlir: &HLIR) -> Option<Type> {
        let statement_count = block.statements.len();
        for (i, statement_id) in block.statements.iter().enumerate() {
            if i.saturating_add(1) == statement_count {
                let final_ty = self.get_type_of_statement_by_id(*statement_id, hlir)?;
                return if block.root_block {
                    if self.validate_return_value(*statement_id, &final_ty) {
                        Some(final_ty)
                    } else {
                        None
                    }
                } else {
                    Some(final_ty)
                };
            } else {
                self.get_type_of_statement_by_id(*statement_id, hlir);
            }
        }
        Some(Type::unit())
    }

    fn get_type_of_block_by_id(&mut self, id: HirId, hlir: &HLIR) -> Option<Type> {
        if let Some(node) = hlir.get_hir_node(id) {
            if let HIRNode::Block(b) = node {
                self.get_type_of_block(b, hlir)
            } else {
                self.failed_check(id, format!("Expected block but found: {:?}", node))
            }
        } else {
            self.failed_check(id, "Failed to get HIR to check")
        }
    }

    fn get_type_of_non_expr_hir_id(&mut self, id: HirId, hlir: &HLIR) -> Option<Type> {
        match hlir.get_hir_node(id)? {
            HIRNode::Param(parameter) => {
                Self::get_type_of_function_param(parameter.fn_id, id.id.0, hlir)
            }
            HIRNode::Block(_block) => todo!(),
            HIRNode::Expr(_expr) => todo!(),
            HIRNode::Statement(statement) => match &statement.kind {
                StatementKind::Let(let_statement) => Some(let_statement.ty.clone()),
                StatementKind::Item(_owner_def_id) => todo!(),
                StatementKind::Expr(_hir_id) => todo!(),
                StatementKind::Semi(_hir_id) => todo!(),
            },
            HIRNode::Field(_struct_field) => todo!(),
            HIRNode::Path(_ref_path) => todo!(),
        }
    }

    fn get_func_sig_by_call_expr_id(id: HirId, hlir: &HLIR) -> Option<(Type, &[Type])> {
        if let HIRNode::Expr(expr) = hlir.get_hir_node(id)? {
            let ExprKind::Path(ref_path) = &expr.kind else {
                return None;
            };
            let ResolvedID::OwnerDef(fn_def_id) = ref_path.resolved_id()? else {
                return None;
            };
            let func = hlir.owning_node(fn_def_id)?.hir_function_ref()?;

            Some((func.sig.output.clone(), &func.sig.parameters))
        } else {
            None
        }
    }

    fn get_type_of_expr_by_id(&mut self, id: HirId, hlir: &HLIR) -> Option<Type> {
        let HIRNode::Expr(expr) = hlir.get_hir_node(id)? else {
            return None;
        };
        self.get_type_of_expr(expr, hlir)
    }

    fn get_type_of_expr(&mut self, expr: &Expr, hlir: &HLIR) -> Option<Type> {
        match &expr.kind {
            ExprKind::Block(block_id) => self.get_type_of_block_by_id(*block_id, hlir),
            ExprKind::Literal(literal) => match literal {
                Literal::String(_) => Some(KitTy::String.into()),
                Literal::Float(_) => Some(KitTy::Float(KitFloat::F32).into()),
                Literal::Integer(_) => Some(KitTy::Int(KitInt::I32).into()),
                Literal::Boolean(_) => Some(KitTy::Boolean.into()),
            },
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                let Some(lhs) = self.get_type_of_expr_by_id(*hir_id, hlir) else {
                    return self
                        .failed_check(*hir_id, "Failed to get type of LHS binary operation!");
                };
                let Some(rhs) = self.get_type_of_expr_by_id(*hir_id1, hlir) else {
                    return self
                        .failed_check(*hir_id1, "Failed to get type of RHS binary operation!");
                };
                let Some(lhs_r) = lhs.resolved() else {
                    return self
                        .failed_check(*hir_id, "Failed to resolved type of LHS binary operation!");
                };
                let Some(rhs_r) = rhs.resolved() else {
                    return self
                        .failed_check(*hir_id1, "Failed to resolve type of RHS binary operation!");
                };

                if let Some(resulting_type) = lhs_r.binary_op_result_type(rhs_r, *binary_op_kind) {
                    Some(Type::Resolved(resulting_type))
                } else {
                    self.failed_check(
                        expr.id,
                        format!(
                            "Failed to determine binary operation result type! {:?} {:?} {:?}",
                            lhs, binary_op_kind, rhs
                        ),
                    )
                }
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let Some(rhs) = self.get_type_of_expr_by_id(*hir_id, hlir) else {
                    return self
                        .failed_check(*hir_id, "Failed to get type of RHS unary operation!");
                };
                let Some(rhs_r) = rhs.resolved() else {
                    return self
                        .failed_check(*hir_id, "Failed to resolved type of RHS unary operation!");
                };

                if let Some(resulting_type) = rhs_r.unary_op_result_type(*unary_op_kind) {
                    Some(Type::Resolved(resulting_type))
                } else {
                    self.failed_check(expr.id, "Failed to determine unary op result type!")
                }
            }
            ExprKind::If(hir_id, hir_id1, hir_id2) => {
                let condition_type = self.get_type_of_expr_by_id(*hir_id, hlir)?;
                if let Some(condition_type_res) = condition_type.resolved() {
                    if *condition_type_res != KitTy::Boolean {
                        return self.failed_check(
                            *hir_id,
                            "If statement condition must be a boolean expression!",
                        );
                    }
                } else {
                    return self
                        .failed_check(*hir_id, "Failed to resolve if statement condition type!");
                }

                let Some(if_block_type) = self.get_type_of_block_by_id(*hir_id1, hlir) else {
                    return self.failed_check(*hir_id1, "Failed to determine type of if block.");
                };

                if let Some(else_block_id) = hir_id2 {
                    if let Some(else_block_type) = self.get_type_of_expr_by_id(*else_block_id, hlir)
                    {
                        if else_block_type == if_block_type {
                            Some(if_block_type)
                        } else {
                            self.failed_check(
                                expr.id,
                                format!("If block and else expression types do not match! True block: {:?}, Else: {:?}",
                                    if_block_type,
                                    else_block_type
                                )
                            )
                        }
                    } else {
                        self.failed_check(*else_block_id, "Failed to determine_type of else block!")
                    }
                } else {
                    Some(if_block_type)
                }
            }
            ExprKind::While(hir_id, hir_id1) => {
                let condition_type = self.get_type_of_expr_by_id(*hir_id, hlir)?;
                if let Some(condition_type_res) = condition_type.resolved() {
                    if *condition_type_res != KitTy::Boolean {
                        return self.failed_check(
                            *hir_id,
                            "While statement condition must be a boolean expression!",
                        );
                    }
                } else {
                    return self.failed_check(
                        *hir_id,
                        "Failed to resolve while statement condition type!",
                    );
                }

                let Some(_while_block_type) = self.get_type_of_block_by_id(*hir_id1, hlir) else {
                    return self.failed_check(*hir_id1, "Failed to determine type of while block.");
                };

                Some(Type::unit())
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                let Some(lhs) = self.get_type_of_expr_by_id(*hir_id, hlir) else {
                    return self.failed_check(*hir_id, "Failed to get type of assign target!");
                };
                let Some(rhs) = self.get_type_of_expr_by_id(*hir_id1, hlir) else {
                    return self.failed_check(*hir_id1, "Failed to get type of assign value!");
                };
                let Some(assign_target) = lhs.resolved() else {
                    return self.failed_check(*hir_id, "Failed to resolved type of assign target!");
                };
                let Some(value_type) = rhs.resolved() else {
                    return self.failed_check(*hir_id1, "Failed to resolve type of assign value!");
                };

                if assign_target == value_type {
                    Some(Type::Resolved(*assign_target))
                } else {
                    self.failed_check(
                        expr.id,
                        format!(
                            "Assign type mismatch! {:?} = {:?}",
                            assign_target, value_type
                        ),
                    );
                    None
                }
            }
            ExprKind::Call(hir_id, hir_ids) => {
                if let Some((func_return_type, func_args)) =
                    Self::get_func_sig_by_call_expr_id(*hir_id, hlir)
                {
                    let user_parameter_types: Option<Vec<(Type, HirId)>> = hir_ids
                        .iter()
                        .map(|id| Some((self.get_type_of_expr_by_id(*id, hlir)?, *id)))
                        .collect();

                    if let Some(user_params) = user_parameter_types {
                        if user_params.len() != func_args.len() {
                            return self.failed_check(
                                expr.id,
                                format!(
                                    "Function argument count mismatch. Expected: {}, found: {}",
                                    func_args.len(),
                                    user_params.len()
                                ),
                            );
                        }

                        for (expected, (provided, prov_id)) in
                            func_args.iter().zip(user_params.iter())
                        {
                            if expected != provided {
                                return self.failed_check(
                                    *prov_id,
                                    format!(
                                        "Function parameter type mismatch. Expected: {}, Found: {}",
                                        expected, provided
                                    ),
                                );
                            }
                        }

                        //println!("Call check passed! Return type: {:?}", func_return_type);
                        Some(func_return_type)
                    } else {
                        self.failed_check(expr.id, "Failed to determine all parameter types!")
                    }
                } else {
                    self.failed_check(*hir_id, "Failed to get call signature from id.")
                }
            }
            // ExprKind::MethodCall(hir_id, ident, hir_ids) => todo!(),
            // ExprKind::Index(hir_id, hir_id1) => todo!(),
            // ExprKind::FieldAccess(hir_id, ident) => todo!(),
            // ExprKind::StructInit(struct_initialisation) => todo!(),
            ExprKind::Path(ref_path) => {
                if let Some(resolved_id) = ref_path.resolved_id() {
                    match resolved_id {
                        ResolvedID::Hir(hir_id) => self.get_type_of_non_expr_hir_id(hir_id, hlir),
                        ResolvedID::Def(_def_id) => todo!(),
                        ResolvedID::OwnerDef(_owner_def_id) => todo!(),
                    }
                } else {
                    self.failed_check(expr.id, format!("Path not resolved: {:?}", ref_path))
                }
            }
            ExprKind::Return(hir_id) => {
                if let Some(return_expr_id) = hir_id {
                    self.validate_return_value_by_id(*return_expr_id, hlir)
                } else if !self.current_expected_return()?.is_unit() {
                    self.failed_check(
                        expr.id,
                        format!(
                            "Return with no value, when function expected to return: {:?}",
                            self.current_expected_return()?
                        ),
                    )
                } else {
                    Some(Type::Resolved(KitTy::Unit))
                }
            }
            _ => None,
        }
    }
}

impl HLIRVisitor for TypeValidator {
    fn visit_expr(&mut self, expr: &Expr, hlir: &HLIR) {
        self.get_type_of_expr(expr, hlir);
    }

    fn visit_let_statement(&mut self, id: HirId, let_statement: &LetStatement, hlir: &HLIR) {
        if let Some(init_value_id) = let_statement.initial_value {
            if let Some(init_ty) = self.get_type_of_expr_by_id(init_value_id, hlir) {
                if init_ty != let_statement.ty {
                    self.failed_check(
                        id,
                        format!(
                            "Let statement type mismatch! Tried to assign {}:{:?} = {:?}",
                            let_statement.ident.str(),
                            let_statement.ty,
                            init_ty
                        ),
                    );
                }
            } else {
                self.failed_check(
                    id,
                    format!(
                        "Failed to determine type of initial value of let statement {:?}",
                        let_statement.ident
                    ),
                );
            }
        }
    }

    fn visit_statement(&mut self, statement: &Statement, _parent_block: &Block, hlir: &HLIR) {
        self.get_type_of_statement(statement, hlir);
    }

    fn visit_block(&mut self, block: &Block, hlir: &HLIR) {
        self.get_type_of_block(block, hlir);
    }

    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        self.push_current_return_type(function.sig.output.clone());
        self.super_function(function, hlir);
        self.pop_current_return_type();
    }
}

impl TypeValidator {
    fn get_type_of_statement_mut(
        &mut self,
        statement: &mut Statement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> Option<Type> {
        match &mut statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement_mut(statement.id, let_statement, hlir);
                Some(Type::unit())
            }
            _ => None,
        }
    }

    fn get_type_of_statement_by_id_mut(
        &mut self,
        id: HirId,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> Option<Type> {
        if let Some(node) = hlir.hir_node_mut(id) {
            if let HIRNode::Statement(s) = node.value_mut() {
                self.get_type_of_statement_mut(s, hlir)
            } else {
                self.failed_check(
                    id,
                    format!("Expected statement but found: {:?}", node.value_mut()),
                )
            }
        } else {
            self.failed_check(id, "Failed to get HIR to check")
        }
    }

    pub fn infer_in_block(
        &mut self,
        block: &mut Block,
        hlir: &mut HLIRDisjointMut<'_>,
    ) -> Option<Type> {
        let statement_count = block.statements.len();
        for (i, statement_id) in block.statements.iter().enumerate() {
            if i.saturating_add(1) == statement_count {
                let final_ty =
                    self.get_type_of_statement_by_id(*statement_id, hlir.nonmut_ref())?;
                return if block.root_block {
                    if self.validate_return_value(*statement_id, &final_ty) {
                        Some(final_ty)
                    } else {
                        None
                    }
                } else {
                    Some(final_ty)
                };
            } else {
                self.get_type_of_statement_by_id_mut(*statement_id, hlir);
            }
        }
        Some(Type::unit())
    }
}

impl HLIRVisitorMut<'_> for TypeValidator {
    fn visit_expr_mut(&mut self, _expr: &mut Expr, _hlir: &mut HLIRDisjointMut<'_>) {}

    fn visit_let_statement_mut(
        &mut self,
        id: HirId,
        let_statement: &mut LetStatement,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        if let_statement.ty.is_infer() {
            let Some(init_value_id) = let_statement.initial_value else {
                self.failed_check(id, "Cannot infer type of local without initialiser.");
                return;
            };
            if let Some(init_ty) = self.get_type_of_expr_by_id(init_value_id, hlir.nonmut_ref()) {
                let_statement.ty = init_ty;
                println!(
                    "Inferred type of '{:?}' to '{}'",
                    let_statement.ident, let_statement.ty
                );
            } else {
                self.failed_check(id, "Cannot infer type of statement.");
            }
        }
    }

    fn visit_statement_mut(
        &mut self,
        statement: &mut Statement,
        parent_block: &Block,
        hlir: &mut HLIRDisjointMut<'_>,
    ) {
        match &mut statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement_mut(statement.id, let_statement, hlir)
            }
            _ => self.visit_statement(statement, parent_block, hlir.nonmut_ref()),
        }
    }

    fn visit_block_mut(&mut self, block: &mut Block, hlir: &mut HLIRDisjointMut<'_>) {
        self.infer_in_block(block, hlir);
    }

    fn visit_function_mut(&mut self, function: &mut Function, hlir: &mut HLIRDisjointMut<'_>) {
        self.push_current_return_type(function.sig.output.clone());
        self.super_function_mut(function, hlir);
        self.pop_current_return_type();
    }
}

#[derive(Debug, Default)]
struct TypeChecker {
    pub should_infer: bool,

    pub errors: Vec<TypeCheckFail>,
    pub return_type_stack: Vec<Type>,
}

impl TypeChecker {
    pub fn new(should_infer: bool) -> Self {
        Self {
            should_infer,
            errors: Vec::new(),
            return_type_stack: Vec::new(),
        }
    }

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

impl TypeChecker {
    fn resolved_type(id: HirId, t: &Type, hlir: &mut HLIRDisjointMut<'_>) -> TypeResult<KitTy> {
        t.resolved()
            .ok_or_else(|| type_fail!(hlir.as_ref(), id, "Failed to resolve expression type."))
            .cloned()
    }
}

impl TypeChecker {
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

impl TypeChecker {
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
            HIRNode::Block(_block) => todo!(),
            HIRNode::Expr(_expr) => todo!(),
            HIRNode::Statement(statement) => match &statement.kind {
                StatementKind::Let(let_statement) => Ok(let_statement.ty.clone()),
                StatementKind::Item(_owner_def_id) => todo!(),
                StatementKind::Expr(_hir_id) => todo!(),
                StatementKind::Semi(_hir_id) => todo!(),
            },
            HIRNode::Field(_struct_field) => todo!(),
            HIRNode::Path(_ref_path) => todo!(),
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
            self.eval_expr_type(expr, hlir)
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
            // ExprKind::MethodCall(hir_id, ident, hir_ids) => todo!(),
            // ExprKind::Index(hir_id, hir_id1) => todo!(),
            // ExprKind::FieldAccess(hir_id, ident) => todo!(),
            // ExprKind::StructInit(struct_initialisation) => todo!(),
            ExprKind::Path(ref_path) => {
                if let Some(resolved_id) = ref_path.resolved_id() {
                    match resolved_id {
                        ResolvedID::Hir(hir_id) => self.eval_non_expr_hir_id(hir_id, hlir),
                        ResolvedID::Def(_def_id) => todo!(),
                        ResolvedID::OwnerDef(_owner_def_id) => todo!(),
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
            StatementKind::Item(_owner_def_id) => {
                todo!("TODO ITEM!");
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
}

impl HLIRVisitorMut<'_> for TypeChecker {
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

    pub fn from_fail((id, reason): &(HirId, String), hlir: &HLIR) -> Self {
        let span = hlir.span_by_hir_id(*id).unwrap_or(SourceSpan::null_span());
        Self {
            span,
            reason: reason.clone(),
        }
    }
}

fn run_type_checker_validator(hlir: &mut HLIR) -> LowerResult<()> {
    let mut validator = TypeValidator::default();

    // Infer..
    validator.walk_mut(hlir);

    // Validate.
    //validator.visit_root(hlir);

    if !validator.failed_to_resolve.is_empty() {
        let fails: Vec<TypeCheckFail> = validator
            .failed_to_resolve
            .iter()
            .map(|v| TypeCheckFail::from_fail(v, hlir))
            .collect();
        return Err(LoweringError::new(LoweringErrorKind::TypeCheckFail(fails)));
    }

    // println!("============== Type checker ===============");
    // for (failed_id, failed_reason) in &validator.failed_to_resolve {
    //     println!("{:?} : {}", failed_id, failed_reason);
    // }
    // println!("============================================");

    Ok(())
}

fn run_type_checker_new(hlir: &mut HLIR) -> LowerResult<()> {
    let mut checker = TypeChecker::new(false);

    checker.walk_mut(hlir);

    if !checker.errors.is_empty() {
        Err(LoweringError::new(LoweringErrorKind::TypeCheckFail(
            checker.errors,
        )))
    } else {
        Ok(())
    }
}

/// This function will run the type checker stage on the resolved HIR output.
/// This will validate that all type rules are followed, as well as fill in any types that must be
/// inferred from context.
pub fn run_type_checker(hlir: &mut HLIR) -> LowerResult<()> {
    //run_type_checker_validator(hlir)
    run_type_checker_new(hlir)
}
