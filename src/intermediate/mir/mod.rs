use ::std::fmt::Debug;
use std::collections::HashMap;

use crate::ast::{BinaryOpKind, Ident, Literal, Mutability, UnaryOpKind};

use crate::intermediate::hir::OwnerDefId;
use crate::intermediate::resolver::TypeID;
use crate::intermediate::types::{KitFloat, KitInt, KitTy, KitUInt};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BasicBlockId(pub u32);

impl BasicBlockId {
    pub const PLACEHOLDER_ID: Self = Self(u32::MAX);
    pub const ENTRY_BLOCK: Self = Self(0);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Debug for BasicBlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BasicBlockId({})", self.0)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LocalId(pub u32);

impl LocalId {
    pub const PLACEHOLDER_ID: LocalId = LocalId(u32::MAX);
    pub const RETURN_VALUE: LocalId = LocalId(0);

    pub fn is_placeholder(self) -> bool {
        self == Self::PLACEHOLDER_ID
    }

    pub fn is_return_value(self) -> bool {
        self == Self::RETURN_VALUE
    }

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Debug for LocalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LocalId({})", self.0)
    }
}

#[derive(Clone)]
pub enum LocalInfo {
    UserDeclared(Ident), /*(SourceSpan)*/
    Temp,
}

impl Debug for LocalInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserDeclared(arg0) => write!(f, "User({})", arg0.str()),
            Self::Temp => write!(f, "Temp"),
        }
    }
}

#[derive(Clone)]
pub struct LocalDefinition {
    pub mutable: Mutability,
    pub info: LocalInfo,
}

impl Debug for LocalDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LocalDefinition{{ {:?}, {:?} }}",
            self.info, self.mutable
        )
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum Operand {
    Copy(AssignTarget),
    Unit,
    Literal(Literal),
    Const, // Not sure yet
}

impl Debug for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Copy(arg0) => write!(f, "Copy({:?})", arg0),
            Self::Unit => write!(f, "Unit"),
            Self::Literal(arg0) => arg0.fmt(f),
            Self::Const => write!(f, "Const"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CastKind {
    Int(KitInt),
    UInt(KitUInt),
    Float(KitFloat),
}

impl CastKind {
    pub fn from_type(ty: KitTy) -> Option<Self> {
        match ty {
            KitTy::Int(int_kind) => Some(CastKind::Int(int_kind)),
            KitTy::UInt(uint_kind) => Some(CastKind::UInt(uint_kind)),
            KitTy::Float(float_kind) => Some(CastKind::Float(float_kind)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ADTKind {
    Struct(TypeID),
}

#[derive(Clone)]
pub enum RValue {
    Unchanged(Operand),
    Ref(AssignTarget),
    BinaryOp(BinaryOpKind, (Operand, Operand)),
    UnaryOp(UnaryOpKind, Operand),
    ADT(ADTKind, Vec<Operand>),
    Cast(Operand, CastKind),
}

impl Debug for RValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unchanged(arg0) => arg0.fmt(f),
            Self::Ref(arg0) => f.debug_tuple("Ref").field(arg0).finish(),
            Self::BinaryOp(arg0, arg1) => {
                write!(f, "BinaryOp({:?}, ({:?}, {:?}))", arg0, arg1.0, arg1.1)
            }
            Self::UnaryOp(arg0, arg1) => write!(f, "UnaryOp({:?}, {:?})", arg0, arg1),
            Self::ADT(kind, operands) => {
                write!(f, "ADT({:?}, ", kind)?;
                for (i, operand) in operands.iter().enumerate() {
                    if i == 0 {
                        write!(f, "{:?}", operand)?;
                    } else {
                        write!(f, ", {:?}", operand)?;
                    }
                }
                write!(f, ")")
            }
            Self::Cast(operand, target_type) => {
                write!(f, "Cast({:?} as {:?})", operand, target_type)
            }
        }
    }
}

impl RValue {
    pub fn unit() -> Self {
        Self::Unchanged(Operand::Unit)
    }

    pub fn literal(literal: Literal) -> Self {
        Self::Unchanged(Operand::Literal(literal))
    }

    pub fn copy(assign_target: AssignTarget) -> Self {
        Self::Unchanged(Operand::Copy(assign_target))
    }

    pub fn refer(assign_target: AssignTarget) -> Self {
        Self::Ref(assign_target)
    }

    pub fn cast(operand: Operand, target_type: CastKind) -> Self {
        Self::Cast(operand, target_type)
    }
}

/// Defines a `slot` that is the target for a value to be assigned or copied into.
/// This can either be a [`LocalId`] or a `Field` defined in a [`LocalId`].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AssignTarget {
    Local(LocalId),
    Field(LocalId, usize /*FieldIndex*/),
}

impl AssignTarget {
    pub fn from_local(id: LocalId) -> Self {
        Self::Local(id)
    }

    pub fn from_field_access(id: LocalId, field_index: usize) -> Self {
        Self::Field(id, field_index)
    }

    /// Returns either the `LocalId` if `self` is of the `Local` variant, or the `LocalId` of the
    /// object being accessed if `self` is of the `Field` variant.
    pub fn local_id(&self) -> LocalId {
        match self {
            Self::Local(local_id) | Self::Field(local_id, _) => *local_id,
        }
    }

    /// Only returns the [`LocalId`] if the [`AssignTarget`] is of the `Local` variant.
    /// Returns `None` otherwise.
    pub fn local(&self) -> Option<LocalId> {
        match self {
            AssignTarget::Local(local_id) => Some(*local_id),
            _ => None,
        }
    }

    /// Only returns the [`LocalId`] if the [`AssignTarget`] is of the `Local` variant.
    /// Panics otherwise.
    pub fn local_expect(&self) -> LocalId {
        self.local().expect("Assign target should be local!")
    }

    /// Only returns the [`LocalId`] if the [`AssignTarget`] is of the `Field` assignment variant.
    /// Returns `None` otherwise.
    pub fn field_access(&self) -> Option<(LocalId, usize)> {
        match self {
            AssignTarget::Field(local_id, field_index) => Some((*local_id, *field_index)),
            _ => None,
        }
    }

    /// Only returns the [`LocalId`] if the [`AssignTarget`] is of the `Field` assignment variant.
    /// Panics otherwise.
    pub fn field_access_expect(&self) -> (LocalId, usize) {
        self.field_access()
            .expect("Assign target should be field access!")
    }
}

impl From<LocalId> for AssignTarget {
    fn from(value: LocalId) -> Self {
        Self::Local(value)
    }
}

#[derive(Clone)]
pub enum StatementKind {
    Assign(AssignTarget, RValue),
}

impl Debug for StatementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assign(arg0, arg1) => write!(f, "Assign{{ {:?} = {:?} }}", arg0, arg1),
        }
    }
}

