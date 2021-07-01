extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_str,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    Attribute, FnArg, GenericArgument, Ident, Pat, PatType, PathArguments, ReturnType, Token, Type, Visibility,
};

/// The unit type.
fn unit_type() -> Type {
    parse_str("()").unwrap()
}

/// Converts the identifier to pascal case.
fn to_pascal_case(ident: &Ident) -> Ident {
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
fn attribute_tokens(attrs: &[Attribute]) -> TokenStream2 {
    let mut tokens = quote! {};
    for attr in attrs {
        attr.to_tokens(&mut tokens);
    }
    tokens
}

/// Service definition.
#[derive(Debug)]
struct Service {
    /// Trait attributes.
    attrs: Vec<Attribute>,
    /// Trait visibily.
    vis: Visibility,
    /// Name.
    ident: Ident,
    /// Methods.
    methods: Vec<ServiceMethod>,
}

impl Parse for Service {
    /// Parses a service trait.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse trait definition.
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![trait]>()?;
        let ident: Ident = input.parse()?;

        // Extract content of trait definition.
        let content;
        braced!(content in input);

        // Parse service method definitions.
        let mut methods: Vec<ServiceMethod> = Vec::new();
        while !content.is_empty() {
            methods.push(content.parse()?);
        }

        Ok(Self { attrs, vis, ident, methods })
    }
}

impl Service {
    /// Identifier of service enum.
    fn service_enum_ident(&self) -> Ident {
        format_ident!("{}Service", &self.ident)
    }

