use std::collections::HashMap;

use crate::ast::Mutability;

use crate::intermediate::hir::errors::LowerResult;
use crate::intermediate::hir::nodes::{
    Block, Expr, ExprKind, Function, LetStatement, Parameter, RefPath, ResolvedID, Statement,
    StatementKind, Type,
};
use crate::intermediate::hir::visitor::HLIRVisitor;
use crate::intermediate::hir::{HLIR, HirId, OwnerDefId, ProgramMetaData};

use crate::intermediate::mir::{
    BasicBlock, BasicBlockId, BlockExitKind, Body, ExitDirective, LocalDefinition, LocalId,
    LocalInfo, MIR, Operand, RValue,
};
// Aliases.
use crate::intermediate::mir::{Statement as MIRStatement, StatementKind as MIRStatementKind};
use crate::intermediate::types::KitTy;

use super::AssignTarget;

#[derive(Debug, Default)]
struct HIRToMIRBlockBuilder {
    pub directive: Option<BlockExitKind>,
    pub statements: Vec<MIRStatement>,
}

impl HIRToMIRBlockBuilder {
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }

    #[allow(dead_code)]
    pub fn push_field_assign(&mut self, target: LocalId, field_index: usize, value: RValue) {
        self.push_assign(AssignTarget::Field(target, field_index), value);
    }

    pub fn push_local_assign(&mut self, target: LocalId, value: RValue) {
        self.push_assign(AssignTarget::Local(target), value);
    }

    pub fn push_assign(&mut self, target: AssignTarget, value: RValue) {
        self.push_statement_kind(MIRStatementKind::Assign(target, value));
    }

    pub fn push_statement_kind(&mut self, kind: MIRStatementKind) {
        self.push_statement(MIRStatement { kind });
    }

    pub fn push_statement(&mut self, statement: MIRStatement) {
        self.statements.push(statement);
    }

    pub fn set_exit_kind(&mut self, kind: BlockExitKind) {
        self.directive = Some(kind);
    }

    pub fn build(self) -> Option<BasicBlock> {
        Some(BasicBlock {
            statements: self.statements,
            exit_directive: ExitDirective {
                kind: self.directive?,
            },
        })
    }
}

#[derive(Default)]
struct HIRToMIRFuncLowererState {
    pub parse_assign_target: bool,
    pub owner_target: Option<OwnerDefId>,
    pub assign_target: Option<AssignTarget>,
    pub last_block_target: Option<AssignTarget>,

    pub loop_stack: Vec<HIRToMIRLoopState>,
}

struct HIRToMIRLoopState {
    pub condition_block_id: BasicBlockId,
    pub breaks_to_update: Vec<BasicBlockId>,
}

impl HIRToMIRLoopState {
    pub fn new(condition_block_id: BasicBlockId) -> Self {
        Self {
            condition_block_id,
            breaks_to_update: Vec::new(),
        }
    }

    pub fn push_break_to_update(&mut self, block_id: BasicBlockId) {
        self.breaks_to_update.push(block_id);
    }
}

fn read_and_reset<T: Copy>(v: &mut Option<T>) -> Option<T> {
    if v.is_some() {
        let value = *v;
        *v = None;
        value
    } else {
        None
    }
}

impl HIRToMIRFuncLowererState {
    pub fn read_assign_target(&mut self) -> Option<AssignTarget> {
        read_and_reset(&mut self.assign_target)
    }

    pub fn read_last_block_target(&mut self) -> Option<AssignTarget> {
        read_and_reset(&mut self.last_block_target)
    }

    pub fn read_owner_target(&mut self) -> Option<OwnerDefId> {
        read_and_reset(&mut self.owner_target)
    }

    pub fn is_in_loop(&self) -> bool {
        !self.loop_stack.is_empty()
    }

    pub fn current_loop(&mut self) -> Option<&mut HIRToMIRLoopState> {
        self.loop_stack.last_mut()
    }
}

struct HIRToMIRFuncLowerer<'a> {
    pub program_meta_data: &'a ProgramMetaData,
    pub body: Body,
    pub func_owner_id: OwnerDefId,
    pub func_body_id: HirId,

    pub lut: HashMap<HirId, LocalId>,

    pub block_stack: Vec<HIRToMIRBlockBuilder>,

    pub state: HIRToMIRFuncLowererState,
}

