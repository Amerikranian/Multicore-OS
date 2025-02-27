use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, ItemFn, parse_macro_input};

/// A proc-macro for creating tests in the TAOS operating system.
///
/// This works with both async and non-async functions, automatically
/// wrapping non-async functions in an async block to make them compatible
/// with the test runner.
#[proc_macro_attribute]
pub fn async_test_case(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let name = &input.sig.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let block = &input.block;

    let is_async = input.sig.asyncness.is_some();

    let output = if is_async {
        quote! {
            #(#attrs)*
            #[test_case]
            #vis fn #name() -> impl ::core::future::Future<Output = ()> + Send + 'static {
                async move #block
            }
        }
    } else {
        quote! {
            #(#attrs)*
            #[test_case]
            #vis fn #name() -> impl ::core::future::Future<Output = ()> + Send + 'static {
                async move {
                    #block
                }
            }
        }
    };

    output.into()
}
