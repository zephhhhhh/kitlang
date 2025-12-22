use crate::{
    ast::{BinaryOpKind, Ty as ASTTy, UnaryOpKind},
    intermediate::resolver::TypeID,
};

use log::{error, warn};

/// Represents the integer types in Kitlang.
/// This is not a value in itself, but rather a type representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitInt {
    /// Represents the size of a pointer on the target architecture.
    ISize,
    /// `Byte` (8 bits).
    I8,
    /// `Word` (16 bits).
    I16,
    /// `DWord` (32 bits).
    I32,
    /// `QWord` (64 bits).
    I64,
    /// `OWord` (128 bits).
    I128,
}

impl KitInt {
    /// Returns the symbol string (the type as written in a source file) of the integer type.
    #[inline]
    #[must_use]
    pub const fn symbol_str(&self) -> &'static str {
        match *self {
            Self::ISize => "isize",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
        }
    }

    /// Returns the bit width of the integer type.
    #[inline]
    #[must_use]
    pub const fn bit_width(&self) -> u64 {
        match *self {
            // For now we will just use the size of a I64 on all targets.
            Self::I8 => 8,
            Self::I16 => 16,
            Self::I32 => 32,
            Self::ISize | Self::I64 => 64,
            Self::I128 => 128,
        }
    }

    /// Returns the byte width of the integer type.
    #[inline]
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    /// Returns the largest width between two integer types.
    #[inline]
    #[must_use]
    pub const fn largest_width(&self, other: &Self) -> Self {
        if self.bit_width() > other.bit_width() {
            *self
        } else {
            *other
        }
    }
}

/// Represents the unsigned integer types in Kitlang.
/// This is not a value in itself, but rather a type representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitUInt {
    /// Represents the size of a pointer on the target architecture.
    USize,
    /// `Byte` (8 bits).
    U8,
    /// `Word` (16 bits).
    U16,
    /// `DWord` (32 bits).
    U32,
    /// `QWord` (64 bits).
    U64,
    /// `OWord` (128 bits).
    U128,
}

impl KitUInt {
    /// Returns the symbol string (the type as written in a source file) of the unsigned integer type.
    #[inline]
    #[must_use]
    pub const fn symbol_str(&self) -> &'static str {
        match *self {
            Self::USize => "usize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
        }
    }

    /// Returns the bit width of the unsigned integer type.
    #[inline]
    #[must_use]
    pub const fn bit_width(&self) -> u64 {
        match *self {
            Self::USize => (std::mem::size_of::<usize>() * 8) as u64,
            Self::U8 => 8,
            Self::U16 => 16,
            Self::U32 => 32,
            Self::U64 => 64,
            Self::U128 => 128,
        }
    }

    /// Returns the byte width of the unsigned integer type.
    #[inline]
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    /// Returns the largest width between two unsigned integer types.
    #[inline]
    #[must_use]
    pub const fn largest_width(&self, other: &Self) -> Self {
        if self.bit_width() > other.bit_width() {
            *self
        } else {
            *other
        }
    }
}

/// Represents the floating point types in Kitlang.
/// This is not a value in itself, but rather a type representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitFloat {
    /// Half precision floating point.
    F16,
    /// Single precision floating point.
    F32,
    /// Double precision floating point.
    F64,
    /// Quadruple precision floating point.
    F128,
}

impl KitFloat {
    /// Returns the symbol string (the type as written in a source file) of the floating point type.
    #[inline]
    #[must_use]
    pub const fn symbol_str(&self) -> &'static str {
        match *self {
            Self::F16 => "f16",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::F128 => "f128",
        }
    }

    /// Returns the bit width of the floating point type.
    #[inline]
    #[must_use]
    pub const fn bit_width(&self) -> u64 {
        match *self {
            Self::F16 => 16,
            Self::F32 => 32,
            Self::F64 => 64,
            Self::F128 => 128,
        }
    }

    /// Returns the byte width of the floating point type.
    #[inline]
    #[must_use]
    pub const fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    /// Returns the largest width between two floating point types.
    #[inline]
    #[must_use]
    pub const fn largest_width(&self, other: &Self) -> Self {
        if self.bit_width() > other.bit_width() {
            *self
        } else {
            *other
        }
    }
}

/// Lightweight and copyable representation of types in Kitlang.
///
/// This is not a value in itself, but rather a type representation.
///
/// It can represent primitive types, as well as user-defined types.
///
/// Primitive types are represented directly, while user-defined types are represented
/// by their `TypeID`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitTy {
    /// Unit type
    Unit,
    /// Signed integer types
    Int(KitInt),
    /// Unsigned integer types
    UInt(KitUInt),
    /// Floating point types
    Float(KitFloat),
    /// Boolean type
    Boolean,
    /// Character type (Unicode scalar value)
    Char,
    /// String type
    String,
    /// User-defined / abstract types, denoted by their `TypeID`.
    Abstract(TypeID),
    /// Tuple type..
    Tuple(Vec<KitTy>), // TODO: Array, Tuple..
}

