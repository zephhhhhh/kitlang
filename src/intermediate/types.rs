use crate::{
    ast::{BinaryOpKind, Ty as ASTTy, UnaryOpKind},
    intermediate::resolver::TypeID,
};

use log::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitInt {
    ISize,
    I8,
    I16,
    I32,
    I64,
    I128,
}

impl KitInt {
    pub fn symbol_str(&self) -> &'static str {
        match *self {
            KitInt::ISize => "isize",
            KitInt::I8 => "i8",
            KitInt::I16 => "i16",
            KitInt::I32 => "i32",
            KitInt::I64 => "i64",
            KitInt::I128 => "i128",
        }
    }

    pub fn bit_width(&self) -> u64 {
        match *self {
            // For now we will just use the size of a I64 on all targets.
            KitInt::ISize => 64,
            KitInt::I8 => 8,
            KitInt::I16 => 16,
            KitInt::I32 => 32,
            KitInt::I64 => 64,
            KitInt::I128 => 128,
        }
    }

    pub fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    pub fn largest_width(&self, other: &KitInt) -> KitInt {
        if self > other { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitUInt {
    USize,
    U8,
    U16,
    U32,
    U64,
    U128,
}

impl KitUInt {
    pub fn symbol_str(&self) -> &'static str {
        match *self {
            KitUInt::USize => "usize",
            KitUInt::U8 => "u8",
            KitUInt::U16 => "u16",
            KitUInt::U32 => "u32",
            KitUInt::U64 => "u64",
            KitUInt::U128 => "u128",
        }
    }

    pub fn bit_width(&self) -> u64 {
        match *self {
            KitUInt::USize => (std::mem::size_of::<usize>() * 8) as u64,
            KitUInt::U8 => 8,
            KitUInt::U16 => 16,
            KitUInt::U32 => 32,
            KitUInt::U64 => 64,
            KitUInt::U128 => 128,
        }
    }

    pub fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    pub fn largest_width(&self, other: &KitUInt) -> KitUInt {
        if self > other { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitFloat {
    F16,
    F32,
    F64,
    F128,
}

impl KitFloat {
    pub fn symbol_str(&self) -> &'static str {
        match *self {
            KitFloat::F16 => "f16",
            KitFloat::F32 => "f32",
            KitFloat::F64 => "f64",
            KitFloat::F128 => "f128",
        }
    }

    pub fn bit_width(&self) -> u64 {
        match *self {
            KitFloat::F16 => 16,
            KitFloat::F32 => 32,
            KitFloat::F64 => 64,
            KitFloat::F128 => 128,
        }
    }

    pub fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    pub fn largest_width(&self, other: &KitFloat) -> KitFloat {
        if self > other { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitTy {
    Unit,
    Int(KitInt),
    UInt(KitUInt),
    Float(KitFloat),
    Boolean,
    Char,
    String,
    // Structs, etc..
    Abstract(TypeID),
    // TODO: Array, Tuple..
}

impl KitTy {
    pub fn try_from_ast_ty(ty: &ASTTy) -> Option<Self> {
        match ty {
            ASTTy::Unit(_) => Some(Self::Unit),
            ASTTy::Type(spanned_ident_path) => {
                KitTy::from_primitive_ty_str(spanned_ident_path.path.path_stem())
            }
            ASTTy::Infer | ASTTy::This(_) => None,
            a => {
                error!("KitTy conversion not implemented for: {:?}", a);
                None
            }
        }
    }
}

impl KitTy {
    pub fn is_primitive(&self) -> bool {
        match self {
            KitTy::Unit
            | KitTy::Int(_)
            | KitTy::UInt(_)
            | KitTy::Float(_)
            | KitTy::Boolean
            | KitTy::Char
            | KitTy::String => true,
            KitTy::Abstract(_) => false,
        }
    }

    pub fn is_unit(&self) -> bool {
        matches!(self, KitTy::Unit)
    }

    pub fn is_int(&self) -> bool {
        matches!(self, KitTy::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, KitTy::Float(_))
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, KitTy::Boolean)
    }

    pub fn is_char(&self) -> bool {
        matches!(self, KitTy::Char)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, KitTy::String)
    }

    pub fn is_abstract(&self) -> bool {
        matches!(self, KitTy::Abstract(_))
    }
}

impl KitTy {
    pub fn is_compatible(&self, other: &KitTy) -> bool {
        match self {
            KitTy::Unit => matches!(other, KitTy::Unit),
            KitTy::Int(_) => matches!(other, KitTy::Int(_)),
            KitTy::UInt(_) => matches!(other, KitTy::UInt(_)),
            KitTy::Float(_) => matches!(other, KitTy::Float(_)),
            KitTy::Boolean => matches!(other, KitTy::Boolean),
            KitTy::Char => matches!(other, KitTy::Char),
            KitTy::String => matches!(other, KitTy::String),
            KitTy::Abstract(_) => false,
        }
    }

    pub fn unary_op_result_type(&self, op_kind: UnaryOpKind) -> Option<KitTy> {
        match self {
            KitTy::Unit => None,
            KitTy::Int(kit_int) => match op_kind {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not => Some(KitTy::Int(*kit_int)),
                UnaryOpKind::Negate => Some(KitTy::Int(*kit_int)),
            },
            KitTy::UInt(kit_uint) => match op_kind {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not => Some(KitTy::UInt(*kit_uint)),
                UnaryOpKind::Negate => None,
            },
            KitTy::Float(kit_float) => match op_kind {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not => None,
                UnaryOpKind::Negate => Some(KitTy::Float(*kit_float)),
            },
            KitTy::Boolean => match op_kind {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not => Some(KitTy::Boolean),
                UnaryOpKind::Negate => None,
            },
            KitTy::Char => match op_kind {
                UnaryOpKind::Dereference => None,
                _ => None,
            },
            KitTy::String => match op_kind {
                UnaryOpKind::Dereference => None,
                _ => None,
            },
            KitTy::Abstract(_) => {
                warn!("Tried to do abstract result type for unary.");
                None
            }
        }
    }

    pub fn binary_op_result_type(&self, other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
        fn unit_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
            match (other, op_kind) {
                (
                    KitTy::Unit,
                    BinaryOpKind::NotEqual
                    | BinaryOpKind::Equal
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual,
                ) => Some(KitTy::Unit),
                _ => None,
            }
        }

        fn int_result_type(lhs: &KitInt, other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
            match other {
                KitTy::Int(rhs_int) => match op_kind {
                    BinaryOpKind::Add
                    | BinaryOpKind::Sub
                    | BinaryOpKind::Mul
                    | BinaryOpKind::Div
                    | BinaryOpKind::Mod => Some(KitTy::Int(lhs.largest_width(rhs_int))),
                    BinaryOpKind::BitwiseXOR
                    | BinaryOpKind::BitwiseAND
                    | BinaryOpKind::BitwiseOR => {
                        if lhs == rhs_int {
                            Some(KitTy::Int(*lhs))
                        } else {
                            None
                        }
                    }
                    BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => Some(KitTy::Int(*lhs)),
                    BinaryOpKind::And | BinaryOpKind::Or => None,
                    BinaryOpKind::NotEqual
                    | BinaryOpKind::Equal
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual => Some(KitTy::Boolean),
                },
                _ => None,
            }
        }

        fn uint_result_type(lhs: &KitUInt, other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
            match other {
                KitTy::UInt(rhs_uint) => match op_kind {
                    BinaryOpKind::Add
                    | BinaryOpKind::Sub
                    | BinaryOpKind::Mul
                    | BinaryOpKind::Div
                    | BinaryOpKind::Mod => Some(KitTy::UInt(lhs.largest_width(rhs_uint))),
                    BinaryOpKind::BitwiseXOR
                    | BinaryOpKind::BitwiseAND
                    | BinaryOpKind::BitwiseOR => {
                        if lhs == rhs_uint {
                            Some(KitTy::UInt(*lhs))
                        } else {
                            None
                        }
                    }
                    BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => Some(KitTy::UInt(*lhs)),
                    BinaryOpKind::And | BinaryOpKind::Or => None,
                    BinaryOpKind::NotEqual
                    | BinaryOpKind::Equal
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual => Some(KitTy::Boolean),
                },
                _ => None,
            }
        }

        fn float_result_type(
            lhs: &KitFloat,
            other: &KitTy,
            op_kind: BinaryOpKind,
        ) -> Option<KitTy> {
            match other {
                KitTy::Float(rhs_float) => match op_kind {
                    BinaryOpKind::Add
                    | BinaryOpKind::Sub
                    | BinaryOpKind::Mul
                    | BinaryOpKind::Div
                    | BinaryOpKind::Mod => Some(KitTy::Float(lhs.largest_width(rhs_float))),
                    BinaryOpKind::BitwiseXOR
                    | BinaryOpKind::BitwiseAND
                    | BinaryOpKind::BitwiseOR => None,
                    BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => None,
                    BinaryOpKind::Or | BinaryOpKind::And => None,
                    BinaryOpKind::NotEqual
                    | BinaryOpKind::Equal
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual => Some(KitTy::Boolean),
                },
                _ => None,
            }
        }

        fn boolean_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
            match (other, op_kind) {
                (
                    KitTy::Boolean,
                    BinaryOpKind::And
                    | BinaryOpKind::Or
                    | BinaryOpKind::Equal
                    | BinaryOpKind::NotEqual
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual,
                ) => Some(KitTy::Boolean),
                _ => None,
            }
        }

        fn char_result_type(_other: &KitTy, _op_kind: BinaryOpKind) -> Option<KitTy> {
            todo!()
        }

        fn string_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
            match (other, op_kind) {
                (
                    KitTy::String,
                    BinaryOpKind::And
                    | BinaryOpKind::Or
                    | BinaryOpKind::Equal
                    | BinaryOpKind::NotEqual
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual,
                ) => Some(KitTy::Boolean),
                (KitTy::String, BinaryOpKind::Add) => Some(KitTy::String),
                _ => None,
            }
        }

        match self {
            KitTy::Unit => unit_result_type(other, op_kind),
            KitTy::Int(kit_int) => int_result_type(kit_int, other, op_kind),
            KitTy::UInt(kit_uint) => uint_result_type(kit_uint, other, op_kind),
            KitTy::Float(kit_float) => float_result_type(kit_float, other, op_kind),
            KitTy::Boolean => boolean_result_type(other, op_kind),
            KitTy::Char => char_result_type(other, op_kind),
            KitTy::String => string_result_type(other, op_kind),
            KitTy::Abstract(_) => {
                warn!("Tried to do abstract result type.");
                None
            }
        }
    }

    pub fn cast_result_type(&self, target: &KitTy) -> Option<KitTy> {
        if self == target {
            return Some(*target);
        }
        match (self, target) {
            // Allow casting between same kinds..
            (KitTy::Int(_), KitTy::Int(_))
            | (KitTy::UInt(_), KitTy::UInt(_))
            | (KitTy::Float(_), KitTy::Float(_)) => Some(*target),
            // Allow casting between int/uint/float..
            (KitTy::Int(_), KitTy::UInt(_))
            | (KitTy::Int(_), KitTy::Float(_))
            | (KitTy::UInt(_), KitTy::Int(_))
            | (KitTy::UInt(_), KitTy::Float(_))
            | (KitTy::Float(_), KitTy::Int(_))
            | (KitTy::Float(_), KitTy::UInt(_)) => Some(*target),
            (KitTy::Boolean, KitTy::UInt(_)) => Some(*target),
            (KitTy::Boolean, KitTy::Int(_)) => Some(*target),
            // Disallow other casts for now..
            _ => None,
        }
    }
}

impl From<KitInt> for KitTy {
    fn from(value: KitInt) -> Self {
        Self::Int(value)
    }
}

impl From<KitUInt> for KitTy {
    fn from(value: KitUInt) -> Self {
        Self::UInt(value)
    }
}

impl From<KitFloat> for KitTy {
    fn from(value: KitFloat) -> Self {
        Self::Float(value)
    }
}

macro_rules! define_primitive_tys {
    (
        $(
            ($prim_str: pat_param, $result_expr: expr)
        ),+
    ) =>{
        impl KitTy {
            #[inline]
            pub fn from_primitive_ty_str(s: &str) -> Option<Self> {
                match s {
                    $($prim_str => Some($result_expr),)*
                    _ => None,
                }
            }
        }
    };
}

define_primitive_tys!(
    // Unit type..
    ("()", KitTy::Unit),
    // Integers..
    ("i8", KitInt::I8.into()),
    ("i16", KitInt::I16.into()),
    ("i32", KitInt::I32.into()),
    ("i64", KitInt::I64.into()),
    ("i128", KitInt::I128.into()),
    ("isize", KitInt::ISize.into()),
    // Unsigned integers..
    ("u8", KitUInt::U8.into()),
    ("u16", KitUInt::U16.into()),
    ("u32", KitUInt::U32.into()),
    ("u64", KitUInt::U64.into()),
    ("u128", KitUInt::U128.into()),
    ("usize", KitUInt::USize.into()),
    // Floating point..
    ("f16", KitFloat::F16.into()),
    ("f32", KitFloat::F32.into()),
    ("f64", KitFloat::F64.into()),
    ("f128", KitFloat::F64.into()),
    ("bool", KitTy::Boolean),
    ("char", KitTy::Char),
    ("string", KitTy::String)
);

impl KitTy {
    pub fn to_type_str(&self) -> Option<String> {
        match self {
            KitTy::Unit => Some("()".into()),
            KitTy::Int(i) => Some(i.symbol_str().to_string()),
            KitTy::UInt(u) => Some(u.symbol_str().to_string()),
            KitTy::Float(f) => Some(f.symbol_str().to_string()),
            KitTy::Boolean => Some("bool".into()),
            KitTy::Char => Some("char".into()),
            KitTy::String => Some("string".into()),
            KitTy::Abstract(_) => None,
        }
    }
}