    /// Service enum definition.
    fn service_enum(&self) -> TokenStream2 {
        let Self { vis, .. } = self;
        let enum_ident = self.service_enum_ident();

        let mut defs = quote! {};
        for m in &self.methods {
            defs.append_all(m.service_enum_entry());
        }

        quote! {
            #[derive(::serde::Serialize, ::serde::Deserialize)]
            #vis enum #enum_ident {
                #defs
            }
        }
    }

    /// Server trait definition.
    fn server_trait(&self) -> TokenStream2 {
        let Self { vis, ident, .. } = self;
        let enum_ident = self.service_enum_ident();

        // Attributes.
        let mut attrs = quote! {};
        for attr in &self.attrs {
            attr.to_tokens(&mut attrs);
        }

        // Generate server trait methods and match discriminators for dispatch.
        let mut defs = quote! {};
        let mut dispatch_match = quote! {};
        for m in &self.methods {
            defs.append_all(m.server_trait_method());
            dispatch_match.append_all(m.server_dispatch(&enum_ident));
        }

        quote! {
            #attrs
            #[async_trait::async_trait]
            #vis trait #ident : Sized + Send + Sync + 'static {
                #defs

                #[doc="Binds an RPC server to a chmux server."]
                async fn serve<Content, Codec>(
                    self,
                    mut server: ::chmux::Server<#enum_ident, Content, Codec>,
                ) -> Result<Self, ::chmux::ServerError>
                where
                    Content: ::serde::Serialize + ::serde::de::DeserializeOwned + Send + 'static,
                    Codec: ::chmux::CodecFactory<Content> + 'static
                {
                    use ::futures::future::FutureExt;
                    use ::futures::sink::SinkExt;
                    use ::futures::stream::StreamExt;
                    let this = ::std::sync::Arc::new(::tokio::sync::RwLock::new(Some(self)));
                    loop {
                        match server.next().await {
                            None => {
                                let mut this_write = this.write().await;
                                return Ok(this_write.take().unwrap());
                            }
                            Some((Err(err), _)) => return Err(err),
                            Some((Ok(service), req)) => {
                                let this = this.clone();
                                match service {
                                    #dispatch_match
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The client proxy.
    fn client_part(&self) -> TokenStream2 {
        let Self { vis, ident, .. } = self;
        let enum_ident = self.service_enum_ident();
        let client_trait_ident = format_ident!("{}ClientDispatch", ident);
        let client_inner_ident = format_ident!("{}ClientInner", ident);
        let client_ident = format_ident!("{}Client", ident);

        // Client struct attributes.
        let mut attrs = quote! {};
        for attr in &self.attrs {
            attr.to_tokens(&mut attrs);
        }

        // Generate client method definition, implementations and wrappers.
        let mut method_defs = quote! {};
        let mut method_impls = quote! {};
        let mut method_wrappers = quote! {};
        for m in &self.methods {
            // Client trait method definition.
            method_defs.append_all(m.client_method_header());
            method_defs.append_all(quote! {;});

            // Client proxy method implementation.
            method_impls.append_all(m.client_method_impl(&enum_ident));

            // Client wrapper implementation.
            method_wrappers.append_all(m.client_method_wrapper());
        }

        quote! {
            #[doc(hidden)]
            #[async_trait::async_trait]
            trait #client_trait_ident {
                #method_defs
            }

            #[doc(hidden)]
            struct #client_inner_ident <Content, Codec>
            where
                Content: Send,
            {
                client: ::chmux::Client<#enum_ident, Content, Codec>
            }

            #[doc(hidden)]
            #[async_trait::async_trait]
            impl<Content, Codec> #client_trait_ident for #client_inner_ident <Content, Codec>
            where
                Content: Send + 'static,
                Codec: ::chmux::CodecFactory<Content> + Send + Sync + 'static,
            {
                #method_impls
            }

            #attrs
            #vis struct #client_ident {
                inner: Box<dyn #client_trait_ident + Send + Sync>
            }

            impl #client_ident {
                #[doc="Binds an RPC client to a chmux client."]
                pub fn bind<Content, Codec>(client: ::chmux::Client<#enum_ident, Content, Codec>) -> Self
                where
                    Content: Send + 'static,
                    Codec: ::chmux::CodecFactory<Content> + Send + Sync + 'static,
                {
                    Self {
                        inner: Box::new(#client_inner_ident {client})
                    }
                }

                #method_wrappers
            }

            impl ::std::fmt::Debug for #client_ident {
                fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    write!(f, "#client_ident")
                }
            }

            impl ::std::ops::Drop for #client_ident {
                fn drop(&mut self) {
                    // Drop implementation must be present to make reference guard work.
                }
            }
        }
    }
}

/// Argument type of a service method.
#[derive(Debug)]
enum ServiceMethodArgType {
    /// chmux::Sender<_>
    Sender(Type),
    /// chmux::Receiver<_>
    Receiver(Type),
    /// other type
    Other(Type),
}

impl ServiceMethodArgType {
    /// Parses a type of the form `(chmux::)Name<TY>` and returns the type `TY`,
    /// if it matches the pattern.
    fn parse_chmux_type(ty: &Type, name: &str) -> Option<Type> {
        if let Type::Path(path) = ty {
            let segments = &path.path.segments;
            let last;
            if segments.len() > 2 {
                return None;
            } else if segments.len() == 2 {
                if segments[0].ident.to_string() != "chmux" {
                    return None;
                }
                last = &segments[1];
            } else {
                last = &segments[0];
            }

            if last.ident.to_string() != name {
                return None;
            }
            if let PathArguments::AngleBracketed(ty_args) = &last.arguments {
                if ty_args.args.len() != 1 {
                    return None;
                }
                if let GenericArgument::Type(generic_ty) = &ty_args.args[0] {
                    return Some(generic_ty.clone());
                } else {
                    return None;
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    /// Parse type.
    fn parse(ty: &Type) -> Self {
        if let Some(ty) = Self::parse_chmux_type(ty, "Sender") {
            Self::Sender(ty)
        } else if let Some(ty) = Self::parse_chmux_type(ty, "Receiver") {
            Self::Receiver(ty)
        } else {
            Self::Other(ty.clone())
        }
    }
}

/// Response delivery method and type.
#[derive(Debug)]
enum Response {
    /// Response is streamed by a sender.
    Sender(Type),
    /// Single response of specified type is returned.
    Single(Type),
}

impl Response {
    /// Return type.
    fn ty(&self) -> &Type {
        match self {
            Self::Sender(ty) => ty,
            Self::Single(ty) => ty,
        }
    }
}

/// Self reference of method.
#[derive(Debug)]
enum SelfRef {
    /// &self
    Ref,
    /// &mut self
    MutRef,
}

/// A named argument.
#[derive(Debug)]
struct NamedArg {
    /// Name.
    ident: Ident,
    /// Type.
    ty: Type,
}

impl NamedArg {
    /// Create a `NamedArg` from a `PatType`.
    fn extract(pat_type: &PatType) -> syn::Result<Self> {
        let ident = if let Pat::Ident(pat_ident) = &*pat_type.pat {
            pat_ident.ident.clone()
        } else {
            return Err(syn::Error::new(pat_type.pat.span(), "expected identifier"));
        };
        Ok(Self { ident, ty: (*pat_type.ty).clone() })
    }
}

/// A method in a service trait.
#[derive(Debug)]
struct ServiceMethod {
    /// Method attributes.
    attrs: Vec<Attribute>,
    /// Method name.
    ident: Ident,
    /// Self reference of method.
    self_ref: SelfRef,
    /// Request arguments.
    request: Vec<NamedArg>,
    /// Type of receiver, if present.
    receiver: Option<Type>,
    /// Response delivery method and type.
    response: Response,
    /// Whether method should be cancelled, if client sends hangup message.
    cancel: bool,
}

impl Parse for ServiceMethod {
    /// Parses a method within the service trait.
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse method definition.
        let mut attrs = input.call(Attribute::parse_outer)?;
        input.parse::<Token![async]>()?;
        input.parse::<Token![fn]>()?;
        let ident: Ident = input.parse()?;
        if ident.to_string() == "serve" || ident.to_string() == "bind" {
            return Err(input.error("Service method must not be named 'serve' or 'bind'."));
        }

        // Check for no_cancel attribute.
        let mut cancel = true;
        attrs.retain(|attr| {
            if let Some(attr) = attr.path.get_ident() {
                if attr.to_string() == "no_cancel" {
                    cancel = false;
                    return false;
                }
            }
            true
        });

        // Parse arguments.
        let content;
        parenthesized!(content in input);
        let args: Punctuated<FnArg, Comma> = content.parse_terminated(FnArg::parse)?;

        // Parse return type.
        let ret: ReturnType = input.parse()?;
        input.parse::<Token![;]>()?;

        // Extract request, sender and receiver arguments.
        let mut self_ref = None;
        let mut request = Vec::new();
        let mut sender = None;
        let mut receiver = None;
        for arg in args {
            match arg {
                // &self or &mut self receiver
                FnArg::Receiver(recv) => {
                    self_ref = Some(if recv.reference.is_some() {
                        if recv.mutability.is_some() {
                            SelfRef::MutRef
                        } else {
                            SelfRef::Ref
                        }
                    } else {
                        return Err(input.error("Service method must use &self or &mut self receiver."));
                    });
                }
                // other argument
                FnArg::Typed(pat_type) => {
                    let arg = NamedArg::extract(&pat_type)?;
                    match ServiceMethodArgType::parse(&arg.ty) {
                        ServiceMethodArgType::Other(_) if sender.is_none() && receiver.is_none() => {
                            request.push(arg)
                        }
                        ServiceMethodArgType::Sender(sty) if sender.is_none() && receiver.is_none() => {
                            sender = Some(sty)
                        }
                        ServiceMethodArgType::Receiver(rty) if receiver.is_none() => receiver = Some(rty),
                        _ => {
                            return Err(input.error(
                                "Service method must have zero or more request arguments, \
                                                     optionally followed by a chmux::Sender<_>, \
                                                     optionally followed by a chmux::Receiver<_>.",
                            ))
                        }
                    }
                }
            }
        }
        let self_ref = self_ref.ok_or(input.error("Service method must have self argument"))?;

        // Determine response method and type.
        let response = match (ret, sender) {
            (ReturnType::Default, None) => Response::Single(unit_type()),
            (ReturnType::Default, Some(sender)) => Response::Sender(sender),
            (ReturnType::Type(_, return_ty), None) => Response::Single(*return_ty),
            _ => {
                return Err(input.error(
                    "Service method must not simultaneously return a value and \
                                         have a chmux::Sender<_> argument to stream its response.",
                ))
            }
        };

        Ok(Self { attrs, ident, self_ref, request, receiver, response, cancel })
    }
}

impl ServiceMethod {
    /// Method definition within server trait.
    fn server_trait_method(&self) -> TokenStream2 {
        let Self { ident, .. } = self;

        // Attributes
        let attrs = attribute_tokens(&self.attrs);

        // Build argument list.
        let mut args = quote! {};

        // Self argument.
        let self_ref = match self.self_ref {
            SelfRef::Ref => quote! {&self,},
            SelfRef::MutRef => quote! {&mut self,},
        };
        args.append_all(self_ref);

        // Request arguments.
        for NamedArg { ident, ty } in &self.request {
            args.append_all(quote! { #ident : #ty , });
        }

        // Optional sender.
        if let Response::Sender(sender) = &self.response {
            args.append_all(quote! {rpc_tx: ::chmux::Sender<#sender>, });
        }

        // Optional receiver.
        if let Some(receiver) = &self.receiver {
            args.append_all(quote! {rpx_rx: ::chmux::Receiver<#receiver>, });
        }

        // Optional return type.
        let ret = if let Response::Single(ret_ty) = &self.response {
            quote! { -> #ret_ty}
        } else {
            quote! {}
        };

        quote! {
            #attrs async fn #ident ( #args ) #ret;
        }
    }

    /// Entry within service enum.
    fn service_enum_entry(&self) -> TokenStream2 {
        let ident = to_pascal_case(&self.ident);

        let mut entries = quote! {};
        for NamedArg { ident, ty } in &self.request {
            entries.append_all(quote! { #ident : #ty , });
        }

        quote! { #ident {#entries}, }
    }

    /// Discriminator and dispatch code within server loop.
    fn server_dispatch(&self, service_enum_ident: &Ident) -> TokenStream2 {
        let ident = &self.ident;
        let enum_entry_ident = to_pascal_case(&ident);

        // Build match selector and call argument list.
        let mut entries = quote! {};
        let mut args = quote! {};
        for (i, NamedArg { ident, .. }) in self.request.iter().enumerate() {
            let arg_ident = format_ident!("arg{}", i);
            entries.append_all(quote! { #ident: #arg_ident , });
            args.append_all(quote! { #arg_ident , });
        }
        if let Response::Sender(_) = &self.response {
            args.append_all(quote! { tx, });
        }
        if let Some(_) = &self.receiver {
            args.append_all(quote! { rx, });
        }

        // Build accept call.
        let tx_type = match &self.response {
            Response::Sender(ty) => ty,
            Response::Single(ty) => ty,
        };
        let unit_type = unit_type();
        let (rx_type, rx_ident) = match &self.receiver {
            Some(ty) => (ty, quote! {rx}),
            None => (&unit_type, quote! {_}),
        };
        let tx_rx = quote! {
            let (tx, #rx_ident): (::chmux::Sender<#tx_type>, ::chmux::Receiver<#rx_type>) = req.accept().await;
        };

        // Request hangup notification, if desired.
        let hangup = if self.cancel {
            quote! { let mut hangup = tx.hangup_notify().fuse(); }
        } else {
            quote! {}
        };

        // Get (mutable) reference to our object.
        let obj = match &self.self_ref {
            SelfRef::Ref => quote! {
                let this_read = this.read().await;
                let this_obj = this_read.as_ref().unwrap();
            },
            SelfRef::MutRef => quote! {
                let mut this_write = this.write().await;
                let this_obj = this_write.as_mut().unwrap();
            },
        };

        // Obtain Future for method call.
        let fut = quote! {
            let mut ret_fut = this_obj.#ident(#args).fuse();
        };

        // Send reply.
        let reply = match &self.response {
            Response::Single(_) => quote! {
                ::futures::pin_mut!(tx);
                let _ = tx.send(ret).await;
            },
            Response::Sender(_) => quote! {},
        };

        // Future execution.
        let exec = if self.cancel {
            quote! {
                ::tokio::select! {
                    ret = ret_fut => { #reply },
                    () = hangup => { }
                }
            }
        } else {
            quote! {
                let ret = ret_fut.await;
            }
        };

        // Task execution sequence.
        let task = quote! {
            #tx_rx
            #hangup
            #obj
            #fut
            #exec
        };

        // Spawning of task for immutable method.
        // Execution on server loop for mutable method.
        let task_spawn = match &self.self_ref {
            SelfRef::Ref => quote! {::tokio::spawn(async move { #task });},
            SelfRef::MutRef => quote! { #task },
        };

        quote! {
            #service_enum_ident :: #enum_entry_ident { #entries } => {
                #task_spawn
            },
        }
    }

    /// Argument definitions with types.
    fn arg_defs(&self) -> TokenStream2 {
        let mut arg_defs = quote! {};
        for NamedArg { ident, ty } in &self.request {
            arg_defs.append_all(quote! {#ident : #ty ,});
        }
        arg_defs
    }

    /// Comma-seperated argument list.
    fn arg_list(&self) -> TokenStream2 {
        let mut arg_list = quote! {};
        for NamedArg { ident, .. } in &self.request {
            arg_list.append_all(quote! {#ident, });
        }
        arg_list
    }

    /// Client method definition.
    fn client_method_header(&self) -> TokenStream2 {
        let ident = &self.ident;

        // Self reference.
        let self_ref = match &self.self_ref {
            SelfRef::Ref => quote! {self},
            SelfRef::MutRef => quote! {mut self},
        };

        // Argument definition in method signature and argument list.
        let arg_defs = self.arg_defs();

        // Method return type.
        let return_ty = match (&self.response, &self.receiver) {
            (Response::Sender(sender), Some(receiver)) => quote! {
                Result<(::chrpc::Sender<'a, #receiver>, ::chrpc::Receiver<'a, #sender>),
                    ::chmux::ConnectError>
            },
            (Response::Sender(sender), None) => quote! {
                Result<::chrpc::Receiver<'a, #sender>, ::chmux::ConnectError>
            },
            (Response::Single(ret), Some(receiver)) => quote! {
                Result<::chrpc::SendingCall<'a, #receiver, #ret>, ::chmux::ConnectError>
            },
            (Response::Single(ret), None) => quote! {
                Result<#ret, ::chrpc::CallError>
            },
        };

        quote! {
            async fn #ident <'a> (&'a #self_ref, #arg_defs) -> #return_ty
        }
    }

    /// Client method implementation.
    fn client_method_impl(&self, service_enum_ident: &Ident) -> TokenStream2 {
        let method_header = self.client_method_header();
        let service_ident = to_pascal_case(&self.ident);

        // Argument list.
        let arg_list = self.arg_list();

        // Server response type, i.e. client return or receiver type.
        let response_ty = self.response.ty();

        // Server receiver type, i.e. client sender type.
        let unit_type = unit_type();
        let receive_ty = match &self.receiver {
            Some(ty) => ty,
            None => &unit_type,
        };

        // Processing statements.
        let processing = match (&self.response, &self.receiver) {
            (Response::Sender(_), Some(_)) => quote! {
                Ok((::chrpc::Sender::new(tx), ::chrpc::Receiver::new(rx)))
            },
            (Response::Sender(_), None) => quote! {
                Ok(::chrpc::Receiver::new(rx))
            },
            (Response::Single(_), Some(_)) => quote! {
                Ok(::chrpc::SendingCall::new(tx, rx))
            },
            (Response::Single(_), None) => quote! {
                ::std::mem::drop(tx);
                let ret = rx.next().await.ok_or(::chmux::ReceiveError::MultiplexerError)??;
                Ok(ret)
            },
        };

        quote! {
            #method_header {
                let service = #service_enum_ident :: #service_ident { #arg_list };
                let (mut tx, mut rx): (::chmux::Sender<#receive_ty>, ::chmux::Receiver<#response_ty>) =
                    self.client.connect(service).await?;
                #processing
            }
        }
    }

    /// Client method wrapper.
    fn client_method_wrapper(&self) -> TokenStream2 {
        let method_header = self.client_method_header();
        let ident = &self.ident;

        // Method attributes.
        let attrs = attribute_tokens(&self.attrs);

        // Argument list.
        let arg_list = self.arg_list();

        quote! {
            #attrs
            pub #method_header {
                self.inner.#ident (#arg_list).await
            }
        }
    }
}

/// Denotes a trait defining an RPC server.
///
/// It adds the provided method `serve` to the trait, which serves the object using
/// a `chmux::Server`.
/// All methods in the service trait definition must be async.
/// The server trait implementation must use `[async_trait::async_trait]` attribute.
///
/// Additionally a client proxy struct named using the same name suffixed with `Client`
/// is generated.
/// It is constructed using the method `bind` from a `chmux::Client`.
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let service = parse_macro_input!(input as Service);

    let server_trait = service.server_trait();
    let service_enum = service.service_enum();
    let client_part = service.client_part();

    TokenStream::from(quote! {
        #server_trait
        #service_enum
        #client_part
    })
}