impl KitTy {
    /// Tries to convert an AST type to a `KitTy`.
    /// # Returns
    /// * `Some(KitTy)` if the AST type corresponds to a primitive type.
    /// * `None` if the AST type is not a primitive type (e.g., inferred type, user-defined type).
    #[inline]
    #[must_use]
    pub fn try_from_ast_ty(ty: &ASTTy) -> Option<Self> {
        match ty {
            ASTTy::Unit(_) => Some(Self::Unit),
            ASTTy::Type(spanned_ident_path) => {
                Self::from_primitive_ty_str(spanned_ident_path.path.path_stem())
            }
            ASTTy::Infer | ASTTy::This(..) | ASTTy::Tuple(..) => None,
            a => {
                error!("KitTy conversion not implemented for: {a:?}");
                None
            }
        }
    }
}

impl KitTy {
    /// Checks if the type is a primitive type.
    #[inline]
    #[must_use]
    pub const fn is_primitive(&self) -> bool {
        !matches!(self, Self::Abstract(_))
    }

    /// Returns true if the type is the unit type.
    #[inline]
    #[must_use]
    pub const fn is_unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    /// Returns true if the type is an integer type.
    #[inline]
    #[must_use]
    pub const fn is_int(&self) -> bool {
        matches!(self, Self::Int(_))
    }

    /// Returns true if the type is an unsigned integer type.
    #[inline]
    #[must_use]
    pub const fn is_uint(&self) -> bool {
        matches!(self, Self::UInt(_))
    }

    /// Returns true if the type is a floating point type.
    #[inline]
    #[must_use]
    pub const fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }

    /// Returns true if the type is a boolean type.
    #[inline]
    #[must_use]
    pub const fn is_bool(&self) -> bool {
        matches!(self, Self::Boolean)
    }

    /// Returns true if the type is a char type.
    #[inline]
    #[must_use]
    pub const fn is_char(&self) -> bool {
        matches!(self, Self::Char)
    }

    /// Returns true if the type is a string type.
    #[inline]
    #[must_use]
    pub const fn is_string(&self) -> bool {
        matches!(self, Self::String)
    }

    /// Returns true if the type is an abstract (user-defined) type.
    #[inline]
    #[must_use]
    pub const fn is_abstract(&self) -> bool {
        matches!(self, Self::Abstract(_))
    }
}

impl KitTy {
    /// Returns the bit width of type.
    #[inline]
    #[must_use]
    pub fn bit_width(&self) -> u64 {
        match self {
            Self::Int(kit_int) => kit_int.bit_width(),
            Self::UInt(kit_uint) => kit_uint.bit_width(),
            Self::Float(kit_float) => kit_float.bit_width(),
            Self::Unit => 0,
            Self::Boolean => 8,
            Self::Char => 32,
            Self::String => 192,
            Self::Abstract(_) => 64,
            Self::Tuple(i) => i.iter().map(KitTy::bit_width).sum(),
        }
    }

    /// Returns the byte width of the type.
    #[inline]
    #[must_use]
    pub fn byte_count(&self) -> u64 {
        self.bit_width() / 8
    }

    /// Returns the largest width between types.
    #[inline]
    #[must_use]
    pub fn largest_width(&self, other: &Self) -> Self {
        if self.bit_width() > other.bit_width() {
            self.clone()
        } else {
            other.clone()
        }
    }
}

impl KitTy {
    /// Returns the result type of applying a unary operation to this type.
    #[must_use]
    pub fn unary_op_result_type(&self, op_kind: UnaryOpKind) -> Option<Self> {
        match self {
            Self::Int(kit_int) => match op_kind {
                UnaryOpKind::Dereference => None,
                UnaryOpKind::Not | UnaryOpKind::Negate => Some(Self::Int(*kit_int)),
            },
            Self::UInt(kit_uint) => match op_kind {
                UnaryOpKind::Not => Some(Self::UInt(*kit_uint)),
                UnaryOpKind::Dereference | UnaryOpKind::Negate => None,
            },
            Self::Float(kit_float) => match op_kind {
                UnaryOpKind::Dereference | UnaryOpKind::Not => None,
                UnaryOpKind::Negate => Some(Self::Float(*kit_float)),
            },
            Self::Boolean => match op_kind {
                UnaryOpKind::Not => Some(Self::Boolean),
                UnaryOpKind::Dereference | UnaryOpKind::Negate => None,
            },
            Self::Unit | Self::Char | Self::String => None,
            Self::Abstract(_) => {
                warn!("Tried to do abstract result type for unary.");
                None
            }
            Self::Tuple(..) => {
                warn!("Tried to do tuple result type for unary.");
                None
            }
        }
    }

