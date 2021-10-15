//! Trait parsing and client and server generation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, TokenStreamExt};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    Attribute, GenericParam, Generics, Ident, Lifetime, LifetimeDef, Token, Visibility, WhereClause,
};

use crate::{
    method::{SelfRef, TraitMethod},
    util::attribute_tokens,
};

/// Trait definition.
#[derive(Debug)]
pub struct TraitDef {
    /// Trait attributes.
    attrs: Vec<Attribute>,
    /// Trait visibily.
    vis: Visibility,
    /// Name.
    ident: Ident,
    /// Generics.
    /// Contains type parameter `Codec`.
    generics: Generics,
    /// Methods.
    methods: Vec<TraitMethod>,
}

impl Parse for TraitDef {
    /// Parses a service trait.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse trait definition.
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![trait]>()?;
        let ident: Ident = input.parse()?;

        // Parse generics.
        let generics = input.parse::<Generics>()?;
        if !generics.params.iter().any(|p| matches!(p, GenericParam::Type(tp) if tp.ident == "Codec")) {
            return Err(input.error("remote trait must be generic over type parameter Codec"));
        }
        if generics.params.iter().any(|p| matches!(p, GenericParam::Type(tp) if tp.ident == "Target")) {
            return Err(input.error("remote trait must not be generic over type parameter Target"));
        }

        // Extract content of trait definition.
        let content;
        braced!(content in input);

        // Parse service method definitions.
        let mut methods: Vec<TraitMethod> = Vec::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
        }

        Ok(Self { attrs, vis, ident, generics, methods })
    }
}

impl TraitDef {
    /// True, if any trait method takes self by value.
    fn is_taking_value(&self) -> bool {
        self.methods.iter().any(|m| m.self_ref == SelfRef::Value)
    }

    /// True, if any trait method takes self by mutable reference.
    fn is_taking_ref_mut(&self) -> bool {
        self.methods.iter().any(|m| m.self_ref == SelfRef::RefMut)
    }

    /// Identifier of the client type.
    fn client_ident(&self) -> Ident {
        format_ident!("{}Client", &self.ident)
    }

