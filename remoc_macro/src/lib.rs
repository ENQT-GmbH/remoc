//! Procedural macros for ReMOC.

#![deny(unsafe_code)]

use quote::quote;
use syn::parse_macro_input;

mod method;
mod trait_def;
mod util;

use crate::trait_def::TraitDef;

/// Denotes a trait as remotely callable.
#[proc_macro_attribute]
pub fn remote(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let trait_def = parse_macro_input!(input as TraitDef);

    let vanilla_trait = trait_def.vanilla_trait();
    let request_enums = trait_def.request_enums();
    let servers = trait_def.servers();
    let client = trait_def.client();

    proc_macro::TokenStream::from(quote! {
        #vanilla_trait
        #request_enums
        #servers
        #client
    })
}
