//! Method parsing and generation.

use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token,
    token::Comma,
    Attribute, Block, FnArg, Generics, Ident, Pat, PatType, ReturnType, Stmt, Token, Type,
};

use crate::util::{attribute_tokens, to_pascal_case};

/// Self reference of method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfRef {
    /// self
    Value,
    /// &self
    Ref,
    /// &mut self
    RefMut,
}

/// A named argument.
#[derive(Debug)]
pub struct NamedArg {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Name.
    pub ident: Ident,
    /// Type.
    pub ty: Type,
}

impl NamedArg {
    /// Create a `NamedArg` from a `PatType`.
    fn extract(pat_type: &PatType) -> syn::Result<Self> {
        let ident = if let Pat::Ident(pat_ident) = &*pat_type.pat {
            pat_ident.ident.clone()
        } else {
            return Err(syn::Error::new(pat_type.pat.span(), "expected identifier"));
        };
        Ok(Self { attrs: pat_type.attrs.clone(), ident, ty: (*pat_type.ty).clone() })
    }
}

/// A method in a trait.
#[derive(Debug)]
pub struct TraitMethod {
    /// Attributes.
    pub attrs: Vec<Attribute>,
    /// Name.
    pub ident: Ident,
    /// Self reference of method.
    pub self_ref: SelfRef,
    /// Arguments.
    pub args: Vec<NamedArg>,
    /// Return type.
    pub ret_ty: Type,
    /// Whether method should be cancelled, if client sends hangup message.
    pub cancel: bool,
    /// Method body.
    pub body: Option<Vec<Stmt>>,
}

impl Parse for TraitMethod {
    /// Parses a method within the service trait.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse method definition.
        let mut attrs = input.call(Attribute::parse_outer)?;
        input.parse::<Token![async]>()?;
        input.parse::<Token![fn]>()?;
        let ident: Ident = input.parse()?;

        // Check for no_cancel attribute.
        let mut cancel = true;
        attrs.retain(|attr| {
            if let Some(attr) = attr.path().get_ident() {
                if *attr == "no_cancel" {
                    cancel = false;
                    return false;
                }
            }
            true
        });

        // Parse generics.
        let generics = input.parse::<Generics>()?;
        if generics.lt_token.is_some() {
            return Err(input.error("generics and lifetimes are not allowed on remote trait methods"));
        }

        // Parse arguments.
        let content;
        parenthesized!(content in input);
        let raw_args: Punctuated<FnArg, Comma> = content.parse_terminated(FnArg::parse, Token![,])?;

        // Extract receiver and arguments.
        let mut self_ref = None;
        let mut args = Vec::new();
        for arg in raw_args {
            match arg {
                // self, &self or &mut self receiver
                FnArg::Receiver(recv) => {
                    self_ref = Some(if recv.reference.is_some() {
                        if recv.mutability.is_some() {
                            SelfRef::RefMut
                        } else {
                            SelfRef::Ref
                        }
                    } else {
                        SelfRef::Value
                    });
                }
                // other argument
                FnArg::Typed(pat_type) => {
                    let arg = NamedArg::extract(&pat_type)?;
                    args.push(arg);
                }
            }
        }
        let self_ref =
            self_ref.ok_or_else(|| input.error("associated functions are not allowed in remote traits"))?;

        // Parse return type.
        let ret: ReturnType = input.parse()?;
        let ret_ty = match ret {
            ReturnType::Type(_, ty) => *ty,
            ReturnType::Default => return Err(input.error("all methods must return a Result type")),
        };

        // Parse default body.
        let body = if input.peek(token::Brace) {
            let content;
            braced!(content in input);
            Some(content.call(Block::parse_within)?)
        } else {
            input.parse::<Token![;]>()?;
            None
        };

        Ok(Self { attrs, ident, self_ref, args, ret_ty, cancel, body })
    }
}

