use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{FnArg, Ident, ItemFn, Pat, parse_macro_input};

fn kitlang_path() -> proc_macro2::TokenStream {
    match crate_name("kitlang") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        // Fallback.
        Err(_) => quote!(::kitlang),
    }
}

/// Attribute macro that wraps a strongly-typed function so it can be called from the
/// Kitlang interpreter as `fn(&[Value]) -> ReturnType`.
///
/// ## Usage
///
/// ```rust
/// use kitlang_macros::kitlang_native_fn;
///
/// #[kitlang_native_fn]
/// fn add(a: i64, b: i64) -> i64 {
///     a + b
/// }
/// ```
///
/// Notes:
/// - Destructuring is not supported.
/// - `self` receivers are not supported.
/// - Generics are not supported by this helper (it will emit a compile-time error).
#[proc_macro_attribute]
pub fn kitlang_native_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    let kitlang_crate = kitlang_path();

    // Reject unsupported cases early.
    if !input_fn.sig.generics.params.is_empty() {
        return quote! { compile_error!("kitlang_native_fn does not support generics"); }.into();
    }

    let mut param_idents = Vec::new();
    let mut param_types = Vec::new();

    for arg in &input_fn.sig.inputs {
        match arg {
            FnArg::Typed(pat_ty) => {
                if let Pat::Ident(pat_ident) = pat_ty.pat.as_ref() {
                    param_idents.push(pat_ident);
                    param_types.push(&pat_ty.ty);
                } else {
                    return quote! {
                        compile_error!("kitlang_native_fn parameters must be simple identifiers");
                    }
                    .into();
                }
            }
            FnArg::Receiver(_) => {
                return quote! { compile_error!("kitlang_native_fn does not support methods or self parameters"); }.into();
            }
        }
    }

    let fn_ident = &input_fn.sig.ident;
    let inner_ident = format_ident!("__kitlang_inner_{}", fn_ident);
    let fn_vis = &input_fn.vis;
    let fn_output = &input_fn.sig.output;

    // Preserve non-macro attributes (skip the wrap_native_fn attribute itself which is already consumed).
    let fn_attrs: Vec<_> = input_fn
        .attrs
        .into_iter()
        .filter(|a| !a.path().is_ident("kitlang_native_fn"))
        .collect();

    let fn_block = &input_fn.block;

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis fn #fn_ident(args: &[#kitlang_crate::interpreter::mir_interpreter::Value]) #fn_output {
            // Inner function retains the original, strongly-typed signature.
            fn #inner_ident(#(#param_idents: #param_types),*) #fn_output {
                #fn_block
            }

            let mut __kitlang_idx: usize = 0;
            #(
                let #param_idents: #param_types = {
                    use #kitlang_crate::interpreter::native_functions::ExtractValue;
                    let __kitlang_val = args.get(__kitlang_idx)
                        .unwrap_or(&#kitlang_crate::interpreter::mir_interpreter::Value::Unit);
                    #[allow(unused_assignments)] { __kitlang_idx += 1; }
                    ExtractValue::extract(__kitlang_val)
                };
            )*

            #inner_ident(#(#param_idents),*)
        }
    };

    TokenStream::from(expanded)
}
