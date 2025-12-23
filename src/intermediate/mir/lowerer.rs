use std::collections::HashMap;

use crate::ast::{BinaryOpKind, Mutability};

use crate::intermediate::hir::errors::{LowerResult, lowering_err, push_lower_err};
use crate::intermediate::hir::nodes::{
    BindingKind, Block, Expr, ExprKind, Function, HirNode, LetStatement, Parameter, RefPath,
    ResolvedID, Statement, StatementKind, StructInitialisation, Type,
};
use crate::intermediate::hir::visitor::HLIRVisitor;
use crate::intermediate::hir::{
    HLIR, HirId, LoweringError, LoweringErrorKind, OwnerDefId, ProgramMetaData,
};

use crate::intermediate::mir::{
    BasicBlock, BasicBlockId, BlockExitKind, Body, CastKind, ExitDirective, LocalDefinition,
    LocalId, LocalInfo, MIR, Operand, RValue,
};
// Aliases.
use crate::intermediate::mir::{Statement as MIRStatement, StatementKind as MIRStatementKind};
use crate::intermediate::types::KitTy;

use super::AssignTarget;

use log::{debug, warn};

#[derive(Debug, Default)]
struct HIRToMIRBlockBuilder {
    pub directive: Option<BlockExitKind>,
    pub statements: Vec<MIRStatement>,
}

impl HIRToMIRBlockBuilder {
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }

    #[allow(dead_code)]
    pub fn push_field_assign(&mut self, target: LocalId, field_index: usize, value: RValue) {
        self.push_assign(AssignTarget::Field(target, field_index), value);
    }

    #[inline]
    pub fn push_local_assign(&mut self, target: LocalId, value: RValue) {
        self.push_assign(AssignTarget::Local(target), value);
    }

    #[inline]
    pub fn push_assign(&mut self, target: AssignTarget, value: RValue) {
        self.push_statement_kind(MIRStatementKind::Assign(target, value));
    }

    #[inline]
    pub fn push_statement_kind(&mut self, kind: MIRStatementKind) {
        self.push_statement(MIRStatement { kind });
    }

    #[inline]
    pub fn push_statement(&mut self, statement: MIRStatement) {
        self.statements.push(statement);
    }

    #[inline]
    pub fn push_goto(&mut self, target: BasicBlockId) {
        self.set_exit_kind(BlockExitKind::Goto(target));
    }

    #[inline]
    pub fn push_return(&mut self, value: RValue) {
        self.push_local_assign(LocalId::RETURN_VALUE, value);
        self.set_exit_kind(BlockExitKind::Return);
    }

    #[inline]
    pub fn push_branch(
        &mut self,
        condition: Operand,
        true_block: BasicBlockId,
        else_block: BasicBlockId,
    ) {
        self.set_exit_kind(BlockExitKind::Branch(condition, true_block, else_block));
    }

    #[inline]
    pub fn push_call(
        &mut self,
        result_slot: LocalId,
        target: OwnerDefId,
        args: Vec<Operand>,
        continue_block: BasicBlockId,
    ) {
        self.set_exit_kind(BlockExitKind::Call(
            result_slot,
            target,
            args,
            continue_block,
        ));
    }

    #[inline]
    pub fn set_exit_kind(&mut self, kind: BlockExitKind) {
        self.directive = Some(kind);
    }

    #[inline]
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
    pub const fn new(condition_block_id: BasicBlockId) -> Self {
        Self {
            condition_block_id,
            breaks_to_update: Vec::new(),
        }
    }

    pub fn push_break_to_update(&mut self, block_id: BasicBlockId) {
        self.breaks_to_update.push(block_id);
    }
}

const fn read_and_reset<T: Copy>(v: &mut Option<T>) -> Option<T> {
    if v.is_some() {
        let value = *v;
        *v = None;
        value
    } else {
        None
    }
}

impl HIRToMIRFuncLowererState {
    pub const fn read_assign_target(&mut self) -> Option<AssignTarget> {
        read_and_reset(&mut self.assign_target)
    }

    pub const fn read_last_block_target(&mut self) -> Option<AssignTarget> {
        read_and_reset(&mut self.last_block_target)
    }

    pub const fn read_owner_target(&mut self) -> Option<OwnerDefId> {
        read_and_reset(&mut self.owner_target)
    }

    pub const fn is_in_loop(&self) -> bool {
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

    pub param_bindings: Vec<(HirId, LocalId)>,

    pub lut: HashMap<HirId, LocalId>,

    pub block_stack: Vec<HIRToMIRBlockBuilder>,

    pub state: HIRToMIRFuncLowererState,

    pub errors: Vec<LoweringError>,
}

impl<'a> HIRToMIRFuncLowerer<'a> {
    pub fn from_func_id(
        hlir: &HLIR,
        meta_data: &'a ProgramMetaData,
        func_id: OwnerDefId,
    ) -> Result<Body, Vec<LoweringError>> {
        let mut f = Self {
            program_meta_data: meta_data,
            body: Body::new_empty(),
            func_owner_id: func_id,
            func_body_id: HirId::PLACEHOLDER_ID,
            param_bindings: Vec::new(),
            lut: HashMap::new(),
            block_stack: Vec::new(),
            state: HIRToMIRFuncLowererState::default(),
            errors: Vec::new(),
        };

        if let Some(node) = hlir.owning_node(func_id)
            && let Some(func) = node.hir_function_ref()
            && let Some(func_body) = &func.body
        {
            f.func_body_id = func_body.block;
            f.visit_function(func, hlir);
        }

        if f.errors.is_empty() {
            Ok(f.body)
        } else {
            Err(f.errors)
        }
    }
}

impl HIRToMIRFuncLowerer<'_> {
    const DEBUG_BLOCK_CREATION: bool = false;