impl TraitMethod {
    /// Method definition within trait (without argument attributes).
    pub fn trait_method(&self) -> TokenStream {
        let Self { attrs, ident, ret_ty, .. } = self;
        let attrs = attribute_tokens(attrs);

        // Build argument list.
        let mut args = quote! {};

        // Self argument.
        let self_ref = match self.self_ref {
            SelfRef::Value => quote! {self,},
            SelfRef::Ref => quote! {&self,},
            SelfRef::RefMut => quote! {&mut self,},
        };
        args.append_all(self_ref);

        // Request arguments.
        for NamedArg { attrs: _, ident, ty } in &self.args {
            args.append_all(quote! { #ident : #ty , });
        }

        // Body.
        let body_opt = match &self.body {
            Some(stmts) => {
                let mut body = quote! {};
                body.append_all(stmts);
                quote! { { #body } }
            }
            None => quote! { ; },
        };

        quote! {
            #attrs async fn #ident ( #args ) -> #ret_ty
            #body_opt
        }
    }

    /// Entry within request enum.
    pub fn request_enum_entry(&self) -> TokenStream {
        let ident = to_pascal_case(&self.ident);
        let ret_ty = &self.ret_ty;

        let mut entries = quote! {
            #[doc="Reply channel for sending the result of the method invocation.\n\n"]
            #[doc="The channel is closed when the calling async method is cancelled "]
            #[doc="or a connection error occurs."]
            __reply_tx: ::remoc::rch::oneshot::Sender<#ret_ty, Codec>,
        };

        for NamedArg { attrs, ident, ty } in &self.args {
            if !attrs.iter().any(|attr| attr.path().is_ident("doc")) {
                entries.append_all(quote! {
                    #[doc = concat!(stringify!(#ident), " parameter")]
                });
            }

            let attrs = attribute_tokens(attrs);
            entries.append_all(quote! {
                #attrs
                #ident : #ty ,
            });
        }

        let docs_attrs = attribute_tokens(
            &self
                .attrs
                .iter()
                .filter(|attr| matches!(attr.path().get_ident(), Some(ident) if *ident == "doc"))
                .cloned()
                .collect::<Vec<_>>(),
        );
        quote! { #docs_attrs #ident {#entries} , }
    }

    /// Conversion clause for `impl From<#from_ty>`.
    pub fn impl_from_clause(&self, from_ty: &Ident) -> TokenStream {
        let ident = &self.ident;
        let enum_ident = to_pascal_case(ident);

        // Build enum argument list.
        let mut entries = quote! { __reply_tx, };
        for NamedArg { ident: arg_ident, .. } in &self.args {
            entries.append_all(quote! { #arg_ident, });
        }

        let entry = quote! { #enum_ident {#entries} };
        quote! { #from_ty :: #entry => Self :: #entry , }
    }

    /// Enum match discriminator and dispatch code.
    pub fn dispatch_discriminator(&self) -> TokenStream {
        let ident = &self.ident;
        let enum_ident = to_pascal_case(ident);

        // Build match and call argument lists.
        let mut entries = quote! { __reply_tx, };
        let mut args = quote! {};
        for NamedArg { ident: arg_ident, .. } in &self.args {
            entries.append_all(quote! { #arg_ident, });
            args.append_all(quote! { #arg_ident, });
        }

        // Generate call code.
        let call = if self.cancel {
            quote! {
                ::remoc::rtc::select! {
                    biased;
                    () = __reply_tx.closed() => (),
                    result = __target.#ident(#args) => {
                        ::remoc::rtc::send_reply(__reply_tx, &__err_tx, result).await;
                    }
                }
            }
        } else {
            quote! {
                let result = __target.#ident(#args).await;
                ::remoc::rtc::send_reply(__reply_tx, &__err_tx, result).await;
            }
        };

        // Generate match clause.
        quote! {
            Self :: #enum_ident { #args __reply_tx } => {
                #call
            },
        }
    }

    /// Client method implementation.
    pub fn client_method(&self, req_value: &Ident, req_ref: &Ident, req_ref_mut: &Ident) -> TokenStream {
        let Self { ident, self_ref, ret_ty, .. } = self;

        // Self reference and request enum.
        let (self_ref, req_enum, req_type) = match self_ref {
            SelfRef::Value => (quote! { self }, req_value, quote! { Value }),
            SelfRef::Ref => (quote! { &self }, req_ref, quote! { Ref }),
            SelfRef::RefMut => (quote! { &mut self }, req_ref_mut, quote! { RefMut }),
        };
        let req_case = to_pascal_case(ident);

        // Argument and request enum entry list.
        let mut args = quote! {};
        let mut entries = quote! {};
        for NamedArg { ident, ty, .. } in &self.args {
            args.append_all(quote! { #ident : #ty , });
            entries.append_all(quote! { #ident , });
        }

        quote! {
            async fn #ident (#self_ref, #args) -> #ret_ty {
                let (mut reply_tx, reply_rx) = ::remoc::rch::oneshot::channel();
                reply_tx.set_max_item_size(self.max_reply_size);
                let req_value = #req_enum :: #req_case { __reply_tx: reply_tx, #entries };
                let req = ::remoc::rtc::Req::#req_type(req_value);
                self.req_tx.send(req).await.map_err(::remoc::rtc::CallError::from)?;
                let reply = reply_rx.await.map_err(::remoc::rtc::CallError::from)?;
                reply
            }
        }
    }
}
