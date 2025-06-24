#![no_std]

extern crate alloc;

use alloc::{format, string::ToString};
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// A macro to make a function an mock/integration testing function
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
    let tuple_ident = syn::Ident::new(&format!("__test_tuple_{fn_name}"), fn_name.span());

    // Output: original function + tuple
    let output = quote! {
        #input

        #[test_case]
        #[allow(non_upper_case_globals)]
        static #tuple_ident: (fn(), &'static str) = (#fn_name, #fn_name_str);
    };

    output.into()
}

/// Make a function an ISR. This macro creates a stub (called `__isr_stub_isr` where `isr` is the name of
/// the function) that calls the macro tagged with that attribute
///
/// NOTE: When registering the ISR within the IDT use `__isr_stub_isr` and NOT `isr`. The ISR stub
/// will call the actual ISR.
#[cfg(target_arch = "x86_64")]
#[proc_macro_attribute]
pub fn isr(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input function
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_args = &input_fn.sig.inputs;
    let fn_body = &input_fn.block;

    // Generate the wrapper function name
    let wrapper_name = syn::Ident::new(&format!("__isr_stub_{fn_name}"), fn_name.span());

    // Generate the macro output
    let expanded = quote! {
        // The original ISR function (renamed internally)
        #fn_vis fn #fn_name(#fn_args) {
            #fn_body
        }

        // The naked wrapper function with ISR assembly
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        #fn_vis unsafe extern "C" fn #wrapper_name() {
            unsafe {
                core::arch::naked_asm!(
                    // Call the actual ISR
                    "call {}",
                    // Return from interrupt
                    "iretq",
                    sym #fn_name,
                );
            }
        }
    };

    TokenStream::from(expanded)
}
