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

/// Helper trait for extracting underlying values from a [`MIRValue`].
pub trait ExtractValue {
    fn extract(value: &MIRValue) -> Self;
}

macro_rules! impl_extract {
    ($type:ty, $variant:ident, $convert:expr, $default:expr) => {
        #[allow(clippy::cast_possible_truncation)]
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

macro_rules! impl_extract_tuple {
    ($($t:ident : $idx:tt),+) => {
        impl<$($t: ExtractValue),+> ExtractValue for ($($t,)+) {
            fn extract(value: &MIRValue) -> Self {
                if let MIRValue::Tuple(elements) = value {
                    ($(
                        $t::extract(elements.get($idx).unwrap_or(&MIRValue::Unit)),
                    )+)
                } else {
                    ($(
                        $t::extract(&MIRValue::Unit),
                    )+)
                }
            }
        }
    };
}

impl_extract_tuple!(T1: 0);
impl_extract_tuple!(T1: 0, T2: 1);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2, T4: 3);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5, T7: 6);
impl_extract_tuple!(T1: 0, T2: 1, T3: 2, T4: 3, T5: 4, T6: 5, T7: 6, T8: 7);
