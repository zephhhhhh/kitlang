use super::mir_interpreter::Value as MIRValue;
use paste::paste;

macro_rules! impl_native_fn_interface {
    ($ident: ident, $interpreter_ty: ty, $value_ty: ty) => {
        paste! {
            pub type [<Kitlang $ident NativeFn>] = dyn for<'a> FnMut(&'a [$value_ty]) -> Option<$value_ty> + 'static;

            pub trait [<Into $ident KitlangFn>] {
                fn into_kitlang_fn(self) -> Box<[<Kitlang $ident NativeFn>]>;
            }

            pub trait [<Into $ident Return>] {
                fn into_kitlang_return(self) -> Option<$value_ty>;
            }

            impl<T: Into<$value_ty>> [<Into $ident Return>] for T {
                fn into_kitlang_return(self) -> Option<$value_ty> {
                    Some(self.into())
                }
            }

            impl<T: Into<$value_ty>> [<Into $ident Return>] for Option<T> {
                fn into_kitlang_return(self) -> Option<$value_ty> {
                    Some(self?.into())
                }
            }

            impl<R, F> [<Into $ident KitlangFn>] for F
            where
                F: for<'a> FnMut(&'a [$value_ty]) -> R + 'static,
                R: [<Into $ident Return>],
            {
                fn into_kitlang_fn(mut self) -> Box<[<Kitlang $ident NativeFn>]> {
                    Box::new(move |args| {
                        self(args).into_kitlang_return()
                    })
                }
            }
        }
    };
}

impl_native_fn_interface!(MIR, MIRInterpreterState, MIRValue);

#[macro_export]
macro_rules! register_native_fn {
    ($interpreter: expr, $fn: ident) => {
        $interpreter.register_native_function(stringify!($fn), $fn);
    };
    ($interpreter: expr, $fn: ident, $($rest: ident),+) => {
        register_native_fn!($interpreter, $fn);
        register_native_fn!($interpreter, $($rest),*);
    }
}

/// Macro to define a native function with automatic `&[Value]` parameter extraction
///
/// The macro creates a function that accepts `&[Value]` and internally calls the
/// user-defined function with extracted parameters.
#[macro_export]
macro_rules! wrap_native_fn {
    // Function with return type and body
    ($fn_name:ident($($param_name:ident : $param_ty:ty),* $(,)?) -> $ret_ty:ty $body:block) => {
        fn $fn_name(args: &[$crate::interpreter::mir_interpreter::Value]) -> $ret_ty {
            fn _inner($($param_name: $param_ty),*) -> $ret_ty {
                $body
            }

            let mut idx = 0;
            $(
                let $param_name: $param_ty = {
                    use $crate::interpreter::native_functions::ExtractValue;
                    let val = args.get(idx).unwrap_or(&$crate::interpreter::mir_interpreter::Value::Unit);
                    #[allow(unused_assignments)] { idx += 1; }
                    ExtractValue::extract(val)
                };
            )*

            _inner($($param_name),*)
        }
    };

    // Function without return type (returns Unit) with body
    ($fn_name:ident($($param_name:ident : $param_ty:ty),* $(,)?) $body:block) => {
        fn $fn_name(args: &[$crate::interpreter::mir_interpreter::Value]) {
            fn _inner($($param_name: $param_ty),*) {
                $body
            }

            let mut idx = 0;
            $(
                let $param_name: $param_ty = {
                    use $crate::interpreter::native_functions::ExtractValue;
                    let val = args.get(idx).unwrap_or(&$crate::interpreter::mir_interpreter::Value::Unit);
                    #[allow(unused_assignments)] { idx += 1; }
                    ExtractValue::extract(val)
                };
            )*

            _inner($($param_name),*)
        }
    };
}

/// Helper trait for extracting values from `Value` enum
pub trait ExtractValue {
    fn extract(value: &MIRValue) -> Self;
}

macro_rules! impl_extract {
    ($type:ty, $variant:ident, $convert:expr, $default:expr) => {
        impl ExtractValue for $type {
            fn extract(value: &MIRValue) -> Self {
                if let MIRValue::$variant(inner) = value {
                    $convert(inner)
                } else {
                    $default
                }
            }
        }
    };
}

impl_extract!(String, String, |s: &String| s.clone(), String::new());

impl_extract!(u8, UnsignedInteger, |i: &u64| *i as u8, 0);
impl_extract!(u16, UnsignedInteger, |i: &u64| *i as u16, 0);
impl_extract!(u32, UnsignedInteger, |i: &u64| *i as u32, 0);
impl_extract!(u64, UnsignedInteger, |i: &u64| *i, 0);

impl_extract!(i8, Integer, |i: &i64| *i as i8, 0);
impl_extract!(i16, Integer, |i: &i64| *i as i16, 0);
impl_extract!(i32, Integer, |i: &i64| *i as i32, 0);
impl_extract!(i64, Integer, |i: &i64| *i, 0);

impl_extract!(f32, Float, |f: &f64| *f as f32, 0.0);
impl_extract!(f64, Float, |f: &f64| *f, 0.0);

impl_extract!(bool, Boolean, |b: &bool| *b, false);