    fn create_block_builder(&mut self) {
        self.block_stack.push(HIRToMIRBlockBuilder::default());
        if Self::DEBUG_BLOCK_CREATION {
            debug!("Created block.");
        }
    }

    fn builder(&self) -> Option<&HIRToMIRBlockBuilder> {
        self.block_stack.last()
    }

    #[allow(dead_code)]
    fn builder_mut(&mut self) -> Option<&mut HIRToMIRBlockBuilder> {
        self.block_stack.last_mut()
    }

    fn builder_expect(&self) -> &HIRToMIRBlockBuilder {
        self.block_stack
            .last()
            .expect("MIR Lowering block builder doesn't exist.")
    }

    fn builder_mut_expect(&mut self) -> &mut HIRToMIRBlockBuilder {
        self.block_stack
            .last_mut()
            .expect("MIR Lowering block builder mut doesn't exist.")
    }

    #[allow(clippy::cast_possible_truncation)]
    fn emit_block(&mut self) -> Option<BasicBlockId> {
        if let Some(mut block_builder) = self.block_stack.pop() {
            let block_id = BasicBlockId(self.body.blocks.len() as u32);
            if block_builder.directive.is_none() {
                block_builder.set_exit_kind(BlockExitKind::Goto(block_id.next()));
                if Self::DEBUG_BLOCK_CREATION {
                    debug!("Set default exit kind.");
                }
            }
            if let Some(basic_block) = block_builder.build() {
                if Self::DEBUG_BLOCK_CREATION {
                    debug!("Successfully built block: {block_id:?}");
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
        if let Some(builder) = self.builder_mut()
            && let Some(exit_kind) = &builder.directive
        {
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

        self.builder_mut_expect().push_return(RValue::unit());
        self.emit_block()
    }
}

impl HIRToMIRFuncLowerer<'_> {
    #[inline]
    #[must_use]
    fn is_directive_set(&self) -> bool {
        self.builder()
            .is_some_and(|builder| builder.directive.is_some())
    }

    #[inline]
    fn process_statement_expr(&mut self, _statement: &Statement, hlir: &HLIR, expr_id: HirId) {
        self.visit_expr_by_id(expr_id, hlir);
    }

    #[inline]
    #[must_use]
    fn get_mutability_of_local(&self, local_id: LocalId) -> Mutability {
        self.body
            .locals
            .get(local_id.0 as usize)
            .map_or(Mutability::Immutable, |l| l.mutable)
    }

    #[inline]
    #[must_use]
    fn new_temp_local_with_mut(&mut self, mutable: Mutability) -> LocalId {
        if self.state.assign_target.is_some() {
            warn!("New local while assign target is active!");
            self.state.assign_target = None;
        }
        self.body.push_local(LocalDefinition {
            mutable,
            info: LocalInfo::Temp,
        })
    }

    #[inline]
    #[must_use]
    fn new_temp_local(&mut self) -> LocalId {
        self.new_temp_local_with_mut(Mutability::Immutable)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    const fn last_local(&self) -> LocalId {
        let locals = self.body.locals.len() as u32;
        LocalId(locals.saturating_sub(1))
    }

    #[inline]
    #[must_use]
    fn last_target(&mut self) -> AssignTarget {
        if let Some(assign_target) = self.state.read_assign_target() {
            return assign_target;
        }
        if let Some(block_target) = self.state.read_last_block_target() {
            return block_target;
        }
        self.last_local().into()
    }

    #[inline]
    fn handle_resolved_id(&mut self, resolved: ResolvedID, hlir: &HLIR) {
        match resolved {
            ResolvedID::Hir(hir_id) => {
                if let Some(resolved_local) = self.lut.get(&hir_id).copied() {
                    if self.state.parse_assign_target {
                        self.state.assign_target = Some(resolved_local.into());
                    }
                } else {
                    push_lower_err!(
                        self,
                        hlir,
                        hir_id,
                        "Failed to resolve local variable: `{:?}`",
                        hir_id
                    );
                }
            }
            ResolvedID::Def(_def_id) => {}
            ResolvedID::OwnerDef(owner_def_id) => {
                self.state.owner_target = Some(owner_def_id);
            }
            ResolvedID::TypeDef(_type_id) => {}
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
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

    #[inline]
    #[must_use]
    fn visit_expr_expect_owner(&mut self, expr_id: HirId, hlir: &HLIR) -> Option<OwnerDefId> {
        self.state.owner_target = None;
        self.visit_expr_by_id(expr_id, hlir);
        self.state.read_owner_target()
    }
}

impl HIRToMIRFuncLowerer<'_> {
    /// Updates the else target of a branch block.
    /// # Returns
    /// `true` if the target was updated, `false` otherwise.
    #[inline]
    fn update_branch_else_target(
        &mut self,
        branch_id: BasicBlockId,
        new_target: BasicBlockId,
    ) -> bool {
        if let Some(BlockExitKind::Branch(_, _, else_block)) =
            self.body.block_exit_kind_mut(branch_id)
        {
            *else_block = new_target;
            true
        } else {
            false
        }
    }

    /// Updates the true and else targets of a branch block.
    /// # Returns
    /// `true` if the target was updated, `false` otherwise.
    #[inline]
    fn update_branch_targets(
        &mut self,
        branch_id: BasicBlockId,
        new_true_target: BasicBlockId,
        new_false_target: BasicBlockId,
    ) -> bool {
        if let Some(BlockExitKind::Branch(_, true_block, else_block)) =
            self.body.block_exit_kind_mut(branch_id)
        {
            *true_block = new_true_target;
            *else_block = new_false_target;
            true
        } else {
            false
        }
    }

    /// Updates the goto target of a goto block.
    /// # Returns
    /// `true` if the target was updated, `false` otherwise.
    #[inline]
    fn update_goto_target(&mut self, block_id: BasicBlockId, new_target: BasicBlockId) -> bool {
        if let Some(BlockExitKind::Goto(goto)) = self.body.block_exit_kind_mut(block_id) {
            *goto = new_target;
            true
        } else {
            false
        }
    }

    /// Updates all break goto targets from the current loop to the given block.
    /// # Parameters
    /// - `expr_id`: The expression ID of the full loop expression used for error reporting.
    /// - `block_id`: The block ID to update the break targets to.
    #[inline]
    fn update_goto_targets_from_loop(
        &mut self,
        hlir: &HLIR,
        expr_id: HirId,
        block_id: BasicBlockId,
    ) {
        let breaks_to_update = if let Some(current_loop) = self.state.current_loop() {
            current_loop.breaks_to_update.clone()
        } else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Tried to update goto targets while not in loop?"
            );
            return;
        };
        for break_to_update in breaks_to_update {
            self.update_goto_target(break_to_update, block_id);
        }
    }

    /// Converts a block target to an [`RValue`].
    #[inline]
    fn block_target_to_rvalue(target: Option<AssignTarget>) -> RValue {
        target
            .as_ref()
            .map_or_else(RValue::unit, |t| RValue::copy(*t))
    }
}

impl HIRToMIRFuncLowerer<'_> {
    fn lower_if(
        &mut self,
        hlir: &HLIR,
        if_expr_id: HirId,
        condition: HirId,
        true_block: HirId,
        else_expr: Option<&HirId>,
    ) {
        // No else
        //      True block -> Goto next block
        //      Else block -> Goto next block after last true block (Same)
        // Else
        //      True block -> Goto block after last else block.
        //      Else block -> Goto next block after last else block (Same)

        let Some(target) = self.visit_expr_assigned(condition, hlir) else {
            push_lower_err!(self, hlir, condition, "Failed to eval if condition.");
            return;
        };

        let branch_bb_id = self.body.next_block_id();
        let true_start_id = branch_bb_id.next();

        self.builder_mut_expect().push_branch(
            Operand::Copy(target),
            true_start_id,
            BasicBlockId::PLACEHOLDER_ID,
        );
        self.emit_and_replace_block().unwrap();

        let mut is_local_set = false;
        let if_result_local = self.new_temp_local();

        self.visit_block_by_id(true_block, hlir);

        if !self.is_directive_set() {
            let true_block_value =
                Self::block_target_to_rvalue(self.state.read_last_block_target());
            self.builder_mut_expect()
                .push_local_assign(if_result_local, true_block_value);
            is_local_set = true;
        }

        let final_true_block_id = self.emit_and_replace_block().unwrap();
        if !self.update_branch_else_target(branch_bb_id, final_true_block_id.next()) {
            push_lower_err!(self, hlir, if_expr_id, "Failed to update if branch block.");
        }

        if let Some(else_id) = else_expr {
            self.visit_expr_by_id(*else_id, hlir);

            if is_local_set && self.is_directive_set() {
                self.emit_and_replace_block().unwrap();
            }

            if !self.is_directive_set() {
                let false_value = Self::block_target_to_rvalue(self.state.read_last_block_target());
                self.builder_mut_expect()
                    .push_local_assign(if_result_local, false_value);
            }

            let final_false_block_id = self.emit_and_replace_block().unwrap();

            if !self.update_goto_target(final_true_block_id, final_false_block_id.next())
                && !self.body.is_block_exit_return(final_true_block_id)
            {
                push_lower_err!(
                    self,
                    hlir,
                    if_expr_id,
                    "Failed to update else branch block."
                );
            }
        } else {
            if !is_local_set {
                self.builder_mut_expect()
                    .push_local_assign(if_result_local, RValue::unit());
            }
            if !self.update_goto_target(final_true_block_id, final_true_block_id.next())
                && !self.body.is_block_exit_return(final_true_block_id)
            {
                push_lower_err!(self, hlir, if_expr_id, "Failed to update if goto block.");
            }
        }

        if is_local_set {
            self.state.last_block_target = Some(if_result_local.into());
        }
    }

    fn lower_loop(&mut self, hlir: &HLIR, loop_expr_id: HirId, loop_block_id: HirId) {
        if !self.builder_expect().is_empty() {
            self.emit_and_replace_block();
        }

        let loop_result_local = self.new_temp_local();

        let loop_body_start_id = self.body.next_block_id();
        self.state
            .loop_stack
            .push(HIRToMIRLoopState::new(loop_body_start_id));

        self.visit_block_by_id(loop_block_id, hlir);

        self.builder_mut_expect().push_goto(loop_body_start_id);
        let final_loop_body_id = self.emit_and_replace_block().unwrap();

        self.update_goto_targets_from_loop(hlir, loop_expr_id, final_loop_body_id.next());

        self.state.loop_stack.pop();
        self.builder_mut_expect()
            .push_local_assign(loop_result_local, RValue::unit());
        self.state.last_block_target = Some(loop_result_local.into());
    }

    fn lower_for(
        &mut self,
        hlir: &HLIR,
        for_expr_id: HirId,
        binding_id: HirId,
        iterable_id: HirId,
        block_id: HirId,
    ) {
        if !self.builder_expect().is_empty() {
            self.emit_and_replace_block();
        }

        let for_result_local = self.new_temp_local();
        let binding_local = self.new_temp_local();
        let (inclusive, max_expr_id) = {
            let Some(HirNode::Expr(iterable_expr)) = hlir.get_hir_node(iterable_id) else {
                push_lower_err!(self, hlir, iterable_id, "Failed to get iterable expr.");
                return;
            };
            let ExprKind::Range(min_expr, max_expr, inclusive) = &iterable_expr.kind else {
                push_lower_err!(self, hlir, iterable_id, "Iterable is not a range.");
                return;
            };

            let Some(binding_init_rhs_local) = self.visit_expr_assigned(*min_expr, hlir) else {
                push_lower_err!(
                    self,
                    hlir,
                    *min_expr,
                    "Failed to eval for loop binding initial value."
                );
                return;
            };

            self.builder_mut_expect()
                .push_assign(binding_local.into(), RValue::copy(binding_init_rhs_local));
            self.emit_and_replace_block();
            self.lut.insert(binding_id, binding_local);
            (*inclusive, *max_expr)
        };

        let loop_start_bb_id = self.body.next_block_id();
        self.state
            .loop_stack
            .push(HIRToMIRLoopState::new(loop_start_bb_id));

        let condition_expr = self.new_temp_local();
        let binary_op_kind = if inclusive {
            BinaryOpKind::LessThanOrEqual
        } else {
            BinaryOpKind::LessThan
        };
        let Some(max_expr) = self.visit_expr_assigned(max_expr_id, hlir) else {
            push_lower_err!(self, hlir, max_expr_id, "Failed to eval max expr!");
            return;
        };
        self.builder_mut_expect().push_local_assign(
            condition_expr,
            RValue::BinaryOp(
                binary_op_kind,
                (Operand::Copy(binding_local.into()), Operand::Copy(max_expr)),
            ),
        );

        self.builder_mut_expect().push_branch(
            Operand::Copy(condition_expr.into()),
            BasicBlockId::PLACEHOLDER_ID,
            BasicBlockId::PLACEHOLDER_ID,
        );
        let branch_block_id = self.emit_and_replace_block().unwrap();

        self.visit_block_by_id(block_id, hlir);
        self.builder_mut_expect().push_local_assign(
            binding_local,
            RValue::Increment(Operand::Copy(binding_local.into())),
        );
        self.builder_mut_expect().push_goto(loop_start_bb_id);
        let final_loop_body_id = self.emit_and_replace_block().unwrap();

        if !self.update_branch_targets(
            branch_block_id,
            branch_block_id.next(),
            final_loop_body_id.next(),
        ) {
            push_lower_err!(
                self,
                hlir,
                for_expr_id,
                "Failed to get for loop branch block."
            );
        }

        self.update_goto_targets_from_loop(hlir, for_expr_id, final_loop_body_id.next());
        self.state.loop_stack.pop();
        self.builder_mut_expect()
            .push_local_assign(for_result_local, RValue::unit());
        self.state.last_block_target = Some(for_result_local.into());
    }

    fn lower_while(&mut self, hlir: &HLIR, while_expr_id: HirId, condition: HirId, block: HirId) {
        if !self.builder_expect().is_empty() {
            self.emit_and_replace_block();
        }

        let Some(target) = self.visit_expr_assigned(condition, hlir) else {
            push_lower_err!(self, hlir, condition, "Failed to eval while condition.");
            return;
        };

        let while_result_local = self.new_temp_local();
        let condition_check_bb_id = self.body.next_block_id();
        self.state
            .loop_stack
            .push(HIRToMIRLoopState::new(condition_check_bb_id));

        self.builder_mut_expect().push_branch(
            Operand::Copy(target),
            BasicBlockId::PLACEHOLDER_ID,
            BasicBlockId::PLACEHOLDER_ID,
        );
        let branch_block_id = self.emit_and_replace_block().unwrap();
        self.visit_block_by_id(block, hlir);
        self.builder_mut_expect()
            .set_exit_kind(BlockExitKind::Goto(condition_check_bb_id));
        let final_loop_body_id = self.emit_and_replace_block().unwrap();

        self.update_branch_targets(
            branch_block_id,
            branch_block_id.next(),
            final_loop_body_id.next(),
        );

        self.update_goto_targets_from_loop(hlir, while_expr_id, final_loop_body_id.next());

        self.state.loop_stack.pop();
        self.builder_mut_expect()
            .push_local_assign(while_result_local, RValue::unit());
        self.state.last_block_target = Some(while_result_local.into());
    }

    fn lower_call(&mut self, hlir: &HLIR, expr_id: HirId, call_id: HirId, args: &[HirId]) {
        let Some(target) = self.visit_expr_expect_owner(call_id, hlir) else {
            push_lower_err!(self, hlir, call_id, "Failed to get call target.");
            return;
        };

        let Some(arg_local_ids) = args
            .iter()
            .map(|a_id| {
                let local_id = self.visit_expr_assigned(*a_id, hlir)?;
                Some(Operand::Copy(local_id))
            })
            .collect::<Option<Vec<_>>>()
        else {
            push_lower_err!(self, hlir, expr_id, "Failed to eval all args!");
            return;
        };

        if self.is_directive_set() {
            self.emit_and_replace_block()
                .expect("Emit block before call.");
        }
        let call_block_id = self.body.next_block_id();
        let call_result_slot = self.new_temp_local();
        self.builder_mut_expect().push_call(
            call_result_slot,
            target,
            arg_local_ids,
            call_block_id.next(),
        );
        assert!(
            self.emit_and_replace_block().expect("Emit call block.") == call_block_id,
            "Call block id does not match expected."
        );
        self.state.last_block_target = Some(call_result_slot.into());
    }

    fn lower_method_call(
        &mut self,
        hlir: &HLIR,
        expr_id: HirId,
        target_id: HirId,
        method_name: &str,
        args: &[HirId],
    ) {
        fn is_def_method_func(hlir: &HLIR, owner_id: OwnerDefId) -> bool {
            let Some(owning_node) = hlir.owning_node(owner_id) else {
                return false;
            };
            let Some(func) = owning_node.hir_function_ref() else {
                return false;
            };
            func.is_method
        }

        let Some(self_id) = self.visit_expr_assigned(target_id, hlir) else {
            push_lower_err!(self, hlir, target_id, "Failed to eval target method!");
            return;
        };
        let Some(obj_ty) = self.program_meta_data.type_map.get(&target_id) else {
            push_lower_err!(
                self,
                hlir,
                target_id,
                "Failed to get object type for method call `{:?}`",
                target_id
            );
            return;
        };

        let Some(method_def) = self
            .program_meta_data
            .find_ty_method_owner_def(obj_ty, method_name)
        else {
            push_lower_err!(
                self,
                hlir,
                target_id,
                "Failed to find method `{}` for type `{:?}`",
                method_name,
                self.program_meta_data.type_name(obj_ty.clone())
            );
            return;
        };

        if !is_def_method_func(hlir, method_def) {
            push_lower_err!(self, hlir, target_id, "Associated call is not a method!");
            return;
        }

        let self_arg = Operand::Copy(self_id);
        let Some(arg_local_ids) = std::iter::once(Some(self_arg))
            .chain(args.iter().map(|a_id| {
                let local_id = self.visit_expr_assigned(*a_id, hlir)?;
                Some(Operand::Copy(local_id))
            }))
            .collect::<Option<Vec<_>>>()
        else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Failed to eval all args of method call!"
            );
            return;
        };

        if self.is_directive_set() {
            self.emit_and_replace_block()
                .expect("Emit block before method call.");
        }
        let call_block_id = self.body.next_block_id();
        let call_result_slot = self.new_temp_local();
        self.builder_mut_expect().push_call(
            call_result_slot,
            method_def,
            arg_local_ids,
            call_block_id.next(),
        );
        assert!(
            self.emit_and_replace_block().expect("Emit call block.") == call_block_id,
            "Call block id does not match expected."
        );
        self.state.last_block_target = Some(call_result_slot.into());
    }