    // This is a false flag in my eyes, as most of the lines are just nested functions, so it isn't unreadable.
    #[allow(clippy::too_many_lines)]
    /// Returns the result type of applying a binary operation between this type and another type.
    /// `Self` is the left-hand side type, and `other` is the right-hand side type.
    /// # Returns
    /// An [`Option`] containing the resulting `KitTy` if the operation is valid, or `None` if it is not.
    #[must_use]
    pub fn binary_op_result_type(&self, other: &Self, op_kind: BinaryOpKind) -> Option<Self> {
        const fn unit_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
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

        fn int_result_type(lhs: KitInt, other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
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
                        if lhs == *rhs_int {
                            Some(KitTy::Int(lhs))
                        } else {
                            None
                        }
                    }
                    BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => Some(KitTy::Int(lhs)),
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

        fn uint_result_type(lhs: KitUInt, other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
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
                        if lhs == *rhs_uint {
                            Some(KitTy::UInt(lhs))
                        } else {
                            None
                        }
                    }
                    BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => Some(KitTy::UInt(lhs)),
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

        const fn float_result_type(
            lhs: KitFloat,
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
                    BinaryOpKind::NotEqual
                    | BinaryOpKind::Equal
                    | BinaryOpKind::LessThan
                    | BinaryOpKind::LessThanOrEqual
                    | BinaryOpKind::GreaterThan
                    | BinaryOpKind::GreaterThanOrEqual => Some(KitTy::Boolean),
                    BinaryOpKind::BitwiseXOR
                    | BinaryOpKind::BitwiseAND
                    | BinaryOpKind::BitwiseOR
                    | BinaryOpKind::ShiftLeft
                    | BinaryOpKind::ShiftRight
                    | BinaryOpKind::Or
                    | BinaryOpKind::And => None,
                },
                _ => None,
            }
        }

        const fn boolean_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
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

        const fn string_result_type(other: &KitTy, op_kind: BinaryOpKind) -> Option<KitTy> {
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
            Self::Unit => unit_result_type(other, op_kind),
            Self::Int(kit_int) => int_result_type(*kit_int, other, op_kind),
            Self::UInt(kit_uint) => uint_result_type(*kit_uint, other, op_kind),
            Self::Float(kit_float) => float_result_type(*kit_float, other, op_kind),
            Self::Boolean => boolean_result_type(other, op_kind),
            Self::Char => char_result_type(other, op_kind),
            Self::String => string_result_type(other, op_kind),
            Self::Abstract(_) => {
                warn!("Tried to do abstract binary result type.");
                None
            }
            Self::Tuple(..) => {
                warn!("Tried to do tuple binary result type.");
                None
            }
        }
    }

    /// Returns the resulting type after casting to the target type, if the cast is valid.
    /// # Returns
    /// An [`Option`] containing the resulting `KitTy` if the cast is valid, or `None` if it is not.
    #[must_use]
    pub fn cast_result_type(&self, target: &Self) -> Option<Self> {
        if self == target {
            return Some(target.clone());
        }
        match (self, target) {
            // Allow casting between same kinds..
            // Allow casting between int/uint/float..
            (
                Self::Int(_) | Self::UInt(_) | Self::Float(_) | Self::Boolean,
                Self::Int(_) | Self::UInt(_),
            )
            | (Self::Float(_) | Self::Int(_) | Self::UInt(_), Self::Float(_)) => {
                Some(target.clone())
            }
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
    /// Returns the string representation of the type, if it is a primitive type.
    /// # Returns
    /// - `None` if the type is not a primitive type.
    /// - `Some(String)` containing the string representation of the type as written in a source file otherwise.
    #[inline]
    #[must_use]
    pub fn to_type_str(&self) -> Option<String> {
        match self {
            Self::Unit => Some("()".into()),
            Self::Int(i) => Some(i.symbol_str().to_string()),
            Self::UInt(u) => Some(u.symbol_str().to_string()),
            Self::Float(f) => Some(f.symbol_str().to_string()),
            Self::Boolean => Some("bool".into()),
            Self::Char => Some("char".into()),
            Self::String => Some("string".into()),
            Self::Abstract(_) | Self::Tuple(..) => None,
        }
    }
}