impl<'a> HIRToMIRFuncLowerer<'a> {
    pub fn from_func_id(
        hlir: &HLIR,
        meta_data: &'a ProgramMetaData,
        func_id: OwnerDefId,
    ) -> Option<Body> {
        let mut f = Self {
            program_meta_data: meta_data,
            body: Body::new_empty(),
            func_owner_id: func_id,
            func_body_id: HirId::PLACEHOLDER_ID,
            lut: HashMap::new(),
            block_stack: Vec::new(),
            state: HIRToMIRFuncLowererState::default(),
        };
        if let Some(node) = hlir.owning_node(func_id) {
            if let Some(func) = node.hir_function_ref() {
                if let Some(func_body) = &func.body {
                    f.func_body_id = func_body.block;
                    f.visit_function(func, hlir);
                    return Some(f.body);
                }
            }
        }
        None
    }
}

impl HIRToMIRFuncLowerer<'_> {
    const DEBUG_BLOCK_CREATION: bool = false;
    const DEBUG_LOOP_STATE: bool = false;

    fn create_block_builder(&mut self) {
        self.block_stack.push(HIRToMIRBlockBuilder::default());
        if Self::DEBUG_BLOCK_CREATION {
            println!("Created block.")
        }
    }

    fn builder(&self) -> Option<&HIRToMIRBlockBuilder> {
        self.block_stack.last()
    }

    #[allow(dead_code)]
    fn builder_mut(&mut self) -> Option<&mut HIRToMIRBlockBuilder> {
        self.block_stack.last_mut()
    }

    fn builder_expect(&mut self) -> &HIRToMIRBlockBuilder {
        self.block_stack
            .last()
            .expect("MIR Lowering block builder doesn't exist.")
    }

    fn builder_mut_expect(&mut self) -> &mut HIRToMIRBlockBuilder {
        self.block_stack
            .last_mut()
            .expect("MIR Lowering block builder mut doesn't exist.")
    }

    fn emit_block(&mut self) -> Option<BasicBlockId> {
        if let Some(mut block_builder) = self.block_stack.pop() {
            let block_id = BasicBlockId(self.body.blocks.len() as u32);
            if block_builder.directive.is_none() {
                block_builder.set_exit_kind(BlockExitKind::Goto(block_id.next()));
                if Self::DEBUG_BLOCK_CREATION {
                    println!("Set default exit kind.")
                }
            }
            if let Some(basic_block) = block_builder.build() {
                if Self::DEBUG_BLOCK_CREATION {
                    println!("Successfully built block: {:?}", block_id);
                }
                return Some(self.body.push_block(basic_block));
            }
        }

        None
    }

    fn emit_and_replace_block(&mut self) -> Option<BasicBlockId> {
        let before_blocks = self.block_stack.len();
        let block_id = self.emit_block();
        // If the block was actually popped..
        //      Replace it..
        if self.block_stack.len() < before_blocks {
            self.create_block_builder();
            block_id
        } else {
            None
        }
    }

    fn emit_final_block(&mut self) -> Option<BasicBlockId> {
        if self.block_stack.is_empty() {
            self.create_block_builder();
        }

        let next_block_id = self.body.next_block_id();
        if let Some(builder) = self.builder_mut() {
            if let Some(exit_kind) = &builder.directive {
                match exit_kind {
                    BlockExitKind::Goto(goto_target) => {
                        if *goto_target != next_block_id {
                            self.emit_and_replace_block();
                        }
                    }
                    BlockExitKind::Return => return self.emit_block(),
                    _ => {
                        self.emit_and_replace_block();
                    }
                }
            }
        }

        let builder = self.builder_mut_expect();
        builder.push_local_assign(LocalId::RETURN_VALUE, RValue::unit());
        builder.set_exit_kind(BlockExitKind::Return);
        self.emit_block()
    }
}