#[derive(Clone)]
pub struct Statement {
    pub kind: StatementKind,
}

impl Debug for Statement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum BlockExitKind {
    Goto(BasicBlockId),
    Branch(Operand, BasicBlockId, BasicBlockId),
    Return,
    Call(LocalId, OwnerDefId, Vec<Operand>, BasicBlockId),
}

impl Debug for BlockExitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Goto(arg0) => write!(f, "Goto({:?})", arg0),
            Self::Branch(arg0, arg1, arg2) => f
                .debug_tuple("Branch")
                .field(arg0)
                .field(arg1)
                .field(arg2)
                .finish(),
            Self::Return => write!(f, "Return"),
            Self::Call(assign_to, def_id, args, next_block) => write!(
                f,
                "Call({:?}, {:?} -> {:?}) = {:?}",
                def_id, args, next_block, assign_to
            ),
        }
    }
}

#[derive(Clone)]
pub struct ExitDirective {
    pub kind: BlockExitKind,
}

impl Debug for ExitDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExitDirective::{:?}", self.kind)
    }
}

impl ExitDirective {
    pub fn from_kind(kind: BlockExitKind) -> Self {
        Self { kind }
    }
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub exit_directive: ExitDirective,
}

#[derive(Debug, Clone)]
pub struct Body {
    // locals[0] is _ALWAYS_ the return value.
    pub locals: Vec<LocalDefinition>,
    pub blocks: Vec<BasicBlock>,
    pub arg_count: u32,
}

impl Body {
    fn get_return_slot_def() -> LocalDefinition {
        LocalDefinition {
            mutable: Mutability::Mutable,
            info: LocalInfo::Temp,
        }
    }

    pub fn new(params: &[LocalDefinition]) -> Self {
        let mut locals = vec![Self::get_return_slot_def()];
        locals.extend_from_slice(params);
        Self {
            locals,
            blocks: Vec::new(),
            arg_count: params.len() as u32,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            locals: vec![Self::get_return_slot_def()],
            blocks: Vec::new(),
            arg_count: 0,
        }
    }
}

impl Body {
    pub fn local(&self, id: LocalId) -> Option<&LocalDefinition> {
        self.locals.get(id.0 as usize)
    }

    pub fn local_mut(&mut self, id: LocalId) -> Option<&mut LocalDefinition> {
        self.locals.get_mut(id.0 as usize)
    }

    pub fn block(&self, id: BasicBlockId) -> Option<&BasicBlock> {
        self.blocks.get(id.0 as usize)
    }

    pub fn block_mut(&mut self, id: BasicBlockId) -> Option<&mut BasicBlock> {
        self.blocks.get_mut(id.0 as usize)
    }

    pub fn block_exit_kind(&self, id: BasicBlockId) -> Option<&BlockExitKind> {
        Some(&self.block(id)?.exit_directive.kind)
    }

    pub fn block_exit_kind_mut(&mut self, id: BasicBlockId) -> Option<&mut BlockExitKind> {
        Some(&mut self.block_mut(id)?.exit_directive.kind)
    }

    pub fn next_block_id(&mut self) -> BasicBlockId {
        BasicBlockId(self.blocks.len() as u32)
    }

    pub fn next_local_id(&mut self) -> LocalId {
        LocalId(self.locals.len() as u32)
    }
}

impl Body {
    pub fn push_param(&mut self, mutable: Mutability, ident: Ident) -> LocalId {
        let def = LocalDefinition {
            mutable,
            info: LocalInfo::UserDeclared(ident),
        };
        self.arg_count += 1;
        self.push_local(def)
    }

    pub fn push_local(&mut self, def: LocalDefinition) -> LocalId {
        let id = self.locals.len() as u32;
        self.locals.push(def);
        LocalId(id)
    }

    pub fn push_block(&mut self, block: BasicBlock) -> BasicBlockId {
        let id = self.blocks.len() as u32;
        self.blocks.push(block);
        BasicBlockId(id)
    }
}

#[derive(Debug, Clone)]
pub struct MIR {
    pub bodies: HashMap<OwnerDefId, Body>,
    pub native_function_links: HashMap<OwnerDefId, String>,
}

mod lowerer;

pub fn lower_hir_to_mir(
    hlir: &crate::intermediate::hir::HLIR,
    meta_data: &crate::intermediate::hir::ProgramMetaData,
) -> crate::intermediate::hir::errors::LowerResult<MIR> {
    lowerer::lower_hir_to_mir(hlir, meta_data)
}
