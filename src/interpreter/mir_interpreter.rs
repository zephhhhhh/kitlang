use std::time::Duration;
use std::{cell::RefCell, collections::HashMap};

use crate::ast::{BinaryOpKind, Literal, UnaryOpKind};

use crate::intermediate::hir::OwnerDefId;
use crate::intermediate::mir::{
    AssignTarget, BasicBlockId, BlockExitKind, Body, CastKind, LocalId, MIR, Operand, RValue,
    Statement, StatementKind,
};

use crate::intermediate::resolver::{NamespaceKind, RootNamespace, TypeRegistry};
use crate::intermediate::types::{KitFloat, KitInt, KitUInt};
use crate::interpreter::errors::{InterpResult, interp_err};
use crate::interpreter::native_functions::{IntoMIRKitlangFn, KitlangMIRNativeFn};

#[allow(unused_imports)]
use itertools::Itertools;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use log::{debug, error, warn};

#[derive(Debug, Clone)]
pub struct Program {
    pub namespace: RootNamespace,
    pub registry: TypeRegistry,
    pub mir: MIR,
}

impl Program {
    #[inline]
    #[must_use]
    pub const fn new(mir: MIR, registry: TypeRegistry, namespace: RootNamespace) -> Self {
        Self {
            namespace,
            registry,
            mir,
        }
    }
}

pub type ProgramType = Program;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ADTValueKind {
    Struct(Vec<Value>),
}

impl std::fmt::Display for ADTValueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Struct(values) => write!(f, "{values:?}"),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Value {
    #[default]
    Unit,
    Integer(i64),
    UnsignedInteger(u64),
    Float(f64),
    String(String),
    Boolean(bool),
    Ref(ContextTarget),
    ADT(ADTValueKind),
    Tuple(Vec<Value>),
    Array(Vec<Value>),
}

impl Value {
    #[inline]
    #[must_use]
    pub fn from_literal(lit: Literal) -> Self {
        match lit {
            Literal::String(s) => Self::String(s),
            Literal::Float(f) => Self::Float(f),
            Literal::Integer(i) => Self::Integer(i),
            Literal::Boolean(b) => Self::Boolean(b),
        }
    }

    #[inline]
    #[must_use]
    pub fn repr_string(&self) -> String {
        match self {
            Self::Unit => "()".to_string(),
            Self::Integer(i) => i.to_string(),
            Self::UnsignedInteger(u) => u.to_string(),
            Self::Float(f) => f.to_string(),
            Self::String(s) => s.clone(),
            Self::Boolean(b) => b.to_string(),
            Self::Ref(context_target) => format!("&({context_target:?})"),
            Self::ADT(kind) => kind.to_string(),
            Self::Tuple(vals) => {
                let elements: Vec<String> = vals.iter().map(Value::repr_string).collect();
                format!("({})", elements.join(", "))
            }
            Self::Array(vals) => {
                let elements: Vec<String> = vals.iter().map(Value::repr_string).collect();
                format!("[{}]", elements.join(", "))
            }
        }
    }