impl HIRToMIRFuncLowerer<'_> {
    fn is_directive_set(&self) -> bool {
        if let Some(builder) = self.builder() {
            builder.directive.is_some()
        } else {
            false
        }
    }

    fn process_statement_expr(&mut self, _statement: &Statement, hlir: &HLIR, expr_id: HirId) {
        self.visit_expr_by_id(expr_id, hlir);
    }

    fn get_mutability_of_local(&self, local_id: LocalId) -> Mutability {
        self.body
            .locals
            .get(local_id.0 as usize)
            .map(|l| l.mutable)
            .unwrap_or(Mutability::Immutable)
    }

    fn new_temp_local_with_mut(&mut self, mutable: Mutability) -> LocalId {
        if self.state.assign_target.is_some() {
            println!("New local while assign target is active!");
            self.state.assign_target = None;
        }
        self.body.push_local(LocalDefinition {
            mutable,
            info: LocalInfo::Temp,
        })
    }

    fn new_temp_local(&mut self) -> LocalId {
        self.new_temp_local_with_mut(Mutability::Immutable)
    }

    fn last_local(&mut self) -> LocalId {
        let locals = self.body.locals.len() as u32;
        LocalId(locals.saturating_sub(1))
    }

    fn last_target(&mut self) -> AssignTarget {
        if let Some(assign_target) = self.state.read_assign_target() {
            return assign_target;
        }
        if let Some(block_target) = self.state.read_last_block_target() {
            return block_target;
        }
        self.last_local().into()
    }

    fn handle_resolved_id(&mut self, resolved: ResolvedID) {
        match resolved {
            ResolvedID::Hir(hir_id) => {
                if let Some(resolved_local) = self.lut.get(&hir_id).cloned() {
                    if self.state.parse_assign_target {
                        self.state.assign_target = Some(resolved_local.into())
                    } else {
                        // let local = self.new_temp_local();
                        // self.builder_mut_expect()
                        //     .push_assign(local, RValue::Ref(resolved_local));
                        eprintln!("Resolved local: {:?} -> {:?}", resolved_local, hir_id);
                    }
                } else {
                    eprintln!("No resolved local.");
                }
            }
            ResolvedID::Def(_def_id) => {}
            ResolvedID::OwnerDef(owner_def_id) => {
                self.state.owner_target = Some(owner_def_id);
            }
            ResolvedID::TypeDef(_type_id) => {}
        }
    }

    fn visit_expr_assigned(&mut self, expr_id: HirId, hlir: &HLIR) -> Option<AssignTarget> {
        let before_locals = self.body.locals.len();
        self.visit_expr_by_id(expr_id, hlir);
        let after_locals = self.body.locals.len();

        if let Some(assign_target) = self.state.read_assign_target() {
            Some(assign_target)
        } else if let Some(block_target) = self.state.read_last_block_target() {
            Some(block_target)
        } else if after_locals > before_locals {
            Some(LocalId(after_locals.saturating_sub(1) as u32).into())
        } else {
            None
        }
    }

    fn visit_expr_expect_owner(&mut self, expr_id: HirId, hlir: &HLIR) -> Option<OwnerDefId> {
        self.state.owner_target = None;
        self.visit_expr_by_id(expr_id, hlir);
        self.state.read_owner_target()
    }
}