    /// Vanilla trait definition, without remote-specific attributes.
    pub fn vanilla_trait(&self) -> TokenStream {
        let Self { vis, ident, attrs, generics, .. } = self;
        let attrs = attribute_tokens(attrs);

        // Trait methods.
        let mut defs = quote! {};
        for m in &self.methods {
            defs.append_all(m.trait_method());
        }

        quote! {
            #attrs
            #[::remoc::rtc::async_trait]
            #vis trait #ident #generics {
                #defs
            }
        }
    }

    /// Generics for request enum, client type, server type and server trait implementation.
    ///
    /// First return item is server type generics, including Target, Codec and possibly lifetime of target.
    /// Second return itm is server implementation generics, including where-clauses on Target and Codec.
    fn generics(
        &self, with_target: bool, with_lifetime: bool, with_send_sync_static: bool,
    ) -> (Generics, Generics) {
        let ident = &self.ident;

        let mut ty_generics = self.generics.clone();
        let idx = ty_generics
            .params
            .iter()
            .enumerate()
            .find_map(|(idx, p)| match p {
                GenericParam::Type(tp) if tp.ident == "Codec" => Some(idx),
                _ => None,
            })
            .unwrap();
        if with_target {
            ty_generics.params.insert(idx, GenericParam::Type(format_ident!("Target").into()));
        }

        if with_lifetime {
            let target_lt: Lifetime = syn::parse2(quote! {'target}).unwrap();
            ty_generics.params.insert(0, LifetimeDef::new(target_lt).into());
        }

        let mut impl_generics = ty_generics.clone();
        let wc: WhereClause = syn::parse2(quote! { where Codec: ::remoc::codec::Codec }).unwrap();
        impl_generics.make_where_clause().predicates.extend(wc.predicates);

        if with_target {
            let wc: WhereClause = syn::parse2(quote! { where Target: #ident }).unwrap();
            impl_generics.make_where_clause().predicates.extend(wc.predicates);
        }

        if with_send_sync_static {
            let wc: WhereClause =
                syn::parse2(quote! { where Target: ::std::marker::Send + ::std::marker::Sync + 'static })
                    .unwrap();
            impl_generics.make_where_clause().predicates.extend(wc.predicates);
        }

        (ty_generics, impl_generics)
    }

    /// Identifier of request enums for by-value, by-reference and by-mutable-reference requests.
    fn request_enum_idents(&self) -> (Ident, Ident, Ident) {
        (
            format_ident!("{}ReqValue", &self.ident),
            format_ident!("{}ReqRef", &self.ident),
            format_ident!("{}ReqRefMut", &self.ident),
        )
    }

    /// Requests enums with dispatch functions.
    pub fn request_enums(&self) -> TokenStream {
        let ident = &self.ident;

        let (ty_generics, impl_generics) = self.generics(false, false, false);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let (mut value_entries, mut ref_entries, mut ref_mut_entries) = (quote! {}, quote! {}, quote! {});
        let (mut value_clauses, mut ref_clauses, mut ref_mut_clauses) = (quote! {}, quote! {}, quote! {});
        for md in &self.methods {
            match md.self_ref {
                SelfRef::Value => {
                    value_entries.append_all(md.request_enum_entry());
                    value_clauses.append_all(md.dispatch_discriminator());
                }
                SelfRef::Ref => {
                    ref_entries.append_all(md.request_enum_entry());
                    ref_clauses.append_all(md.dispatch_discriminator());
                }
                SelfRef::RefMut => {
                    ref_mut_entries.append_all(md.request_enum_entry());
                    ref_mut_clauses.append_all(md.dispatch_discriminator());
                }
            }
        }

        quote! {
            #[derive(::serde::Serialize, ::serde::Deserialize)]
            enum #req_value #ty_generics {
                #value_entries
            }

            impl #impl_generics_impl #req_value #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: Target) where Target: #ident {
                    match req {
                        #value_clauses
                    }
                }
            }

            #[derive(::serde::Serialize, ::serde::Deserialize)]
            enum #req_ref #ty_generics {
                #ref_entries
            }

            impl #impl_generics_impl #req_ref #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: &Target) where Target: #ident {
                    match self {
                        #ref_clauses
                    }
                }
            }

            #[derive(::serde::Serialize, ::serde::Deserialize)]
            enum #req_ref_mut #ty_generics {
                #ref_mut_entries
            }

            impl #impl_generics_impl #req_ref_mut #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: &mut Target) where Target: #ident {
                    match self {
                        #ref_mut_clauses
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by value.
    fn server_value(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let trait_generics = &self.generics;
        let (ty_generics, impl_generics) = self.generics(true, false, false);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}Server", &ident);

        quote! {
            #[doc="Remote server for [#ident] taking the target object by value."]
            #vis struct #server #ty_generics {
                target: Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #trait_generics,
                        #req_ref #trait_generics,
                        #req_ref_mut #trait_generics,
                    >,
                    Codec,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::Server <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #trait_generics;

                fn new(target: Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client { req_tx })
                }

                async fn serve(self) -> Option<Target> {
                    let Self { mut target, mut req_rx } = self;

                    loop {
                        match req_rx.recv().await {
                            Ok(Some(::remoc::rtc::Req::Value(req))) => {
                                req.dispatch(target).await;
                                return None;
                            },
                            Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                                req.dispatch(&target).await;
                            },
                            Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                                req.dispatch(&mut target).await;
                            },
                            Ok(None) => return Some(target),
                            Err(err) => ::remoc::rtc::log::trace!("Receiving request failed: {}", &err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by reference.
    fn server_ref(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let trait_generics = &self.generics;
        let (ty_generics, impl_generics) = self.generics(true, true, false);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (_, req_ref, _) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerRef", &ident);

        quote! {
            #[doc="Remote server for [#ident] taking the target object by reference."]
            #vis struct #server #ty_generics {
                target: &'target Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<(), #req_ref #trait_generics, ()>,
                    Codec,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerRef <'target, Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #trait_generics;

                fn new(target: &'target Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client { req_tx })
                }

                async fn serve(self) {
                    let Self { target, mut req_rx } = self;

                    loop {
                        match req_rx.recv().await {
                            Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                                req.dispatch(target).await;
                            },
                            Ok(Some(_)) => (),
                            Ok(None) => break,
                            Err(err) => ::remoc::rtc::log::trace!("Receiving request failed: {}", &err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by mutable reference.
    fn server_ref_mut(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let trait_generics = &self.generics;
        let (ty_generics, impl_generics) = self.generics(true, true, false);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (_, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerRefMut", &ident);

        quote! {
            #[doc="Remote server for [#ident] taking the target object by mutable reference."]
            #vis struct #server #ty_generics {
                target: &'target mut Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<(), #req_ref #trait_generics, #req_ref_mut #trait_generics>,
                    Codec,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerRefMut <'target, Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #trait_generics;

                fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client { req_tx })
                }

                async fn serve(self) {
                    let Self { target, mut req_rx } = self;

                    loop {
                        match req_rx.recv().await {
                            Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                                req.dispatch(target).await;
                            },
                            Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                                req.dispatch(target).await;
                            },
                            Ok(Some(_)) => (),
                            Ok(None) => break,
                            Err(err) => ::remoc::rtc::log::trace!("Receiving request failed: {}", &err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by shared reference.
    fn server_shared(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let trait_generics = &self.generics;
        let (ty_generics, impl_generics) = self.generics(true, true, true);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (_, req_ref, _) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerShared", &ident);

        quote! {
            #[doc="Remote server for [#ident] taking the target object by shared reference."]
            #vis struct #server #ty_generics {
                target: ::std::sync::Arc<Target>,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<(), #req_ref #trait_generics, ()>,
                    Codec,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerShared <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #trait_generics;

                fn new(target: ::std::sync::Arc<Target>, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client { req_tx })
                }

                async fn serve(self, spawn: bool) {
                    let Self { target, mut req_rx } = self;

                    loop {
                        match req_rx.recv().await {
                            Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                                if spawn {
                                    let target = target.clone();
                                    ::remoc::rtc::spawn(async move {
                                        req.dispatch(&*target).await;
                                    });
                                } else {
                                    req.dispatch(&*target).await;
                                }
                            },
                            Ok(Some(_)) => (),
                            Ok(None) => break,
                            Err(err) => ::remoc::rtc::log::trace!("Receiving request failed: {}", &err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by shared mutable reference.
    fn server_shared_mut(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let trait_generics = &self.generics;
        let (ty_generics, impl_generics) = self.generics(true, true, true);
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (_, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerSharedMut", &ident);

        quote! {
            #[doc="Remote server for [#ident] taking the target object by shared mutable reference."]
            #vis struct #server #ty_generics {
                target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<(), #req_ref #trait_generics, #req_ref_mut #trait_generics>,
                    Codec, 1,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerShared <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #trait_generics;

                fn new(target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client { req_tx })
                }

                async fn serve(self, spawn: bool) {
                    let Self { target, mut req_rx } = self;

                    loop {
                        match req_rx.recv().await {
                            Ok(Some(::remoc::rtc::Req::Ref(req))) => {
                                if spawn {
                                    let target = target.clone().read_owned().await;
                                    ::remoc::rtc::spawn(async move {
                                        req.dispatch(&*target).await;
                                    });
                                } else {
                                    let target = target.read().await;
                                    req.dispatch(&*target).await;
                                }
                            },
                            Ok(Some(::remoc::rtc::Req::RefMut(req))) => {
                                let mut target = target.write().await;
                                req.dispatch(&mut *target).await;
                            },
                            Ok(Some(_)) => (),
                            Ok(None) => break,
                            Err(err) => ::remoc::rtc::log::trace!("Receiving request failed: {}", &err),
                        }
                    }
                }
            }
        }
    }

    /// Server types and implementations.
    pub fn servers(&self) -> TokenStream {
        // Always generate server taking value.
        let mut servers = self.server_value();

        // Generate servers taking (mutable, shared) references, if possible.
        if !self.is_taking_value() {
            servers.append_all(self.server_ref_mut());
            servers.append_all(self.server_shared_mut());

            if !self.is_taking_ref_mut() {
                servers.append_all(self.server_ref());
                servers.append_all(self.server_shared());
            }
        }

        servers
    }

    /// The client proxy.
    pub fn client(&self) -> TokenStream {
        let Self { vis, ident, attrs, generics, .. } = self;
        let attrs = attribute_tokens(attrs);
        let client_ident = self.client_ident();

        let (ty_generics, impl_generics) = self.generics(false, false, false);
        let (ty_generics_impl, ty_generics_ty, ty_generics_where) = ty_generics.split_for_impl();
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        // Generate client method implementations.
        let mut methods = quote! {};
        for m in &self.methods {
            methods.append_all(m.client_method(&req_value, &req_ref, &req_ref_mut));
        }

        // Allowing cloning if object is accessed by reference only.
        let clone = if !self.is_taking_ref_mut() && !self.is_taking_value() {
            quote! {#[derive(Clone)]}
        } else {
            quote! {}
        };

        quote! {
            #[derive(::serde::Serialize, ::serde::Deserialize)]
            #clone
            #attrs
            #vis struct #client_ident #ty_generics {
                req_tx: ::remoc::rch::mpsc::Sender<
                    ::remoc::rtc::Req<#req_value, #req_ref, #req_ref_mut>,
                    Codec,
                >,
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl #ident #generics for #client_ident #impl_generics_ty #impl_generics_where {
                #methods
            }

            impl #ty_generics_impl ::std::fmt::Debug for #client_ident #ty_generics_ty #ty_generics_where {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    write!(f, "#client_ident")
                }
            }

            impl #ty_generics_impl ::std::ops::Drop for #client_ident #ty_generics_ty #ty_generics_where {
                fn drop(&mut self) {
                    // required for drop order
                }
            }
        }
    }
}
