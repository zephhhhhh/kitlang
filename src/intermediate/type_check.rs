use crate::ast::Literal;

use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::visitor::HLIRVisitor;
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId};

use crate::intermediate::hir::nodes::{
    Block, Expr, ExprKind, HIRNode, ResolvedID, Statement, StatementKind, Type,
};
use crate::intermediate::types::{KitFloat, KitInt, KitTy};

use super::hir::nodes::{Function, LetStatement};

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
    fn validate_return_value(&mut self, expr_id: HirId, hlir: &HLIR) -> Option<Type> {
        if let Some(return_value_type) = self.get_type_of_expr_by_id(expr_id, hlir) {
            let expected_return = self.current_expected_return()?;
            if return_value_type == expected_return {
                Some(expected_return)
            } else {
                self.failed_check(
                    expr_id,
                    format!(
                        "Function expected return type does not match! {:?} != {:?}",
                        return_value_type, expected_return
                    ),
                );
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

    fn get_type_of_statement_by_id(
        &mut self,
        id: HirId,
        root_block: bool,
        hlir: &HLIR,
    ) -> Option<Type> {
        if let Some(node) = hlir.get_hir_node(id) {
            if let HIRNode::Statement(s) = node {
                self.get_type_of_statement(s, root_block, hlir)
            } else {
                self.failed_check(id, format!("Expected statement but found: {:?}", node))
            }
        } else {
            self.failed_check(id, "Failed to get HIR to check")
        }
    }

    fn get_type_of_statement(
        &mut self,
        statement: &Statement,
        root_block: bool,
        hlir: &HLIR,
    ) -> Option<Type> {
        match &statement.kind {
            StatementKind::Let(let_statement) => {
                self.visit_let_statement(statement.id, let_statement, hlir);
                Some(Type::unit())
            }
            StatementKind::Item(_owner_def_id) => {
                todo!("TODO ITEM!");
            }
            StatementKind::Expr(expr_id) => {
                if root_block {
                    self.validate_return_value(*expr_id, hlir)
                } else {
                    self.get_type_of_expr_by_id(*expr_id, hlir)
                }
            }
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
                return self.get_type_of_statement_by_id(*statement_id, block.root_block, hlir);
            } else {
                self.get_type_of_statement_by_id(*statement_id, block.root_block, hlir);
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
                    let user_parameter_types: Option<Vec<Type>> = hir_ids
                        .iter()
                        .map(|id| self.get_type_of_expr_by_id(*id, hlir))
                        .collect();

                    if let Some(user_params) = user_parameter_types {
                        if user_params.len() != func_args.len() {
                            return self
                                .failed_check(expr.id, "Parameters are not the same length!");
                        }

                        for (expected, provided) in func_args.iter().zip(user_params.iter()) {
                            if expected != provided {
                                return self.failed_check(
                                    expr.id,
                                    format!(
                                        "Wrong parameter type! Expected: {:?}, Provided: {:?}",
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
                    self.validate_return_value(*return_expr_id, hlir)
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

    fn visit_statement(&mut self, statement: &Statement, parent_block: &Block, hlir: &HLIR) {
        self.get_type_of_statement(statement, parent_block.root_block, hlir);
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

/// This function will run the type checker stage on the resolved HIR output.
/// This will validate that all type rules are followed, as well as fill in any types that must be
/// inferred from context.
pub fn run_type_checker(hlir: &mut HLIR) -> LowerResult<()> {
    let mut validator = TypeValidator::default();
    validator.visit_root(hlir);

    println!("============== Type checker ===============");
    for (failed_id, failed_reason) in &validator.failed_to_resolve {
        println!("{:?} : {}", failed_id, failed_reason);
    }
    println!("============================================");

    Ok(())
}
