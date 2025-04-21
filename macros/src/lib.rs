use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn test_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input function
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();

    // Validate: no arguments, returns ()
    if !input.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            input.sig.inputs,
            "test_case functions must have no arguments",
        )
        .to_compile_error()
        .into();
    }
    match input.sig.output {
        syn::ReturnType::Default => {}
        syn::ReturnType::Type(_, _) => {
            return syn::Error::new_spanned(
                input.sig.output,
                "test_case functions must not return a value",
            )
            .to_compile_error()
            .into();
        }
    }

    // Generate a unique static for the tuple
    let tuple_ident = syn::Ident::new(&format!("__test_tuple_{}", fn_name), fn_name.span());

    // Output: original function + tuple
    let output = quote! {
        #input

        #[test_case]
        #[allow(non_upper_case_globals)]
        static #tuple_ident: (fn(), &'static str) = (#fn_name, #fn_name_str);
    };

    output.into()
}
