use paste::paste;
use std::sync::Arc;

use super::hir_interpreter::{InterpreterState as HIRInterpreterState, Value as HIRValue};
use super::mir_interpreter::{InterpreterState as MIRInterpreterState, Value as MIRValue};

macro_rules! impl_native_fn_interface {
    ($ident: ident, $interpreter_ty: ty, $value_ty: ty) => {
        paste! {
            pub type [<Kitlang $ident NativeFn>] = dyn Fn(&mut $interpreter_ty, &[$value_ty]) -> Option<$value_ty> + Send + Sync;

            pub trait [<Into $ident KitlangFn>] {
                fn into_kitlang_fn(self) -> Arc<[<Kitlang $ident NativeFn>]>;
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
                F: Fn(&mut $interpreter_ty, &[$value_ty]) -> R + Send + Sync + 'static,
                R: [<Into $ident Return>],
            {
                fn into_kitlang_fn(self) -> Arc<[<Kitlang $ident NativeFn>]> {
                    Arc::new(move |state, args| self(state, args).into_kitlang_return())
                }
            }
        }
    };
}

impl_native_fn_interface!(HIR, HIRInterpreterState, HIRValue);
impl_native_fn_interface!(MIR, MIRInterpreterState, MIRValue);