    fn lower_tuple_index(
        &mut self,
        hlir: &HLIR,
        expr_id: HirId,
        target_local: AssignTarget,
        tuple_tys: &[KitTy],
        index_ident: &str,
    ) {
        let index: usize = if let Ok(i) = index_ident.parse() {
            i
        } else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Failed to parse tuple index `{}`.",
                index_ident
            );
            return;
        };

        if index >= tuple_tys.len() {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Tuple index `{}` out of bounds (max `{}`).",
                index,
                tuple_tys.len().saturating_sub(1)
            );
            return;
        }

        let target_local_id = target_local.local_expect();
        let local = self.new_temp_local_with_mut(self.get_mutability_of_local(target_local_id));
        self.builder_mut_expect().push_local_assign(
            local,
            RValue::refer(AssignTarget::Field(target_local_id, index)),
        );
    }

    fn lower_field_access(
        &mut self,
        hlir: &HLIR,
        expr_id: HirId,
        target_id: HirId,
        field_name: &str,
    ) {
        let Some(target_local) = self.visit_expr_assigned(target_id, hlir) else {
            push_lower_err!(self, hlir, target_id, "Failed to eval target field local!");
            return;
        };

        let type_id = match self.program_meta_data.type_map.get(&target_id) {
            Some(Type::Resolved(KitTy::Abstract(type_id))) => type_id,
            Some(Type::Resolved(KitTy::Tuple(tys))) => {
                return self.lower_tuple_index(hlir, expr_id, target_local, tys, field_name);
            }
            _ => {
                push_lower_err!(self, hlir, target_id, "Target is not abstract.");
                return;
            }
        };

        let to_access = self
            .program_meta_data
            .type_registry
            .get_from_type_id(*type_id)
            .expect("Type exists.");

        let Some(field_index) = to_access.find_field_index(field_name) else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Failed to find field index for `{}`",
                field_name
            );
            return;
        };

        let target_local_id = target_local.local_expect();
        let local = self.new_temp_local_with_mut(self.get_mutability_of_local(target_local_id));
        self.builder_mut_expect().push_local_assign(
            local,
            RValue::refer(AssignTarget::Field(target_local_id, field_index)),
        );
    }

    fn lower_struct_init(
        &mut self,
        hlir: &HLIR,
        expr_id: HirId,
        struct_init: &StructInitialisation,
    ) {
        let RefPath::Resolved(_, resolved_id) = &struct_init.ty_path else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Failed to resolve struct init type `{:?}`",
                struct_init.ty_path
            );
            return;
        };
        let ResolvedID::TypeDef(type_id) = *resolved_id else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Resolved id is not a type id, but rather: `{:?}`",
                resolved_id
            );
            return;
        };

        let Some(type_info) = self
            .program_meta_data
            .type_registry
            .get_from_type_id(type_id)
        else {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Failed to get type info for struct initialisation."
            );
            return;
        };

        if type_info.get_field_count() != struct_init.fields.len() {
            push_lower_err!(
                self,
                hlir,
                expr_id,
                "Field count mismatch in initialisation! Expected `{}`, got `{}`",
                type_info.get_field_count(),
                struct_init.fields.len()
            );
            return;
        }

        let mut field_values = type_info
            .get_fields()
            .iter()
            .map(|_| Operand::Unit)
            .collect::<Vec<_>>();

        for field_init in &struct_init.fields {
            let Some(field_index) = type_info.find_field_index(field_init.ident.str()) else {
                push_lower_err!(
                    self,
                    hlir,
                    expr_id,
                    "Failed to find field index for `{}`",
                    field_init.ident.str()
                );
                return;
            };

            if let Some(field_init_local) = self.visit_expr_assigned(field_init.expr, hlir) {
                *field_values.get_mut(field_index).expect("Field index") =
                    Operand::Copy(field_init_local);
            } else {
                push_lower_err!(
                    self,
                    hlir,
                    field_init.expr,
                    "Failed to eval field initialisation expression."
                );
            }
        }

        let struct_local = self.new_temp_local();
        self.builder_mut_expect().push_local_assign(
            struct_local,
            RValue::ADT(super::ADTKind::Struct(type_id), field_values),
        );
    }

    fn lower_tuple_destructuring(&mut self, hlir: &HLIR, temp_local: LocalId, bind_ids: &[HirId]) {
        for (index, bind_id) in bind_ids.iter().enumerate() {
            let Some(binding) = hlir.binding_by_id(*bind_id) else {
                push_lower_err!(
                    self,
                    hlir,
                    *bind_id,
                    "Failed to get binding for tuple destructuring."
                );
                continue;
            };
            match &binding.kind {
                BindingKind::Ident(i) => {
                    let local_id = self.body.push_local(LocalDefinition {
                        mutable: binding.modifiers.mutable,
                        info: LocalInfo::UserDeclared(i.ident.clone()),
                    });
                    self.lut.insert(*bind_id, local_id);
                    self.builder_mut_expect().push_local_assign(
                        local_id,
                        RValue::refer(AssignTarget::Field(temp_local, index)),
                    );
                }
                BindingKind::Tuple(ids) => {
                    let destructure_local = self.new_temp_local_with_mut(binding.modifiers.mutable);
                    self.builder_mut_expect().push_local_assign(
                        destructure_local,
                        RValue::refer(AssignTarget::Field(temp_local, index)),
                    );
                    self.lower_tuple_destructuring(hlir, destructure_local, ids);
                }
            }
        }
    }
}