impl HLIRVisitor for HIRToMIRFuncLowerer<'_> {
    fn visit_expr(&mut self, expr: &Expr, hlir: &HLIR) {
        match &expr.kind {
            ExprKind::Block(hir_id) => self.visit_block_by_id(*hir_id, hlir),
            ExprKind::Literal(literal) => {
                let local = self.new_temp_local();
                self.builder_mut_expect()
                    .push_local_assign(local, RValue::literal(literal.clone()));
            }
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                if let Some(lhs_local) = self.visit_expr_assigned(*hir_id, hlir) {
                    if let Some(rhs_local) = self.visit_expr_assigned(*hir_id1, hlir) {
                        let local = self.new_temp_local();
                        self.builder_mut_expect().push_local_assign(
                            local,
                            RValue::BinaryOp(
                                *binary_op_kind,
                                (Operand::Copy(lhs_local), Operand::Copy(rhs_local)),
                            ),
                        );
                    } else {
                        eprintln!("Failed to resolve RHS of binary op.");
                    }
                } else {
                    eprintln!("Failed to resolve LHS of binary op.");
                }
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                if let Some(rhs_local) = self.visit_expr_assigned(*hir_id, hlir) {
                    let local = self.new_temp_local();
                    self.builder_mut_expect().push_local_assign(
                        local,
                        RValue::UnaryOp(*unary_op_kind, Operand::Copy(rhs_local)),
                    );
                }
            }
            ExprKind::If(condition, true_block, else_expr) => {
                // FIXME: Really inefficient.
                if let Some(target) = self.visit_expr_assigned(*condition, hlir) {
                    // No else
                    //      True block -> Goto next block
                    //      Else block -> Goto next block after last true block (Same)
                    // Else
                    //      True block -> Goto block after last else block.
                    //      Else block -> Goto next block after last else block (Same)

                    let has_else = else_expr.is_some();

                    let branch_bb_id = self.body.next_block_id();
                    let true_start_id = branch_bb_id.next();
                    self.builder_mut_expect()
                        .set_exit_kind(BlockExitKind::Branch(
                            Operand::Copy(target),
                            true_start_id,
                            BasicBlockId::PLACEHOLDER_ID,
                        ));
                    assert!(
                        self.emit_and_replace_block().unwrap() == branch_bb_id,
                        "Blocks not equal"
                    );
                    assert!(
                        self.body.next_block_id() == true_start_id,
                        "True start id wrong!"
                    );

                    let mut is_local_set = false;
                    let if_result_local = self.new_temp_local();

                    self.visit_block_by_id(*true_block, hlir);
                    if !self.is_directive_set() {
                        let true_block_last_target = self.state.read_last_block_target();
                        let true_final_assign_value =
                            if let Some(last_target) = &true_block_last_target {
                                RValue::Unchanged(Operand::Copy(*last_target))
                            } else {
                                RValue::unit()
                            };
                        self.builder_mut_expect()
                            .push_local_assign(if_result_local, true_final_assign_value);
                        is_local_set = true;
                    }

                    let final_true_block_id = self.emit_and_replace_block().unwrap();
                    if let Some(BlockExitKind::Branch(_, _, else_block)) =
                        self.body.block_exit_kind_mut(branch_bb_id)
                    {
                        *else_block = final_true_block_id.next();
                    } else {
                        eprintln!("Failed to get branch block?");
                    }

                    if has_else {
                        self.visit_expr_by_id(else_expr.unwrap(), hlir);
                        if is_local_set && self.is_directive_set() {
                            self.emit_and_replace_block().unwrap();
                        }
                        if !self.is_directive_set() {
                            let false_block_last_target = self.state.read_last_block_target();
                            let false_final_assign_value =
                                if let Some(last_target) = &false_block_last_target {
                                    RValue::Unchanged(Operand::Copy(*last_target))
                                } else {
                                    RValue::unit()
                                };
                            self.builder_mut_expect()
                                .push_local_assign(if_result_local, false_final_assign_value);
                        }

                        let final_false_block_id = self.emit_and_replace_block().unwrap();

                        if let Some(BlockExitKind::Goto(last_true_goto)) =
                            self.body.block_exit_kind_mut(final_true_block_id)
                        {
                            *last_true_goto = final_false_block_id.next();
                        }
                    } else {
                        if !is_local_set && !has_else {
                            self.builder_mut_expect()
                                .push_local_assign(if_result_local, RValue::unit());
                        }

                        if let Some(BlockExitKind::Goto(last_true_goto)) =
                            self.body.block_exit_kind_mut(final_true_block_id)
                        {
                            *last_true_goto = final_true_block_id.next();
                        }
                    }

                    if is_local_set {
                        // println!("If local: {:?}", if_result_local);
                        self.state.last_block_target = Some(if_result_local.into());
                    }
                }
            }
            ExprKind::While(loop_condition_id, block_id) => {
                if !self.builder_expect().is_empty() {
                    self.emit_and_replace_block();
                }

                if let Some(target) = self.visit_expr_assigned(*loop_condition_id, hlir) {
                    let while_result_local = self.new_temp_local();

                    let condition_check_bb_id = self.body.next_block_id();
                    if Self::DEBUG_LOOP_STATE {
                        println!("Pushed loop stack.");
                    }
                    self.state
                        .loop_stack
                        .push(HIRToMIRLoopState::new(condition_check_bb_id));

                    let loop_body_start_id = condition_check_bb_id.next();
                    self.builder_mut_expect()
                        .set_exit_kind(BlockExitKind::Branch(
                            Operand::Copy(target),
                            loop_body_start_id,
                            BasicBlockId::PLACEHOLDER_ID,
                        ));
                    assert!(
                        self.emit_and_replace_block().unwrap() == condition_check_bb_id,
                        "Blocks not equal"
                    );

                    self.visit_block_by_id(*block_id, hlir);
                    self.builder_mut_expect()
                        .set_exit_kind(BlockExitKind::Goto(condition_check_bb_id));
                    let final_loop_body_id = self.emit_and_replace_block().unwrap();

                    if let Some(BlockExitKind::Branch(_, _, else_block)) =
                        self.body.block_exit_kind_mut(condition_check_bb_id)
                    {
                        *else_block = final_loop_body_id.next();
                    } else {
                        eprintln!("Failed to get while branch block?");
                    }

                    for break_to_update in &self
                        .state
                        .current_loop()
                        .expect("Not in loop?")
                        .breaks_to_update
                    {
                        if let Some(BlockExitKind::Goto(break_goto)) =
                            self.body.block_exit_kind_mut(*break_to_update)
                        {
                            if Self::DEBUG_LOOP_STATE {
                                println!("Updated Goto.");
                            }
                            *break_goto = final_loop_body_id.next();
                        } else {
                            eprintln!("Failed to get break goto block?");
                        }
                    }

                    if Self::DEBUG_LOOP_STATE {
                        println!("Popped loop stack.");
                    }
                    self.state.loop_stack.pop();

                    self.builder_mut_expect()
                        .push_local_assign(while_result_local, RValue::unit());
                    self.state.last_block_target = Some(while_result_local.into());
                } else {
                    println!("No loop condition target!");
                }
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                if let Some(target) = self.visit_expr_assigned(*hir_id, hlir) {
                    if let Some(local) = self.body.local(target.local_id()) {
                        if !local.mutable.is_mutable() {
                            eprintln!("Cannot assign to immutable variable!");
                        }
                    }
                    if let Some(rhs_local) = self.visit_expr_assigned(*hir_id1, hlir) {
                        self.builder_mut_expect()
                            .push_assign(target, RValue::Unchanged(Operand::Copy(rhs_local)));
                    }
                } else {
                    eprintln!("Failed to determine target for assignment!");
                }
            }
            ExprKind::Call(call_expr, args) => {
                if let Some(target) = self.visit_expr_expect_owner(*call_expr, hlir) {
                    let arg_local_ids: Option<Vec<_>> = args
                        .iter()
                        .map(|a_id| {
                            let local_id = self.visit_expr_assigned(*a_id, hlir)?;
                            Some(Operand::Copy(local_id))
                        })
                        .collect();
                    if let Some(arg_ids) = arg_local_ids {
                        if self.is_directive_set() {
                            self.emit_and_replace_block()
                                .expect("Emit block before call.");
                        }
                        let call_block_id = self.body.next_block_id();
                        let call_result_slot = self.new_temp_local();
                        self.builder_mut_expect().set_exit_kind(BlockExitKind::Call(
                            call_result_slot,
                            target,
                            arg_ids,
                            call_block_id.next(),
                        ));
                        assert!(
                            self.emit_and_replace_block().expect("Emit call block.")
                                == call_block_id,
                            "Call block id does not match expected."
                        );
                        self.state.last_block_target = Some(call_result_slot.into());
                    } else {
                        eprintln!("Failed to get all arg local ids.");
                    }
                } else {
                    eprintln!("Failed to get call target id.");
                }
            }
            ExprKind::MethodCall(hir_id, ident, args) => {
                fn is_def_method_func(hlir: &HLIR, owner_id: OwnerDefId) -> bool {
                    let Some(owning_node) = hlir.owning_node(owner_id) else {
                        return false;
                    };
                    let Some(func) = owning_node.hir_function_ref() else {
                        return false;
                    };
                    func.is_method
                }

                let Some(self_id) = self.visit_expr_assigned(*hir_id, hlir) else {
                    eprintln!("Failed to eval target method!");
                    return;
                };
                let Some(Type::Resolved(KitTy::Abstract(type_id))) =
                    self.program_meta_data.type_map.get(hir_id)
                else {
                    eprintln!("Target not abstract.");
                    return;
                };

                let to_access = self
                    .program_meta_data
                    .type_registry
                    .get_from_type_id(*type_id)
                    .expect("Type exists.");
                let Some(assoc_def) = to_access.find_associated_def(hlir, ident.str()) else {
                    eprintln!("Failed to find associated def!");
                    return;
                };

                if !is_def_method_func(hlir, assoc_def) {
                    eprintln!("Associated call is not a method!");
                    return;
                }

                let self_arg = Operand::Copy(self_id);
                let arg_local_ids: Option<Vec<_>> = std::iter::once(Some(self_arg))
                    .chain(args.iter().map(|a_id| {
                        let local_id = self.visit_expr_assigned(*a_id, hlir)?;
                        Some(Operand::Copy(local_id))
                    }))
                    .collect();

                if let Some(arg_ids) = arg_local_ids {
                    if self.is_directive_set() {
                        self.emit_and_replace_block()
                            .expect("Emit block before method call.");
                    }
                    let call_block_id = self.body.next_block_id();
                    let call_result_slot = self.new_temp_local();
                    self.builder_mut_expect().set_exit_kind(BlockExitKind::Call(
                        call_result_slot,
                        assoc_def,
                        arg_ids,
                        call_block_id.next(),
                    ));
                    assert!(
                        self.emit_and_replace_block().expect("Emit call block.") == call_block_id,
                        "Call block id does not match expected."
                    );
                    self.state.last_block_target = Some(call_result_slot.into());
                } else {
                    eprintln!("Failed to eval all args!");
                }
            }
            // ExprKind::Index(hir_id, hir_id1) => todo!(),
            ExprKind::FieldAccess(hir_id, ident) => {
                if let Some(target_local) = self.visit_expr_assigned(*hir_id, hlir) {
                    if let Some(Type::Resolved(KitTy::Abstract(type_id))) =
                        self.program_meta_data.type_map.get(hir_id)
                    {
                        let to_access = self
                            .program_meta_data
                            .type_registry
                            .get_from_type_id(*type_id)
                            .expect("Type exists.");
                        let Some(field_index) = to_access.find_field_index(ident.str()) else {
                            eprintln!("Failed to find field index");
                            return;
                        };

                        let target_local_id = target_local.local_expect();

                        let local = self
                            .new_temp_local_with_mut(self.get_mutability_of_local(target_local_id));
                        self.builder_mut_expect().push_local_assign(
                            local,
                            RValue::refer(AssignTarget::Field(target_local_id, field_index)),
                        );
                    } else {
                        eprintln!("Target not abstract.");
                    }
                } else {
                    eprintln!("Failed to eval target field local!");
                }
            }
            ExprKind::StructInit(struct_initialisation) => {
                let RefPath::Resolved(_, resolved_id) = &struct_initialisation.ty_path else {
                    eprintln!(
                        "Failed to get struct init type id. {:?}",
                        struct_initialisation.ty_path
                    );
                    return;
                };
                let ResolvedID::TypeDef(type_id) = *resolved_id else {
                    eprintln!("Resolved id is not type id.");
                    return;
                };
                if let Some(type_info) = self
                    .program_meta_data
                    .type_registry
                    .get_from_type_id(type_id)
                {
                    if type_info.get_field_count() != struct_initialisation.fields.len() {
                        eprintln!("Field count mismatch in initialisation!");
                        return;
                    }

                    let mut field_values = type_info
                        .get_fields()
                        .iter()
                        .map(|_| Operand::Unit)
                        .collect::<Vec<_>>();

                    for field_init in &struct_initialisation.fields {
                        let Some(field_index) = type_info.find_field_index(field_init.ident.str())
                        else {
                            eprintln!("Failed to find field index for {}", field_init.ident.str());
                            return;
                        };

                        if let Some(field_init_local) =
                            self.visit_expr_assigned(field_init.expr, hlir)
                        {
                            *field_values.get_mut(field_index).expect("Field index") =
                                Operand::Copy(field_init_local);
                        } else {
                            eprintln!("Failed to eval init local");
                        }
                    }

                    let struct_local = self.new_temp_local();
                    self.builder_mut_expect().push_local_assign(
                        struct_local,
                        RValue::ADT(super::ADTKind::Struct(type_id), field_values),
                    );
                } else {
                    eprintln!("Failed to get type info!");
                }
            }
            ExprKind::Path(ref_path) => {
                if let Some(resolved) = ref_path.resolved_id() {
                    self.handle_resolved_id(resolved);
                } else {
                    eprintln!("Warning: Path not resolved: {:?}", ref_path);
                }
            }
            ExprKind::Continue => {
                if self.state.is_in_loop() {
                    let current_loop_cond_bb_id = self
                        .state
                        .current_loop()
                        .expect("Not in loop?")
                        .condition_block_id;
                    self.builder_mut_expect()
                        .set_exit_kind(BlockExitKind::Goto(current_loop_cond_bb_id));
                    self.emit_and_replace_block()
                        .expect("Continue block failed to emit.");
                } else {
                    println!("Cannot continue from outside of a loop!")
                }
            }
            ExprKind::Break => {
                if self.state.is_in_loop() {
                    let break_block_id = self.body.next_block_id();
                    self.builder_mut_expect()
                        .set_exit_kind(BlockExitKind::Goto(BasicBlockId::PLACEHOLDER_ID));
                    self.state
                        .current_loop()
                        .expect("Not inside loop?")
                        .push_break_to_update(break_block_id);
                    assert!(
                        self.emit_and_replace_block().unwrap() == break_block_id,
                        "Break block id mismatch"
                    );
                } else {
                    println!("Cannot break from outside of a loop!");
                }
            }
            ExprKind::Return(hir_id) => {
                if self
                    .builder()
                    .expect("Builder doesn't exist")
                    .directive
                    .is_some()
                {
                    self.emit_and_replace_block().unwrap();
                }

                let return_value = if let Some(return_expr_id) = hir_id {
                    if let Some(target) = self.visit_expr_assigned(*return_expr_id, hlir) {
                        RValue::Unchanged(Operand::Copy(target))
                    } else {
                        panic!("Failed to get return value expression.");
                    }
                } else {
                    RValue::unit()
                };

                self.builder_mut_expect()
                    .push_local_assign(LocalId::RETURN_VALUE, return_value);
                self.builder_mut_expect()
                    .set_exit_kind(BlockExitKind::Return);
            }
            _ => {}
        }
    }

    fn visit_let_statement(&mut self, id: HirId, let_statement: &LetStatement, hlir: &HLIR) {
        let local_id = self.body.push_local(LocalDefinition {
            mutable: let_statement.mutable,
            info: LocalInfo::UserDeclared(let_statement.ident.clone()),
        });
        self.lut.insert(id, local_id);
        if let Some(init_id) = &let_statement.initial_value {
            if let Some(target) = self.visit_expr_assigned(*init_id, hlir) {
                self.builder_mut_expect()
                    .push_local_assign(local_id, RValue::Unchanged(Operand::Copy(target)));
            } else {
                eprintln!("Failed to get let statement initial expression target");
            }
        }
    }

    fn visit_statement(&mut self, statement: &Statement, parent_block: &Block, hlir: &HLIR) {
        match &statement.kind {
            StatementKind::Expr(hir_id) => {
                self.process_statement_expr(statement, hlir, *hir_id);
                // Do as a return.
                if parent_block.id == self.func_body_id {
                    // ..
                    let last_local = self.last_target();
                    let builder = self.builder_mut_expect();
                    builder.set_exit_kind(BlockExitKind::Return);
                    builder.push_statement_kind(MIRStatementKind::Assign(
                        AssignTarget::Local(LocalId::RETURN_VALUE),
                        RValue::Unchanged(Operand::Copy(last_local)),
                    ));
                } else {
                    self.state.last_block_target = Some(self.last_target())
                }
            }
            StatementKind::Semi(hir_id) => {
                self.process_statement_expr(statement, hlir, *hir_id);
                self.state.read_last_block_target();
            }
            _ => self.super_statement(statement, parent_block, hlir),
        }
    }

    fn visit_block(&mut self, block: &Block, hlir: &HLIR) {
        self.state.last_block_target = None;
        self.super_block(block, hlir);
    }

    fn visit_function_param(&mut self, parameter: &Parameter, _hlir: &HLIR) {
        let param_local_id = self
            .body
            .push_param(parameter.mutable, parameter.ident.ident.clone());
        self.lut.insert(parameter.id, param_local_id);
    }

    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        if function.owner_id == self.func_owner_id {
            self.state.parse_assign_target = true;
            self.create_block_builder();
            self.super_function(function, hlir);
            self.emit_final_block();
        }
    }
}

pub fn lower_hir_to_mir(hlir: &HLIR, type_info: &ProgramMetaData) -> LowerResult<MIR> {
    let mut bodies = HashMap::<OwnerDefId, Body>::new();
    let mut native_function_links = HashMap::<OwnerDefId, String>::new();

    for i in hlir.owner_id_iter() {
        if let Some(node) = hlir.owning_node(i) {
            if let Some(func) = node.hir_function_ref() {
                if func.native {
                    native_function_links.insert(i, func.ident.string());
                } else if let Some(result_body) =
                    HIRToMIRFuncLowerer::from_func_id(hlir, type_info, i)
                {
                    bodies.insert(i, result_body);
                }
            }
        }
    }

    Ok(MIR {
        bodies,
        native_function_links,
    })
}
