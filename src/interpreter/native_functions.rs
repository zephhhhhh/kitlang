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

// pub trait NativeFunctionInterface<Param>: Sized {
//     fn call(&mut self, args: Param) -> Option<MIRValue>;
// }

// impl<F, P1> NativeFunctionInterface<(P1,)> for F
// where
//     F: FnMut(P1) -> Option<MIRValue> + 'static,
// {
//     fn call(&mut self, args: (P1,)) -> Option<MIRValue> {
//         (self)(args.0)
//     }
// }

// impl<F, P1, P2> NativeFunctionInterface<(P1, P2)> for F
// where
//     F: FnMut(P1, P2) -> Option<MIRValue> + 'static,
// {
//     fn call(&mut self, args: (P1, P2)) -> Option<MIRValue> {
//         (self)(args.0, args.1)
//     }
// }

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
