pub mod hir;
pub mod mir;
pub mod resolver;
pub mod type_check;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KitNativeTyKind {
    Int(KitInt),
    UInt(KitUInt),
    Float(KitFloat),
    Boolean,
    Char,
    String,
}