impl HLIRVisitor for HIRToMIRFuncLowerer<'_> {
    // This *has* to have too many lines..
    #[allow(clippy::too_many_lines)]
    fn visit_expr(&mut self, expr: &Expr, hlir: &HLIR) {
        match &expr.kind {
            ExprKind::Block(hir_id) => self.visit_block_by_id(*hir_id, hlir),
            ExprKind::Literal(literal) => {
                let local = self.new_temp_local();
                self.builder_mut_expect()
                    .push_local_assign(local, RValue::literal(literal.clone()));
            }
            ExprKind::BinaryOp(binary_op_kind, hir_id, hir_id1) => {
                let Some(lhs_local) = self.visit_expr_assigned(*hir_id, hlir) else {
                    push_lower_err!(self, hlir, *hir_id, "Failed to resolve LHS of binary op.");
                    return;
                };
                let Some(rhs_local) = self.visit_expr_assigned(*hir_id1, hlir) else {
                    push_lower_err!(self, hlir, *hir_id1, "Failed to resolve RHS of binary op.");
                    return;
                };

                let local = self.new_temp_local();
                self.builder_mut_expect().push_local_assign(
                    local,
                    RValue::BinaryOp(
                        *binary_op_kind,
                        (Operand::Copy(lhs_local), Operand::Copy(rhs_local)),
                    ),
                );
            }
            ExprKind::UnaryOp(unary_op_kind, hir_id) => {
                let Some(rhs_local) = self.visit_expr_assigned(*hir_id, hlir) else {
                    push_lower_err!(self, hlir, *hir_id, "Failed to resolve RHS of unary op.");
                    return;
                };
                let local = self.new_temp_local();
                self.builder_mut_expect().push_local_assign(
                    local,
                    RValue::UnaryOp(*unary_op_kind, Operand::Copy(rhs_local)),
                );
            }
            ExprKind::If(condition, true_block, else_expr) => {
                self.lower_if(hlir, expr.id, *condition, *true_block, else_expr.as_ref());
            }
            ExprKind::Loop(block) => {
                self.lower_loop(hlir, expr.id, *block);
            }
            ExprKind::For(_, bind_id, iter_id, loop_block_id) => {
                self.lower_for(hlir, expr.id, *bind_id, *iter_id, *loop_block_id);
            }
            ExprKind::While(loop_condition_id, block_id) => {
                self.lower_while(hlir, expr.id, *loop_condition_id, *block_id);
            }
            ExprKind::Assign(hir_id, hir_id1) => {
                let Some(target) = self.visit_expr_assigned(*hir_id, hlir) else {
                    push_lower_err!(self, hlir, *hir_id, "Failed to resolve assignment target.");
                    return;
                };

                if let Some(local) = self.body.local(target.local_id())
                    && !local.mutable.is_mutable()
                {
                    push_lower_err!(self, hlir, *hir_id, "Cannot assign to immutable variable!");
                    return;
                }

                let Some(rhs_local) = self.visit_expr_assigned(*hir_id1, hlir) else {
                    push_lower_err!(self, hlir, *hir_id1, "Failed to resolve assignment RHS.");
                    return;
                };

                self.builder_mut_expect()
                    .push_assign(target, RValue::copy(rhs_local));
            }
            ExprKind::Call(call_expr, args) => {
                self.lower_call(hlir, expr.id, *call_expr, args);
            }
            ExprKind::MethodCall(hir_id, ident, args) => {
                self.lower_method_call(hlir, expr.id, *hir_id, ident.str(), args);
            }
            // ExprKind::Index(hir_id, hir_id1) => {}
            ExprKind::FieldAccess(hir_id, ident) => {
                self.lower_field_access(hlir, expr.id, *hir_id, ident.str());
            }
            ExprKind::StructInit(struct_initialisation) => {
                self.lower_struct_init(hlir, expr.id, struct_initialisation);
            }
            ExprKind::Path(ref_path) => {
                if let Some(resolved) = ref_path.resolved_id() {
                    self.handle_resolved_id(resolved, hlir);
                } else {
                    push_lower_err!(
                        self,
                        hlir,
                        expr.id,
                        "Path not resolved `{:?}` (This should be impossible, if you see this, please report a bug!)",
                        ref_path
                    );
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
                    push_lower_err!(
                        self,
                        hlir,
                        expr.id,
                        "Cannot continue from outside of a loop!"
                    );
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
                    push_lower_err!(self, hlir, expr.id, "Cannot break from outside of a loop!");
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
                        RValue::copy(target)
                    } else {
                        push_lower_err!(
                            self,
                            hlir,
                            *return_expr_id,
                            "Failed to get return value expression"
                        );
                        RValue::unit()
                    }
                } else {
                    RValue::unit()
                };

                self.builder_mut_expect().push_return(return_value);
            }
            ExprKind::Cast(target, target_type) => {
                let Some(lhs_local) = self.visit_expr_assigned(*target, hlir) else {
                    push_lower_err!(
                        self,
                        hlir,
                        *target,
                        "Failed to get type of cast expression lhs"
                    );
                    return;
                };
                let Some(resolved_type) = target_type.resolved() else {
                    push_lower_err!(self, hlir, *target, "Failed to resolve target cast type");
                    return;
                };
                let Some(cast_kind) = CastKind::from_type(resolved_type) else {
                    push_lower_err!(self, hlir, *target, "Failed to get valid target cast type");
                    return;
                };
                let local = self.new_temp_local();
                self.builder_mut_expect()
                    .push_local_assign(local, RValue::Cast(Operand::Copy(lhs_local), cast_kind));
            }
            ExprKind::Tuple(t) => {
                let mut element_values = Vec::with_capacity(t.len());
                for element_expr_id in t {
                    if let Some(element_local) = self.visit_expr_assigned(*element_expr_id, hlir) {
                        element_values.push(Operand::Copy(element_local));
                    } else {
                        push_lower_err!(
                            self,
                            hlir,
                            *element_expr_id,
                            "Failed to eval tuple element expression."
                        );
                        return;
                    }
                }
                let tuple_local = self.new_temp_local();
                self.builder_mut_expect()
                    .push_local_assign(tuple_local, RValue::Tuple(element_values));
            }
            _ => {}
        }
    }

    fn visit_let_statement(&mut self, _id: HirId, let_statement: &LetStatement, hlir: &HLIR) {
        let Some(HirNode::Binding(binding)) = hlir.get_hir_node(let_statement.binding) else {
            push_lower_err!(
                self,
                hlir,
                let_statement.binding,
                "Let statement binding is not a binding!"
            );
            return;
        };

        match &binding.kind {
            BindingKind::Ident(i) => {
                let local_id = self.body.push_local(LocalDefinition {
                    mutable: binding.modifiers.mutable,
                    info: LocalInfo::UserDeclared(i.ident.clone()),
                });
                self.lut.insert(binding.id, local_id);

                if let Some(init_id) = &let_statement.initial_value {
                    if let Some(target) = self.visit_expr_assigned(*init_id, hlir) {
                        self.builder_mut_expect()
                            .push_local_assign(local_id, RValue::copy(target));
                    } else {
                        push_lower_err!(
                            self,
                            hlir,
                            *init_id,
                            "Failed to get let statement initial expression target"
                        );
                    }
                }
            }
            BindingKind::Tuple(ids) => {
                let temp_local_id = self.new_temp_local();

                let Some(init_id) = &let_statement.initial_value else {
                    return;
                };
                let Some(target) = self.visit_expr_assigned(*init_id, hlir) else {
                    push_lower_err!(
                        self,
                        hlir,
                        *init_id,
                        "Failed to get let statement initial expression target"
                    );
                    return;
                };

                self.builder_mut_expect()
                    .push_local_assign(temp_local_id, RValue::copy(target));

                self.lower_tuple_destructuring(hlir, temp_local_id, ids);
            }
        }
    }

    fn visit_statement(&mut self, statement: &Statement, parent_block: &Block, hlir: &HLIR) {
        match &statement.kind {
            StatementKind::Expr(hir_id) => {
                self.process_statement_expr(statement, hlir, *hir_id);

                if is_non_expr_expr(hlir, *hir_id) {
                    self.state.read_last_block_target();
                    return;
                }

                // Do as a return.
                if parent_block.id == self.func_body_id {
                    let last_local = self.last_target();
                    self.builder_mut_expect()
                        .push_return(RValue::copy(last_local));
                } else {
                    self.state.last_block_target = Some(self.last_target());
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

    fn visit_function_param(&mut self, parameter: &Parameter, hlir: &HLIR) {
        let Some(binding) = hlir.binding_by_id(parameter.binding) else {
            push_lower_err!(
                self,
                hlir,
                parameter.binding,
                "Failed to get binding for function parameter."
            );
            return;
        };

        match &binding.kind {
            BindingKind::Ident(i) => {
                let param_local_id = self
                    .body
                    .push_param(binding.modifiers.mutable, i.ident.clone());
                self.lut.insert(binding.id, param_local_id);
            }
            BindingKind::Tuple(..) => {
                let param_local_id = self.body.push_param_for_binding(binding.modifiers.mutable);
                self.lut.insert(binding.id, param_local_id);
                self.param_bindings.push((binding.id, param_local_id));
            }
        }
    }

    fn visit_function(&mut self, function: &Function, hlir: &HLIR) {
        if function.owner_id == self.func_owner_id {
            self.state.parse_assign_target = true;
            self.create_block_builder();

            if let Some(func_body) = &function.body {
                for param_id in &func_body.params {
                    if let Some(HirNode::Param(param)) = hlir.get_hir_node(*param_id) {
                        self.visit_function_param(param, hlir);
                    }
                }

                for (binding_id, param_local) in std::mem::take(&mut self.param_bindings) {
                    let Some(binding) = hlir.binding_by_id(binding_id) else {
                        push_lower_err!(
                            self,
                            hlir,
                            binding_id,
                            "Failed to get binding for function parameter binding."
                        );
                        continue;
                    };
                    if let BindingKind::Tuple(ids) = &binding.kind {
                        self.lower_tuple_destructuring(hlir, param_local, ids);
                    }
                }

                if let Some(HirNode::Block(func_block)) = hlir.get_hir_node(func_body.block) {
                    self.visit_block(func_block, hlir);
                }
            }

            self.emit_final_block();
        }
    }
}

pub fn is_non_expr_expr(hlir: &HLIR, expr_id: HirId) -> bool {
    let Some(HirNode::Expr(e)) = hlir.get_hir_node(expr_id) else {
        return false;
    };
    matches!(
        e.kind,
        ExprKind::Loop(..) | ExprKind::While(..) | ExprKind::For(..)
    )
}

pub fn lower_hir_to_mir(hlir: &HLIR, type_info: &ProgramMetaData) -> LowerResult<MIR> {
    let mut bodies = HashMap::<OwnerDefId, Body>::new();
    let mut native_function_links = HashMap::<OwnerDefId, String>::new();

    for i in hlir.owner_id_iter() {
        if let Some(node) = hlir.owning_node(i)
            && let Some(func) = node.hir_function_ref()
        {
            if func.native {
                native_function_links.insert(i, func.ident.string());
            } else {
                let body = HIRToMIRFuncLowerer::from_func_id(hlir, type_info, i)
                    .map_err(|e| LoweringErrorKind::LoweringErrors(e).with_no_span())?;
                bodies.insert(i, body);
            }
        }
    }

    Ok(MIR {
        bodies,
        native_function_links,
    })
}