    #[inline]
    #[must_use]
    pub const fn int(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    pub const fn int_coerce(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            Self::UnsignedInteger(i) => Some(*i as i64),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn uint(&self) -> Option<u64> {
        match self {
            Self::UnsignedInteger(i) => Some(*i),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    pub const fn uint_coerce(&self) -> Option<u64> {
        match self {
            Self::UnsignedInteger(i) => Some(*i),
            Self::Integer(i) => Some(*i as u64),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn float(&self) -> Option<f64> {
        match self {
            Self::Float(i) => Some(*i),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub fn string(&self) -> Option<String> {
        match self {
            Self::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub fn str_ref(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn are_matching_types(&self, other: &Self) -> bool {
        match self {
            Self::Unit => matches!(other, Self::Unit),
            Self::Integer(_) => matches!(other, Self::Integer(_)),
            Self::Float(_) => matches!(other, Self::Float(_)),
            Self::String(_) => matches!(other, Self::String(_)),
            Self::Boolean(_) => matches!(other, Self::Boolean(_)),
            Self::ADT(_) => other.is_adt(),
            Self::Tuple(_) => other.is_tuple(),
            Self::Array(_) => other.is_array(),
            _ => false,
        }
    }

    #[must_use]
    pub fn perform_unary_op(&self, op: UnaryOpKind) -> Option<Self> {
        match self {
            Self::Unit => None,
            Self::Integer(i) => match op {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not => Some(Self::Integer(!i)),
                UnaryOpKind::Negate => Some(Self::Integer(-i)),
            },
            Self::UnsignedInteger(i) => match op {
                UnaryOpKind::Not => Some(Self::UnsignedInteger(!i)),
                UnaryOpKind::Dereference | UnaryOpKind::Negate => None,
            },
            Self::Float(f) => match op {
                UnaryOpKind::Dereference | UnaryOpKind::Not => None,
                UnaryOpKind::Negate => Some(Self::Float(-f)),
            },
            Self::String(_s) => match op {
                UnaryOpKind::Dereference | UnaryOpKind::Not | UnaryOpKind::Negate => None,
            },
            Self::Boolean(b) => match op {
                UnaryOpKind::Not => Some(Self::Boolean(!b)),
                UnaryOpKind::Dereference | UnaryOpKind::Negate => None,
            },
            Self::Ref(..) => todo!(),
            Self::ADT(_) => todo!(),
            Self::Tuple(_) => todo!(),
            Self::Array(_) => todo!(),
        }
    }

    /// Perform a binary operation between this value and another value.
    /// # Panics
    /// Panics if the values are either `Ref` or `ADT` variants as they are not yet implemented.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn perform_binary_op(&self, rhs: &Self, op: BinaryOpKind) -> Option<Self> {
        const fn perform_int_op(lhs: i64, rhs: i64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::Integer(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::Integer(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::Integer(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::Integer(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::Integer(lhs % rhs)),
                BinaryOpKind::BitwiseXOR => Some(Value::Integer(lhs ^ rhs)),
                BinaryOpKind::BitwiseAND => Some(Value::Integer(lhs & rhs)),
                BinaryOpKind::BitwiseOR => Some(Value::Integer(lhs | rhs)),
                BinaryOpKind::ShiftLeft => Some(Value::Integer(lhs << rhs)),
                BinaryOpKind::ShiftRight => Some(Value::Integer(lhs >> rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        const fn perform_uint_op(lhs: u64, rhs: u64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::UnsignedInteger(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::UnsignedInteger(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::UnsignedInteger(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::UnsignedInteger(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::UnsignedInteger(lhs % rhs)),
                BinaryOpKind::BitwiseXOR => Some(Value::UnsignedInteger(lhs ^ rhs)),
                BinaryOpKind::BitwiseAND => Some(Value::UnsignedInteger(lhs & rhs)),
                BinaryOpKind::BitwiseOR => Some(Value::UnsignedInteger(lhs | rhs)),
                BinaryOpKind::ShiftLeft => Some(Value::UnsignedInteger(lhs << rhs)),
                BinaryOpKind::ShiftRight => Some(Value::UnsignedInteger(lhs >> rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        #[allow(clippy::float_cmp)]
        fn perform_float_op(lhs: f64, rhs: f64, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::Float(lhs + rhs)),
                BinaryOpKind::Sub => Some(Value::Float(lhs - rhs)),
                BinaryOpKind::Mul => Some(Value::Float(lhs * rhs)),
                BinaryOpKind::Div => Some(Value::Float(lhs / rhs)),
                BinaryOpKind::Mod => Some(Value::Float(lhs % rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        fn perform_string_op(lhs: &str, rhs: &str, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::Add => Some(Value::String(lhs.to_string() + rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(lhs < rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs > rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        const fn perform_bool_op(lhs: bool, rhs: bool, op: BinaryOpKind) -> Option<Value> {
            match op {
                BinaryOpKind::And => Some(Value::Boolean(lhs && rhs)),
                BinaryOpKind::Or => Some(Value::Boolean(lhs || rhs)),
                BinaryOpKind::BitwiseAND => Some(Value::Boolean(lhs & rhs)),
                BinaryOpKind::BitwiseOR => Some(Value::Boolean(lhs | rhs)),
                BinaryOpKind::BitwiseXOR => Some(Value::Boolean(lhs ^ rhs)),
                BinaryOpKind::Equal => Some(Value::Boolean(lhs == rhs)),
                BinaryOpKind::NotEqual => Some(Value::Boolean(lhs != rhs)),
                BinaryOpKind::LessThan => Some(Value::Boolean(!lhs && rhs)),
                BinaryOpKind::GreaterThan => Some(Value::Boolean(lhs && !rhs)),
                BinaryOpKind::LessThanOrEqual => Some(Value::Boolean(lhs <= rhs)),
                BinaryOpKind::GreaterThanOrEqual => Some(Value::Boolean(lhs >= rhs)),
                _ => None,
            }
        }

        if !self.are_matching_types(rhs) {
            return match self {
                Self::Integer(i) => perform_int_op(*i, rhs.int_coerce()?, op),
                Self::UnsignedInteger(u) => perform_uint_op(*u, rhs.uint_coerce()?, op),
                _ => None,
            };
        }

        // TODO: Handle unsigned/signed properly.
        match self {
            Self::Unit => None,
            Self::Integer(i) => perform_int_op(*i, rhs.int()?, op),
            Self::UnsignedInteger(u) => perform_uint_op(*u, rhs.uint()?, op),
            Self::Float(f) => perform_float_op(*f, rhs.float()?, op),
            Self::String(s) => perform_string_op(s, rhs.str_ref()?, op),
            Self::Boolean(b) => perform_bool_op(*b, rhs.bool()?, op),
            Self::Ref(..) => panic!("Cannot perform binary op on reference values!"),
            Self::ADT(_) => panic!("Cannot perform binary op on ADT values!"),
            Self::Tuple(_) => panic!("Cannot perform binary op on Tuple values!"),
            Self::Array(_) => panic!("Cannot perform binary op on Array values!"),
        }
    }

    #[inline]
    #[must_use]
    pub const fn perform_increment(&self) -> Option<Self> {
        match self {
            Self::Integer(i) => Some(Self::Integer(i.saturating_add(1))),
            Self::UnsignedInteger(u) => Some(Self::UnsignedInteger(u.saturating_add(1))),
            _ => None,
        }
    }

    /// Converts the value to a `usize` index if possible.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub const fn as_index_usize(&self) -> Option<usize> {
        match self {
            Self::Integer(i) if *i >= 0 => Some(*i as usize),
            Self::UnsignedInteger(u) => Some(*u as usize),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub fn index(&self, index: usize) -> Option<&Self> {
        match self {
            Self::ADT(adtvalue_kind) => match adtvalue_kind {
                ADTValueKind::Struct(values) => values.get(index),
            },
            Self::Tuple(vals) => vals.get(index),
            Self::Array(elems) => elems.get(index),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub fn index_mut(&mut self, index: usize) -> Option<&mut Self> {
        match self {
            Self::ADT(adtvalue_kind) => match adtvalue_kind {
                ADTValueKind::Struct(values) => values.get_mut(index),
            },
            Self::Tuple(vals) => vals.get_mut(index),
            Self::Array(elems) => elems.get_mut(index),
            _ => None,
        }
    }
}

impl Value {
    #[inline]
    #[must_use]
    pub const fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    #[inline]
    #[must_use]
    pub const fn is_reference(&self) -> bool {
        matches!(self, Self::Ref(..))
    }

    #[inline]
    #[must_use]
    pub const fn is_adt(&self) -> bool {
        matches!(self, Self::ADT(_))
    }

    #[inline]
    #[must_use]
    pub const fn is_tuple(&self) -> bool {
        matches!(self, Self::Tuple(_))
    }

    #[inline]
    #[must_use]
    pub const fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }
}

impl From<Literal> for Value {
    fn from(value: Literal) -> Self {
        Self::from_literal(value)
    }
}

impl From<&Literal> for Value {
    fn from(value: &Literal) -> Self {
        Self::from_literal(value.clone())
    }
}

impl From<isize> for Value {
    fn from(value: isize) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self::UnsignedInteger(value as u64)
    }
}

macro_rules! impl_value_ty {
    ($variant: ident, $int_ty: ty) => {
        impl From<$int_ty> for Value {
            fn from(value: $int_ty) -> Self {
                Self::$variant(value.into())
            }
        }
    };
    ($variant: ident, $int_ty: ty, $($rest:ty),+) => {
        impl_value_ty!($variant, $int_ty);
        impl_value_ty!($variant, $($rest),*);
    };
}

impl_value_ty!(Integer, i64, i32, i16, i8);
impl_value_ty!(UnsignedInteger, u64, u32, u16, u8);
impl_value_ty!(Boolean, bool);
impl_value_ty!(Float, f64, f32);

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<()> for Value {
    fn from((): ()) -> Self {
        Self::Unit
    }
}

// Implement conversions from a tuple to a Value::Tuple.. (upto 8 elements)
macro_rules! impl_tuple_from {
    ($($t:ident : $idx:tt),+) => {
        impl<$($t),+> From<($($t,)+)> for Value
        where
            $($t: Into<Value>,)+
        {
            fn from(value: ($($t,)+)) -> Self {
                Self::Tuple(vec![$(value.$idx.into(),)+])
            }
        }
    };
}

impl_tuple_from!(T1: 0);
impl_tuple_from!(T1: 0, T2: 1);
impl_tuple_from!(T1: 0, T2: 1, T3: 2);
impl_tuple_from!(T1: 0, T2: 1, T3: 2, T4: 3);
impl_tuple_from!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4);
impl_tuple_from!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5);
impl_tuple_from!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5, T7: 6);
impl_tuple_from!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5, T7: 6, T8: 7);

#[derive(Debug, Clone)]
pub struct StackContext {
    pub local_start: u32,
    pub local_count: u32,
}

impl StackContext {
    #[must_use]
    pub fn local_index(&self, local_id: LocalId) -> usize {
        (self.local_start as usize).saturating_add(local_id.0 as usize)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExecutionContext {
    pub value_stack: Vec<Value>,
    pub stack_ctxs: Vec<StackContext>,
}

impl ExecutionContext {
    #[inline]
    pub fn push_stack(&mut self, local_count: usize) {
        #[allow(clippy::cast_possible_truncation)]
        self.stack_ctxs.push(StackContext {
            local_start: self.value_stack.len() as u32,
            local_count: local_count as u32,
        });
        self.value_stack
            .extend_from_slice(&vec![Value::Unit; local_count]);
    }

    #[inline]
    pub fn pop_stack(&mut self) {
        if let Some(ctx) = self.stack_ctxs.pop() {
            self.value_stack.truncate(ctx.local_start as usize);
        }
    }

    #[must_use]
    pub fn current_ctx(&self) -> Option<&StackContext> {
        self.stack_ctxs.last()
    }

    #[allow(clippy::missing_panics_doc)]
    #[must_use]
    pub fn current_ctx_expect(&self) -> &StackContext {
        self.current_ctx().expect("No current stack context?")
    }
}

impl ExecutionContext {
    #[must_use]
    pub fn local_index(&self, local_id: LocalId) -> usize {
        self.current_ctx_expect().local_index(local_id)
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn push_arguments(&mut self, args: &[Value]) {
        let current_context = self.current_ctx_expect().clone();
        assert!(
            args.len().saturating_add(1) <= current_context.local_count as usize,
            "Not enough space in locals for function arguments!"
        );
        let start_index = current_context.local_start as usize;
        for (i, arg) in args.iter().enumerate() {
            self.value_stack[start_index + i + 1] = arg.clone();
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn get(&self, index: usize) -> InterpResult<&Value> {
        self.value_stack
            .get(index)
            .ok_or_else(|| interp_err!("Cannot get value from stack with index `{index}`."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn get_mut(&mut self, index: usize) -> InterpResult<&mut Value> {
        self.value_stack
            .get_mut(index)
            .ok_or_else(|| interp_err!("Cannot get mutable value from stack with index `{index}`."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn get_field(&self, index: usize, field_index: usize) -> InterpResult<&Value> {
        self.get(index)?.index(field_index).ok_or_else(|| {
            interp_err!(
                "Cannot get field index `{field_index}` from value at stack index `{index}`."
            )
        })
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn get_field_mut(&mut self, index: usize, field_index: usize) -> InterpResult<&mut Value> {
        self.get_mut(index)?.index_mut(field_index).ok_or_else(|| {
            interp_err!(
                "Cannot get field index `{field_index}` from value at stack index `{index}`."
            )
        })
    }
}

// Access..
impl ExecutionContext {
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn field_access(&self, id: LocalId, field_index: usize) -> InterpResult<&Value> {
        self.ctx_value(self.perform_deref_target(AssignTarget::Local(id))?)?
            .index(field_index)
            .ok_or_else(|| interp_err!("Field index `{field_index}` out of bounds."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn field_access_mut(
        &mut self,
        id: LocalId,
        field_index: usize,
    ) -> InterpResult<&mut Value> {
        self.ctx_value_mut(self.perform_deref_target(AssignTarget::Local(id))?)?
            .index_mut(field_index)
            .ok_or_else(|| interp_err!("Field index `{field_index}` out of bounds."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn index_value(&self, idx_id: LocalId) -> InterpResult<usize> {
        let ctx_target = self.perform_deref_target(AssignTarget::Local(idx_id))?;
        let index_value = self.ctx_value(ctx_target)?;
        index_value
            .as_index_usize()
            .ok_or_else(|| interp_err!("Cannot convert `{index_value:#?}` to usize index."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn index(&self, id: LocalId, idx_id: LocalId) -> InterpResult<&Value> {
        let ctx_target = self.perform_deref_target(AssignTarget::Local(idx_id))?;
        let index_value = self.ctx_value(ctx_target)?;
        let index = index_value
            .as_index_usize()
            .ok_or_else(|| interp_err!("Cannot convert `{index_value:#?}` to usize index."))?;
        let tar = self.perform_deref_target(AssignTarget::Local(id))?;
        self.ctx_value(tar)?
            .index(index)
            .ok_or_else(|| interp_err!("Index `{index}` out of bounds."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn index_mut(&mut self, id: LocalId, idx_id: LocalId) -> InterpResult<&mut Value> {
        let ctx_target = self.perform_deref_target(AssignTarget::Local(idx_id))?;
        let index_value = self.ctx_value(ctx_target)?;
        let index = index_value
            .as_index_usize()
            .ok_or_else(|| interp_err!("Cannot convert `{index_value:#?}` to usize index."))?;
        let tar = self.perform_deref_target(AssignTarget::Local(id))?;
        self.ctx_value_mut(tar)?
            .index_mut(index)
            .ok_or_else(|| interp_err!("Index `{index}` out of bounds."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn local(&self, id: LocalId) -> InterpResult<&Value> {
        self.get(self.local_index(id))
            .map_err(|_| interp_err!("Local `{id:?}` does not exist."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn local_mut(&mut self, id: LocalId) -> InterpResult<&mut Value> {
        self.get_mut(self.local_index(id))
            .map_err(|_| interp_err!("Local `{id:?}` does not exist (mut)."))
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn ctx_value(&self, at: ContextTarget) -> InterpResult<&Value> {
        match at {
            ContextTarget::StackIndex(idx) => self.get(idx),
            ContextTarget::Field(stack_idx, field_index) => self.get_field(stack_idx, field_index),
            ContextTarget::Index(stack_idx, index) => self.get_field(stack_idx, index),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn ctx_value_mut(&mut self, at: ContextTarget) -> InterpResult<&mut Value> {
        match at {
            ContextTarget::StackIndex(idx) => self.get_mut(idx),
            ContextTarget::Field(stack_idx, field_index) => {
                self.get_field_mut(stack_idx, field_index)
            }
            ContextTarget::Index(stack_idx, index) => self.get_field_mut(stack_idx, index),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn value(&self, at: AssignTarget) -> InterpResult<&Value> {
        let target = self.assign_target_to_context_target(at)?;
        self.ctx_value(target)
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn value_mut(&mut self, at: AssignTarget) -> InterpResult<&mut Value> {
        let target = self.assign_target_to_context_target(at)?;
        self.ctx_value_mut(target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContextTarget {
    StackIndex(usize),
    Field(usize, usize /*FieldIndex*/),
    Index(usize, usize),
}

impl ContextTarget {
    #[inline]
    #[must_use]
    pub const fn stack_index(&self) -> Option<usize> {
        match self {
            Self::StackIndex(stack_idx) => Some(*stack_idx),
            _ => None,
        }
    }
}

// Deref..
impl ExecutionContext {
    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn perform_single_deref_ctx_target(
        &self,
        at: ContextTarget,
    ) -> InterpResult<ContextTarget> {
        match self.ctx_value(at)? {
            Value::Ref(value) => Ok(*value),
            _ => Ok(at),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn perform_deref_ctx_target(&self, at: ContextTarget) -> InterpResult<ContextTarget> {
        match self.ctx_value(at)? {
            Value::Ref(value) => self.perform_deref_ctx_target(*value),
            _ => Ok(at),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn perform_deref_target(&self, at: AssignTarget) -> InterpResult<ContextTarget> {
        match self.value(at)? {
            Value::Ref(value) => self.perform_deref_ctx_target(*value),
            _ => self.assign_target_to_context_target(at),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_panics_doc)]
    pub fn assign_target_to_context_target(&self, at: AssignTarget) -> InterpResult<ContextTarget> {
        Ok(match at {
            AssignTarget::Local(local_id) => ContextTarget::StackIndex(self.local_index(local_id)),
            AssignTarget::Field(local_id, field_index) => ContextTarget::Field(
                self.perform_deref_target(AssignTarget::Local(local_id))?
                    .stack_index()
                    .expect("Field should be stack index."),
                field_index,
            ),
            AssignTarget::Index(local_id, index_id) => {
                ContextTarget::Index(self.local_index(local_id), self.index_value(index_id)?)
            }
        })
    }
}

pub struct InterpreterState {
    pub entry: OwnerDefId,

    pub native_functions: HashMap<String, Box<KitlangMIRNativeFn>>,

    pub ctx: ExecutionContext,
}

impl InterpreterState {
    const DEFAULT_ENTRY_NAME: &str = "main";

    #[inline]
    #[must_use]
    fn find_entry_point(program: &ProgramType) -> Option<OwnerDefId> {
        let main = program
            .namespace
            .namespace
            .items
            .get(Self::DEFAULT_ENTRY_NAME)?;
        if matches!(main.kind, NamespaceKind::Function) {
            main.id.owner_def_id()
        } else {
            None
        }
    }

    #[inline]
    #[must_use]
    pub fn new(program: &ProgramType) -> Option<Self> {
        let entry = Self::find_entry_point(program)?;
        Some(Self {
            entry,
            native_functions: HashMap::new(),
            ctx: ExecutionContext::default(),
        })
    }
}

impl InterpreterState {
    #[inline]
    pub fn register_native_function<F: IntoMIRKitlangFn>(&mut self, name: &str, func: F) {
        if !self.native_functions.contains_key(name) {
            self.native_functions
                .insert(name.to_string(), func.into_kitlang_fn());
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn call_native_function(&mut self, name: &str, args: &[Value]) -> InterpResult<Value> {
        self.native_functions.get_mut(name).map_or_else(
            || Err(interp_err!("Failed to find native function `{name}`")),
            |f| {
                f(args).ok_or_else(|| {
                    interp_err!("Native function `{name}` failed to return a valid value.")
                })
            },
        )
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn execute_from_entry(&mut self, program: &ProgramType) -> InterpResult<Value> {
        // log::info!("Running program: {:#?}", program);
        self.execute_function(program, self.entry, &[])
    }
}

// Implementation details..
impl InterpreterState {
    /// Execute a function by its `OwnerDefId`.
    /// # Errors
    /// Returns an error if the function is not found or if there is an error during execution.
    /// The returned error contains diagnostic information about the error.
    pub fn execute_function(
        &mut self,
        program: &ProgramType,
        id: OwnerDefId,
        args: &[Value],
    ) -> InterpResult<Value> {
        if let Some(kit_body) = program.mir.bodies.get(&id) {
            self.execute_kit_function(program, kit_body, args)
        } else if let Some(native_function_name) = program.mir.native_function_links.get(&id) {
            self.call_native_function(native_function_name, args)
        } else {
            Err(interp_err!("Unknown function reference: {id:?}"))
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn execute_kit_function(
        &mut self,
        program: &ProgramType,
        body: &Body,
        args: &[Value],
    ) -> InterpResult<Value> {
        if body.arg_count != args.len() as u32 {
            return Err(interp_err!(
                "Argument count mismatch! {} != {}",
                body.arg_count,
                args.len()
            ));
        }

        self.ctx.push_stack(body.locals.len());
        self.ctx.push_arguments(args);

        let mut current_block = body
            .block(BasicBlockId::ENTRY_BLOCK)
            .ok_or_else(|| interp_err!("Failed to get entry block from body"))?;

        // TODO: I must have been drunk writing this, but this needs to be fixed.
        for _i in 0..10000 {
            for statement in &current_block.statements {
                self.execute_statement(statement)?;
            }

            match &current_block.exit_directive.kind {
                BlockExitKind::Goto(basic_block_id) => {
                    current_block = body.block(*basic_block_id).ok_or_else(|| {
                        interp_err!("Failed to get goto block: {basic_block_id:?}")
                    })?;
                }
                BlockExitKind::Branch(operand, true_block, false_block) => {
                    let condition = match operand {
                        Operand::Copy(at) => {
                            let cond_target = self.ctx.perform_deref_target(*at)?;
                            let value = self.ctx.ctx_value(cond_target)?;
                            value.bool().ok_or_else(|| {
                                interp_err!(
                                    "Failed to get block branch condition value as a boolean!"
                                )
                            })?
                        }
                        Operand::Literal(Literal::Boolean(b)) => *b,
                        _ => {
                            return Err(interp_err!("Invalid branch operand type! {operand:?}"));
                        }
                    };

                    let block_id = if condition { *true_block } else { *false_block };
                    current_block = body.block(block_id).ok_or_else(|| {
                        interp_err!(
                            "Failed to get basic block from body in branch with id: {block_id:?}"
                        )
                    })?;
                }
                BlockExitKind::Return => break,
                BlockExitKind::Call(local_id, owner_def_id, operands, basic_block_id) => {
                    let args: Vec<_> = operands
                        .iter()
                        .map(|o| match o {
                            Operand::Copy(local) => Ok(self.ctx.value(*local)?.clone()),
                            Operand::LValue(local) => {
                                let ctx_tar = self.ctx.assign_target_to_context_target(*local)?;
                                Ok(Value::Ref(ctx_tar))
                            }
                            Operand::Literal(literal) => Ok(literal.into()),
                            Operand::Unit | Operand::Const => Ok(Value::Unit),
                        })
                        .collect::<InterpResult<Vec<_>>>()?;

                    let result = self.execute_function(program, *owner_def_id, &args)?;
                    self.perform_assignment(AssignTarget::from_local(*local_id), result)?;

                    current_block = body.block(*basic_block_id).ok_or_else(|| {
                        interp_err!(
                            "Failed to get basic block from body with id: {basic_block_id:?}"
                        )
                    })?;
                }
            }
        }

        let return_value = self.ctx.local(LocalId::RETURN_VALUE).cloned();
        self.ctx.pop_stack();
        return_value
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    fn eval_operand(&self, operand: &Operand) -> InterpResult<Value> {
        match operand {
            Operand::Copy(local) => Ok(self.ctx.value(*local)?.clone()),
            Operand::LValue(local) => Ok(Value::Ref(
                self.ctx.assign_target_to_context_target(*local)?,
            )),
            Operand::Unit => Ok(Value::Unit),
            Operand::Literal(literal) => Ok(literal.into()),
            Operand::Const => {
                error!("Warning: Const not implemented! Continuing...");
                Ok(Value::Unit)
            }
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    fn eval_rvalue(&self, rvalue: &RValue) -> InterpResult<Value> {
        match rvalue {
            RValue::Unchanged(operand) => Ok(self.eval_operand(operand)?),
            RValue::BinaryOp(binary_op_kind, (lhs, rhs)) => {
                let lhs_value = self.eval_operand(lhs)?;
                let rhs_value = self.eval_operand(rhs)?;

                lhs_value.perform_binary_op(&rhs_value, *binary_op_kind).ok_or_else(|| {
                    interp_err!(
                        "Failed to perform binary operation: {binary_op_kind:?} between {lhs_value:?} and {rhs_value:?}"
                    )
                })
            }
            RValue::UnaryOp(unary_op_kind, operand) => {
                let rhs_value = self.eval_operand(operand)?;
                match unary_op_kind {
                    UnaryOpKind::Dereference => {
                        let Value::Ref(ctx_tar) = rhs_value else {
                            return Err(interp_err!(
                                "Cannot deref non-reference value: {rhs_value:?}"
                            ));
                        };

                        let final_tar = self.ctx.perform_single_deref_ctx_target(ctx_tar)?;
                        self.ctx.ctx_value(final_tar).cloned()
                    }
                    _ => rhs_value.perform_unary_op(*unary_op_kind).ok_or_else(|| {
                        interp_err!(
                            "Failed to perform unary operation: {unary_op_kind:?} on {rhs_value:?}"
                        )
                    }),
                }
            }
            RValue::Increment(operand) => {
                let rhs_value = self.eval_operand(operand)?;
                rhs_value.perform_increment().ok_or_else(|| {
                    interp_err!("Failed to perform increment operation on {rhs_value:?}")
                })
            }
            RValue::Ref(assign_target) => Ok(Value::Ref(
                self.ctx.assign_target_to_context_target(*assign_target)?,
            )),
            RValue::ADT(kind, operands) => match kind {
                crate::intermediate::mir::ADTKind::Struct(_) => {
                    let adt_values = operands
                        .iter()
                        .map(|o| self.eval_operand(o))
                        .collect::<InterpResult<Vec<_>>>()?;
                    Ok(Value::ADT(ADTValueKind::Struct(adt_values)))
                }
            },
            RValue::Cast(operand, cast_kind) => {
                let value = self.eval_operand(operand)?;
                self.perform_cast(&value, *cast_kind).ok_or_else(|| {
                    interp_err!("Failed to perform cast operation: {cast_kind:?} on {value:?}")
                })
            }
            RValue::Tuple(vals) => {
                let tuple_values = vals
                    .iter()
                    .map(|o| self.eval_operand(o))
                    .collect::<InterpResult<Vec<_>>>()?;
                Ok(Value::Tuple(tuple_values))
            }
            RValue::Array(elems) => {
                let element_values = elems
                    .iter()
                    .map(|o| self.eval_operand(o))
                    .collect::<InterpResult<Vec<_>>>()?;
                Ok(Value::Array(element_values))
            }
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn perform_assignment(
        &mut self,
        target: AssignTarget,
        new_value: Value,
    ) -> InterpResult<()> {
        let derefed_tar = self.ctx.perform_deref_target(target)?;
        let local_mut = self.ctx.ctx_value_mut(derefed_tar)?;
        if !local_mut.are_matching_types(&new_value) && !local_mut.is_unit() {
            warn!("Non matching types {target:?}: {local_mut:?} => {new_value:?}");
        }
        *local_mut = new_value;
        Ok(())
    }

    #[allow(
        clippy::cast_lossless,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::cast_possible_wrap
    )]
    #[must_use]
    pub fn perform_cast(&self, value: &Value, target_type: CastKind) -> Option<Value> {
        // TODO: Please just implement proper value storage. please
        match target_type {
            CastKind::Int(target_int_size) => match *value {
                Value::UnsignedInteger(v) => Some(Value::Integer(match target_int_size {
                    KitInt::I8 => v as i8 as i64,
                    KitInt::I16 => v as i16 as i64,
                    KitInt::I32 => v as i32 as i64,
                    KitInt::I64 | KitInt::ISize | KitInt::I128 => v as i64,
                })),
                Value::Integer(v) => Some(Value::Integer(match target_int_size {
                    KitInt::I8 => v as i8 as i64,
                    KitInt::I16 => v as i16 as i64,
                    KitInt::I32 => v as i32 as i64,
                    KitInt::I64 | KitInt::ISize | KitInt::I128 => v,
                })),
                Value::Float(v) => Some(Value::Integer(match target_int_size {
                    KitInt::I8 => v as i8 as i64,
                    KitInt::I16 => v as i16 as i64,
                    KitInt::I32 => v as i32 as i64,
                    KitInt::I64 | KitInt::ISize | KitInt::I128 => v as i64,
                })),
                Value::Boolean(b) => Some(Value::Integer(match target_int_size {
                    KitInt::I8 => b as i8 as i64,
                    KitInt::I16 => b as i16 as i64,
                    KitInt::I32 => b as i32 as i64,
                    KitInt::I64 | KitInt::ISize | KitInt::I128 => b as i64,
                })),
                _ => None,
            },
            CastKind::UInt(target_uint_size) => match *value {
                Value::UnsignedInteger(v) => Some(Value::UnsignedInteger(match target_uint_size {
                    KitUInt::U8 => v as u8 as u64,
                    KitUInt::U16 => v as u16 as u64,
                    KitUInt::U32 => v as u32 as u64,
                    KitUInt::U64 | KitUInt::USize | KitUInt::U128 => v,
                })),
                Value::Integer(v) => Some(Value::UnsignedInteger(match target_uint_size {
                    KitUInt::U8 => v as u8 as u64,
                    KitUInt::U16 => v as u16 as u64,
                    KitUInt::U32 => v as u32 as u64,
                    KitUInt::U64 | KitUInt::USize | KitUInt::U128 => v as u64,
                })),
                Value::Float(v) => Some(Value::UnsignedInteger(match target_uint_size {
                    KitUInt::U8 => v as u8 as u64,
                    KitUInt::U16 => v as u16 as u64,
                    KitUInt::U32 => v as u32 as u64,
                    KitUInt::U64 | KitUInt::USize | KitUInt::U128 => v as u64,
                })),
                Value::Boolean(b) => Some(Value::UnsignedInteger(match target_uint_size {
                    KitUInt::U8 => b as u8 as u64,
                    KitUInt::U16 => b as u16 as u64,
                    KitUInt::U32 => b as u32 as u64,
                    KitUInt::U64 | KitUInt::USize | KitUInt::U128 => b as u64,
                })),
                _ => None,
            },
            CastKind::Float(target_float_size) => match *value {
                Value::UnsignedInteger(v) => Some(Value::Float(match target_float_size {
                    KitFloat::F16 | KitFloat::F32 => v as f32 as f64,
                    KitFloat::F64 | KitFloat::F128 => v as f64,
                })),
                Value::Integer(v) => Some(Value::Float(match target_float_size {
                    KitFloat::F16 | KitFloat::F32 => v as f32 as f64,
                    KitFloat::F64 | KitFloat::F128 => v as f64,
                })),
                Value::Float(v) => Some(Value::Float(match target_float_size {
                    KitFloat::F16 | KitFloat::F32 => v as f32 as f64,
                    KitFloat::F64 | KitFloat::F128 => v,
                })),
                _ => None,
            },
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn deref_value(&self, value: Value) -> InterpResult<Value> {
        match value {
            Value::Ref(ctx_tar) => {
                let ctx_deref = self.ctx.perform_deref_ctx_target(ctx_tar)?;
                Ok(self.ctx.ctx_value(ctx_deref)?.clone())
            }
            _ => Ok(value),
        }
    }

    #[inline]
    #[allow(clippy::missing_errors_doc)]
    pub fn execute_statement(&mut self, statement: &Statement) -> InterpResult<()> {
        match &statement.kind {
            StatementKind::Assign(target, rvalue) => match self.eval_rvalue(rvalue) {
                Ok(new_value) => {
                    self.perform_assignment(*target, new_value)?;
                }
                Err(e) => {
                    return Err(interp_err!(
                        "Failed to evaluate rvalue for assignment: {rvalue:?} Caused by: {e}"
                    ));
                }
            },
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Interpreter {
    pub program: Option<RefCell<ProgramType>>,
    pub state: Option<InterpreterState>,
}

impl Interpreter {
    #[inline]
    #[must_use]
    pub fn new_with_program(program: ProgramType) -> Option<Self> {
        let state = InterpreterState::new(&program)?;

        Some(Self {
            program: Some(RefCell::new(program)),
            state: Some(state),
        })
    }

    #[inline]
    #[must_use]
    pub fn load_program(&mut self, program: ProgramType) -> Option<()> {
        self.state = Some(InterpreterState::new(&program)?);
        self.program = Some(RefCell::new(program));

        Some(())
    }

    #[inline]
    pub fn register_native_function<F: IntoMIRKitlangFn>(&mut self, name: &str, func: F) {
        if let Some(state) = self.state.as_mut() {
            state.register_native_function(name, func);
        }
    }

    /// Execute the loaded program from its entry point.
    /// # Errors
    /// Errors if no program is loaded or if execution fails.
    pub fn execute_from_entry(&mut self) -> InterpResult<Value> {
        if let Some(state) = self.state.as_mut()
            && let Some(program) = self.program.as_ref()
        {
            let program_ref = program.borrow();
            return state.execute_from_entry(&program_ref);
        }

        Err(interp_err!("Interpreter does not have a program loaded!"))
    }
}

pub type RegisterNativeFns = fn(interpreter: &mut Interpreter);

fn internal_execute_mir(
    interpreter: &mut Interpreter,
    time_execution: bool,
) -> InterpResult<Value> {
    let (result_value, execution_time) = if time_execution {
        crate::profiling::measure_execution(|| interpreter.execute_from_entry())
    } else {
        (interpreter.execute_from_entry(), Duration::ZERO)
    };

    if time_execution {
        debug!(
            "[Profiling] Program executed in {}.",
            crate::profiling::format_duration(execution_time)
        );
    }

    result_value
}

/// Execute MIR with no default compiler intrinsics registered.
/// # Errors
/// Errors if the entry point could not be found or if execution fails.
pub fn execute_mir_no_intrinsics(
    mir: MIR,
    meta_data: &crate::prelude::ProgramMetaData,
    register_fns: RegisterNativeFns,
    time_execution: bool,
) -> crate::KitlangResult<Value> {
    let result = Interpreter::new_with_program(Program::new(
        mir,
        meta_data.type_registry.clone(),
        meta_data.namespace.clone(),
    ))
    .map_or(
        Err(interp_err!("Failed to find entry point.")),
        |mut interpreter| {
            register_fns(&mut interpreter);

            internal_execute_mir(&mut interpreter, time_execution)
        },
    );

    Ok(result?)
}

/// Execute MIR with default compiler intrinsics registered.
/// # Errors
/// Errors if the entry point could not be found or if execution fails.
pub fn execute_mir(
    mir: MIR,
    meta_data: &crate::prelude::ProgramMetaData,
    register_fns: RegisterNativeFns,
    time_execution: bool,
) -> crate::KitlangResult<Value> {
    Ok(Interpreter::new_with_program(Program::new(
        mir,
        meta_data.type_registry.clone(),
        meta_data.namespace.clone(),
    ))
    .map_or(
        Err(interp_err!("Failed to find entry point.")),
        |mut interpreter| {
            intrinsics::register_compiler_intrinsics(&mut interpreter, false);
            register_fns(&mut interpreter);

            internal_execute_mir(&mut interpreter, time_execution)
        },
    )?)
}

/// Execute MIR with default compiler intrinsics registered.
/// # Errors
/// Errors if the entry point could not be found or if execution fails.
pub fn execute_mir_no_io(
    mir: MIR,
    meta_data: &crate::prelude::ProgramMetaData,
    register_fns: RegisterNativeFns,
    time_execution: bool,
) -> crate::KitlangResult<Value> {
    Ok(Interpreter::new_with_program(Program::new(
        mir,
        meta_data.type_registry.clone(),
        meta_data.namespace.clone(),
    ))
    .map_or(
        Err(interp_err!("Failed to find entry point.")),
        |mut interpreter| {
            intrinsics::register_compiler_intrinsics(&mut interpreter, true);
            register_fns(&mut interpreter);

            internal_execute_mir(&mut interpreter, time_execution)
        },
    )?)
}

// Compiler intrinsics implementations..

mod intrinsics {
    #[cfg(not(feature = "webasm"))]
    use std::io::Write;

    use crate::interpreter::mir_interpreter::Interpreter;
    use crate::register_native_fn;
    use kitlang_macros::kitlang_native_fn;

    /// Register the default compiler intrinsics.
    pub fn register_compiler_intrinsics(interpreter: &mut Interpreter, no_io: bool) {
        register_native_fn!(
            interpreter,
            to_lower,
            str_len,
            i64_to_string,
            string_to_i64,
            u64_to_string,
            string_to_u64,
            f64_to_string,
            string_to_f64,
            bool_to_string,
            string_to_bool,
            i64_sqrt,
            u64_sqrt,
            f64_sqrt,
            i64_abs,
            f64_abs
        );

        #[cfg(not(feature = "webasm"))]
        if !no_io {
            register_native_fn!(
                interpreter,
                print,
                println,
                read_line,
                input,
                input_placeholder
            );
        }
    }

    #[kitlang_native_fn]
    fn i64_to_string(x: i64) -> String {
        x.to_string()
    }
    #[kitlang_native_fn]
    fn string_to_i64(s: String) -> (i64, bool) {
        s.parse::<i64>().map(|v| (v, false)).unwrap_or((0, true))
    }
    #[kitlang_native_fn]
    fn f64_to_string(x: f64) -> String {
        x.to_string()
    }
    #[kitlang_native_fn]
    fn string_to_f64(s: String) -> (f64, bool) {
        s.parse::<f64>().map(|v| (v, false)).unwrap_or((0.0, true))
    }
    #[kitlang_native_fn]
    fn u64_to_string(x: u64) -> String {
        x.to_string()
    }
    #[kitlang_native_fn]
    fn string_to_u64(s: String) -> (u64, bool) {
        s.parse::<u64>().map(|v| (v, false)).unwrap_or((0, true))
    }
    #[kitlang_native_fn]
    fn bool_to_string(x: bool) -> String {
        x.to_string()
    }
    #[kitlang_native_fn]
    fn string_to_bool(s: String) -> (bool, bool) {
        s.parse::<bool>()
            .map(|v| (v, false))
            .unwrap_or((false, true))
    }
    #[kitlang_native_fn]
    fn str_len(s: String) -> usize {
        s.len()
    }
    #[kitlang_native_fn]
    fn to_lower(s: String) -> String {
        s.to_lowercase()
    }

    #[kitlang_native_fn]
    fn i64_sqrt(i: i64) -> i64 {
        i.isqrt()
    }
    #[kitlang_native_fn]
    fn u64_sqrt(u: u64) -> u64 {
        u.isqrt()
    }
    #[kitlang_native_fn]
    fn f64_sqrt(f: f64) -> f64 {
        f.sqrt()
    }

    #[kitlang_native_fn]
    fn i64_abs(i: i64) -> i64 {
        i.abs()
    }
    #[kitlang_native_fn]
    fn f64_abs(f: f64) -> f64 {
        f.abs()
    }

    #[cfg(not(feature = "webasm"))]
    #[kitlang_native_fn]
    fn print(s: String) {
        print!("{s}");
        std::io::stdout().flush().expect("Failed to flush.");
    }
    #[cfg(not(feature = "webasm"))]
    #[kitlang_native_fn]
    fn println(s: String) {
        println!("{s}");
    }

    #[cfg(not(feature = "webasm"))]
    #[kitlang_native_fn]
    fn read_line() -> String {
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        input.trim_end().to_string()
    }
    #[cfg(not(feature = "webasm"))]
    #[kitlang_native_fn]
    fn input(prompt: String) -> String {
        print!("{prompt} ");
        std::io::stdout().flush().expect("Failed to flush.");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        input.trim_end().to_string()
    }
    #[cfg(not(feature = "webasm"))]
    #[kitlang_native_fn]
    fn input_placeholder(prompt: String, default: String) -> String {
        let mut input = String::new();
        print!("{prompt} ");
        std::io::stdout().flush().expect("Failed to flush.");
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line.");
        let trimmed = input.trim();
        if trimmed.is_empty() {
            default
        } else {
            trimmed.to_string()
        }
    }
}
