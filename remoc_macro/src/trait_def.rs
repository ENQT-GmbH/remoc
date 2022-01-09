//! Trait parsing and client and server generation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, TokenStreamExt};
use syn::{
    braced,
    parse::{Parse, ParseStream},
    Attribute, GenericParam, Generics, Ident, Lifetime, LifetimeDef, Token, TypeParam, Visibility, WhereClause,
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
    /// Trait visibility.
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
        let mut generics = input.parse::<Generics>()?;
        if generics.params.iter().any(|p| matches!(p, GenericParam::Type(tp) if tp.ident == "Target")) {
            return Err(input.error("remote trait must not be generic over type parameter Target"));
        }
        if generics.lifetimes().count() > 0 {
            return Err(input.error("lifetimes are not allowed on remote traits"));
        }

        // Generics where clause.
        if let Some(where_clause) = input.parse::<Option<WhereClause>>()? {
            generics.make_where_clause().predicates.extend(where_clause.predicates);
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
        let where_clause = &generics.where_clause;
        let attrs = attribute_tokens(attrs);

        // Trait methods.
        let mut defs = quote! {};
        for m in &self.methods {
            defs.append_all(m.trait_method());
        }

        quote! {
            #attrs
            #[::remoc::rtc::async_trait]
            #vis trait #ident #generics #where_clause {
                #defs
            }
        }
    }

    /// Generics for request enum, client type, server type and server trait implementation.
    ///
    /// First return item is server type generics, including Target, Codec and possibly lifetime of target.
    /// Second return itm is server implementation generics, including where-clauses on Target and Codec.
    fn generics(
        &self, with_target: bool, with_codec: bool, with_codec_default: bool, with_lifetime: bool,
        with_send_sync_static: bool,
    ) -> (Generics, Generics) {
        let ident = &self.ident;

        let trait_generics = self.generics.clone();

        let mut ty_generics = self.generics.clone();
        let idx = ty_generics
            .params
            .iter()
            .enumerate()
            .find_map(|(idx, p)| match p {
                GenericParam::Const(_) => Some(idx),
                _ => None,
            })
            .unwrap_or_else(|| ty_generics.params.len());
        if with_codec {
            let codec_param: TypeParam = syn::parse2(if with_codec_default {
                quote! { Codec = ::remoc::codec::Default }
            } else {
                quote! { Codec }
            })
            .unwrap();
            ty_generics.params.insert(idx, GenericParam::Type(codec_param));
        }
        if with_target {
            ty_generics.params.insert(idx, GenericParam::Type(format_ident!("Target").into()));
        }

        if with_lifetime {
            let target_lt: Lifetime = syn::parse2(quote! {'target}).unwrap();
            ty_generics.params.insert(0, LifetimeDef::new(target_lt).into());
        }

        let mut impl_generics = ty_generics.clone();

        if with_codec {
            let wc: WhereClause = syn::parse2(quote! { where Codec: ::remoc::codec::Codec }).unwrap();
            impl_generics.make_where_clause().predicates.extend(wc.predicates);
        }

        if with_target {
            let wc: WhereClause = syn::parse2(quote! { where Target: #ident #trait_generics }).unwrap();
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

        let (trait_generics, _) = self.generics(false, false, false, false, false);
        let (ty_generics, impl_generics) = self.generics(false, true, false, false, false);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();
        let ty_generics_list = &ty_generics.params;

        let impl_generics_where_pred = &impl_generics_where.unwrap().predicates;
        let impl_generics_where_str = quote! { #impl_generics_where_pred }.to_string();

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
            #[derive(::remoc::rtc::Serialize, ::remoc::rtc::Deserialize)]
            #[serde(bound(serialize = #impl_generics_where_str))]
            #[serde(bound(deserialize = #impl_generics_where_str))]
            enum #req_value #ty_generics #ty_generics_where {
                #value_entries
                __Phantom (::std::marker::PhantomData<(#ty_generics_list)>)
            }

            impl #impl_generics_impl #req_value #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: Target) where Target: #ident #trait_generics {
                    match self {
                        #value_clauses
                        Self::__Phantom(_) => ()
                    }
                }
            }

            #[derive(::remoc::rtc::Serialize, ::remoc::rtc::Deserialize)]
            #[serde(bound(serialize = #impl_generics_where_str))]
            #[serde(bound(deserialize = #impl_generics_where_str))]
            enum #req_ref #ty_generics #ty_generics_where {
                #ref_entries
                __Phantom (::std::marker::PhantomData<(#ty_generics_list)>)
            }

            impl #impl_generics_impl #req_ref #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: &Target) where Target: #ident #trait_generics {
                    match self {
                        #ref_clauses
                        Self::__Phantom(_) => ()
                    }
                }
            }

            #[derive(::remoc::rtc::Serialize, ::remoc::rtc::Deserialize)]
            #[serde(bound(serialize = #impl_generics_where_str))]
            #[serde(bound(deserialize = #impl_generics_where_str))]
            enum #req_ref_mut #ty_generics #ty_generics_where {
                #ref_mut_entries
                __Phantom (::std::marker::PhantomData<(#ty_generics_list)>)
            }

            impl #impl_generics_impl #req_ref_mut #impl_generics_ty #impl_generics_where {
                async fn dispatch<Target>(self, target: &mut Target) where Target: #ident #trait_generics {
                    match self {
                        #ref_mut_clauses
                        Self::__Phantom(_) => ()
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by value.
    fn server_value(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (ty_generics, impl_generics) = self.generics(true, true, true, false, false);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}Server", &ident);

        let doc = format!("Server for [{}] taking the target object by value.", &ident);

        quote! {
            #[doc=#doc]
            #vis struct #server #ty_generics #ty_generics_where {
                target: Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #req_generics,
                        #req_ref #req_generics,
                        #req_ref_mut #req_generics,
                    >,
                    Codec,
                >,
            }

            impl #impl_generics_impl ::remoc::rtc::ServerBase for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #req_generics;
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::Server <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                fn new(target: Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client::new(req_tx))
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
                            Err(err) if err.is_final() => return Some(target),
                            Err(err) => ::remoc::rtc::receiving_request_failed(err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by reference.
    fn server_ref(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (ty_generics, impl_generics) = self.generics(true, true, true, true, false);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerRef", &ident);

        let doc = format!("Server for [{}] taking the target object by reference.", &ident);

        quote! {
            #[doc=#doc]
            #vis struct #server #ty_generics #ty_generics_where {
                target: &'target Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #req_generics,
                        #req_ref #req_generics,
                        #req_ref_mut #req_generics,
                    >,
                    Codec,
                >,
            }

            impl #impl_generics_impl ::remoc::rtc::ServerBase for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #req_generics;
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerRef <'target, Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                fn new(target: &'target Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client::new(req_tx))
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
                            Err(err) if err.is_final() => break,
                            Err(err) => ::remoc::rtc::receiving_request_failed(err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by mutable reference.
    fn server_ref_mut(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (ty_generics, impl_generics) = self.generics(true, true, true, true, false);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerRefMut", &ident);

        let doc = format!("Server for [{}] taking the target object by mutable reference.", &ident);

        quote! {
            #[doc=#doc]
            #vis struct #server #ty_generics #ty_generics_where {
                target: &'target mut Target,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #req_generics,
                        #req_ref #req_generics,
                        #req_ref_mut #req_generics,
                    >,
                    Codec,
                >,
            }

            impl #impl_generics_impl ::remoc::rtc::ServerBase for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #req_generics;
            }

            #[::remoc::rtc::async_trait(?Send)]
            impl #impl_generics_impl ::remoc::rtc::ServerRefMut <'target, Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client::new(req_tx))
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
                            Err(err) if err.is_final() => break,
                            Err(err) => ::remoc::rtc::receiving_request_failed(err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by shared reference.
    fn server_shared(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (ty_generics, impl_generics) = self.generics(true, true, true, false, true);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerShared", &ident);

        let doc = format!("Server for [{}] taking the target object by shared reference.", &ident);

        quote! {
            #[doc=#doc]
            #vis struct #server #ty_generics #ty_generics_where {
                target: ::std::sync::Arc<Target>,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #req_generics,
                        #req_ref #req_generics,
                        #req_ref_mut #req_generics,
                    >,
                    Codec,
                >,
            }

            impl #impl_generics_impl ::remoc::rtc::ServerBase for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #req_generics;
            }

            #[::remoc::rtc::async_trait]
            impl #impl_generics_impl ::remoc::rtc::ServerShared <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                fn new(target: ::std::sync::Arc<Target>, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client::new(req_tx))
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
                            Err(err) if err.is_final() => break,
                            Err(err) => ::remoc::rtc::receiving_request_failed(err),
                        }
                    }
                }
            }
        }
    }

    /// Server struct and implementation taking target by shared mutable reference.
    fn server_shared_mut(&self) -> TokenStream {
        let Self { vis, ident, .. } = self;

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (ty_generics, impl_generics) = self.generics(true, true, true, false, true);
        let ty_generics_where = &ty_generics.where_clause;
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let client = self.client_ident();
        let server = format_ident!("{}ServerSharedMut", &ident);

        let doc = format!("Server for [{}] taking the target object by shared mutable reference.", &ident);

        quote! {
            #[doc=#doc]
            #vis struct #server #ty_generics #ty_generics_where {
                target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>,
                req_rx: ::remoc::rch::mpsc::Receiver<
                    ::remoc::rtc::Req<
                        #req_value #req_generics,
                        #req_ref #req_generics,
                        #req_ref_mut #req_generics,
                    >,
                    Codec,
                >,
            }

            impl #impl_generics_impl ::remoc::rtc::ServerBase for #server #impl_generics_ty #impl_generics_where
            {
                type Client = #client #req_generics;
            }

            #[::remoc::rtc::async_trait]
            impl #impl_generics_impl ::remoc::rtc::ServerSharedMut <Target, Codec> for #server #impl_generics_ty #impl_generics_where
            {
                fn new(target: ::std::sync::Arc<::remoc::rtc::LocalRwLock<Target>>, request_buffer: usize) -> (Self, Self::Client) {
                    let (req_tx, req_rx) = ::remoc::rch::mpsc::channel(request_buffer);
                    (Self { target, req_rx }, Self::Client::new(req_tx))
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
                            Err(err) if err.is_final() => break,
                            Err(err) => ::remoc::rtc::receiving_request_failed(err),
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
        let client_ident_str = client_ident.to_string();

        let (ty_generics, impl_generics) = self.generics(false, true, true, false, false);
        let ty_generics_where_ty = &ty_generics.where_clause;
        let (ty_generics_impl, ty_generics_ty, ty_generics_where) = ty_generics.split_for_impl();
        let (impl_generics_impl, impl_generics_ty, impl_generics_where) = impl_generics.split_for_impl();

        let (req_generics, _) = self.generics(false, true, false, false, false);
        let (req_value, req_ref, req_ref_mut) = self.request_enum_idents();

        let impl_generics_where_pred = &impl_generics_where.unwrap().predicates;
        let impl_generics_where_str = quote! { #impl_generics_where_pred }.to_string();

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

        let doc = format!("Remote client for [{}].\n\nCan be sent to a remote endpoint.", &ident);

        quote! {
            #[doc=#doc]
            #[derive(::remoc::rtc::Serialize, ::remoc::rtc::Deserialize)]
            #[serde(bound(serialize = #impl_generics_where_str))]
            #[serde(bound(deserialize = #impl_generics_where_str))]
            #clone
            #attrs
            #vis struct #client_ident #ty_generics #ty_generics_where_ty {
                req_tx: ::remoc::rch::mpsc::Sender<
                    ::remoc::rtc::Req<#req_value #req_generics, #req_ref #req_generics, #req_ref_mut #req_generics>,
                    Codec,
                >,
                #[serde(skip)]
                #[serde(default = "::remoc::rtc::empty_client_drop_tx")]
                drop_tx: ::remoc::rtc::local_broadcast::Sender<()>,
            }

            impl #impl_generics_impl #client_ident #impl_generics_ty #impl_generics_where {
                fn new(req_tx: ::remoc::rch::mpsc::Sender<
                    ::remoc::rtc::Req<#req_value #req_generics, #req_ref #req_generics, #req_ref_mut #req_generics>,
                    Codec,
                >) -> Self
                {
                    Self {
                        req_tx,
                        drop_tx: ::remoc::rtc::empty_client_drop_tx(),
                    }
                }
            }

            impl #impl_generics_impl ::remoc::rtc::Client for #client_ident #impl_generics_ty #impl_generics_where {
                fn capacity(&self) -> usize {
                    self.req_tx.capacity()
                }

                fn closed(&self) -> ::remoc::rtc::Closed {
                    let req_tx = self.req_tx.clone();
                    let mut drop_rx = self.drop_tx.subscribe();
                    ::remoc::rtc::Closed::new(async move {
                        ::remoc::rtc::select! {
                            () = req_tx.closed() => (),
                            _ = drop_rx.recv() => (),
                        }
                    })
                }

                fn is_closed(&self) -> bool {
                    self.req_tx.is_closed()
                }
            }

            #[::remoc::rtc::async_trait]
            impl #impl_generics_impl #ident #generics for #client_ident #impl_generics_ty #impl_generics_where {
                #methods
            }

            impl #ty_generics_impl ::std::fmt::Debug for #client_ident #ty_generics_ty #ty_generics_where {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    write!(f, #client_ident_str)
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
