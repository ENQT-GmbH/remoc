//! Utility functions.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{Attribute, Ident};

/// Converts the identifier to pascal case.
pub fn to_pascal_case(ident: &Ident) -> Ident {
    let s = ident.to_string();
    let mut capital = true;
    let mut out = String::new();
    for c in s.chars() {
        if c == '_' {
            capital = true;
        } else if capital {
            out.push(c.to_uppercase().next().unwrap());
            capital = false;
        } else {
            out.push(c);
        }
    }
    format_ident!("{}", out)
}

/// TokenStream for list of attributes.
pub fn attribute_tokens(attrs: &[Attribute]) -> TokenStream {
    let mut tokens = quote! {};
    for attr in attrs {
        attr.to_tokens(&mut tokens);
    }
    tokens
}
