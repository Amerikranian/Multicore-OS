use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// A proc-macro for creating async tests in the TAOS operating system.
///
/// This transforms an async function into a test case that returns a
/// Future compatible with the test runner system.
///
/// # Example
///
/// ```
/// #[async_test]
/// async fn my_test() {
///     assert_eq!(1, 1);
/// }
/// ```
#[proc_macro_attribute]
pub fn async_test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the function
    let input = parse_macro_input!(item as ItemFn);

    // Extract function details
    let name = &input.sig.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let block = &input.block;

    // Generate the output function that returns the properly typed future
    let output = quote! {
        #(#attrs)*
        #[test_case]
        #vis fn #name() -> impl ::core::future::Future<Output = ()> + Send + 'static {
            async move #block
        }
    };

    // Return the generated code
    output.into()
}
