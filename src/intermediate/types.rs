use crate::ast::Ty as ASTTy;

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
            KitInt::ISize => todo!(),
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
    Abstract, // TODO: Array, Tuple..
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
                eprintln!("KitTy conversion not implemented for: {:?}", a);
                None
            }
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
            KitTy::Abstract => None,
        }
    }
}
