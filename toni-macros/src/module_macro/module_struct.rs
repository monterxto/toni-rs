use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    Ident, ImplItem, ItemImpl, ItemStruct, Token, Type, TypePath, Visibility, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::shared::lifecycle_hooks::{detect_lifecycle_hooks, strip_lifecycle_attrs};

#[derive(Default)]
struct ModuleConfig {
    imports: Vec<syn::Expr>,
    controllers: Vec<syn::Expr>,
    providers: Vec<syn::Expr>,
    exports: Vec<Ident>,
    global: bool,
}

struct ConfigParser {
    imports: Vec<syn::Expr>,
    controllers: Vec<syn::Expr>,
    providers: Vec<syn::Expr>,
    exports: Vec<Ident>,
    global: bool,
}

impl Parse for ConfigParser {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut config = ConfigParser {
            imports: Vec::new(),
            controllers: Vec::new(),
            providers: Vec::new(),
            exports: Vec::new(),
            global: false,
        };

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            // Handle global as a boolean (not an array)
            if key.to_string().as_str() == "global" {
                input.parse::<Token![:]>()?;
                let value: syn::LitBool = input.parse()?;
                config.global = value.value;

                if !input.is_empty() {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }

            input.parse::<Token![:]>()?;
            let content;
            bracketed!(content in input);

            match key.to_string().as_str() {
                "imports" => {
                    // Parse imports as expressions (allows method calls, etc.)
                    let fields = Punctuated::<syn::Expr, Token![,]>::parse_terminated(&content)?;
                    config.imports = fields.into_iter().collect();
                }
                "controllers" => {
                    let fields = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                    config.controllers = fields
                        .into_iter()
                        .map(|field| {
                            // Bar::__toni_controller_factory() — only Bar needs to be in scope
                            syn::parse_quote! { #field::__toni_controller_factory() }
                        })
                        .collect()
                }
                "providers" => {
                    // Parse providers as expressions (allows macro calls like provide!(...))
                    let fields = Punctuated::<syn::Expr, Token![,]>::parse_terminated(&content)?;
                    config.providers = fields
                        .into_iter()
                        .map(|expr| {
                            // Simple path like Foo → Foo::__toni_provider_factory()
                            // Only Foo needs to be in scope; FooProviderFactory stays hidden.
                            if let syn::Expr::Path(ref expr_path) = expr {
                                if expr_path.attrs.is_empty() && expr_path.qself.is_none() {
                                    let path = &expr_path.path;
                                    return syn::parse_quote! { #path::__toni_provider_factory() };
                                }
                            }
                            // Macro calls like provide!(...) are used as-is
                            expr
                        })
                        .collect();
                }
                "exports" => {
                    let fields = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                    config.exports = fields.into_iter().collect()
                }
                _ => return Err(syn::Error::new(key.span(), "Unknown field")),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(config)
    }
}

impl TryFrom<TokenStream> for ModuleConfig {
    type Error = syn::Error;
    fn try_from(attr: TokenStream) -> syn::Result<Self> {
        let parser = syn::parse::<ConfigParser>(attr)?;
        Ok(ModuleConfig {
            imports: parser.imports,
            controllers: parser.controllers,
            providers: parser.providers,
            exports: parser.exports,
            global: parser.global,
        })
    }
}

/// Represents either a struct definition or impl block for module parsing
enum ModuleInput {
    Struct(ItemStruct),
    Impl(ItemImpl),
}

impl ModuleInput {
    fn ident(&self) -> &Ident {
        match self {
            ModuleInput::Struct(s) => &s.ident,
            ModuleInput::Impl(i) => match i.self_ty.as_ref() {
                Type::Path(TypePath { path, .. }) => &path.segments.last().unwrap().ident,
                _ => panic!("Invalid impl type"),
            },
        }
    }

    fn visibility(&self) -> Option<&Visibility> {
        match self {
            ModuleInput::Struct(s) => Some(&s.vis),
            ModuleInput::Impl(_) => None,
        }
    }

    fn impl_items(&self) -> Vec<&ImplItem> {
        match self {
            ModuleInput::Struct(_) => vec![],
            ModuleInput::Impl(i) => i.items.iter().collect(),
        }
    }

    fn validate_unit_struct(&self) -> syn::Result<()> {
        if let ModuleInput::Struct(s) = self {
            if !s.fields.is_empty() {
                return Err(syn::Error::new_spanned(
                    &s.fields,
                    "Module structs must be unit structs with no fields.\n\
                     Example: `pub struct AppModule;`\n\
                     \n\
                     If you need to configure module behavior, use the macro attributes:\n\
                     #[module(\n\
                         imports: [...],\n\
                         providers: [...],\n\
                         controllers: [...],\n\
                         exports: [...],\n\
                     )]",
                ));
            }
        }
        Ok(())
    }
}

pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let config = match ModuleConfig::try_from(attr) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };

    // Try parsing as struct first, then fall back to impl block for backward compatibility
    let input = if let Ok(struct_input) = syn::parse::<ItemStruct>(item.clone()) {
        ModuleInput::Struct(struct_input)
    } else if let Ok(impl_input) = syn::parse::<ItemImpl>(item) {
        ModuleInput::Impl(impl_input)
    } else {
        return syn::Error::new(
            Span::call_site(),
            "Module macro must be applied to either a struct or impl block",
        )
        .to_compile_error()
        .into();
    };

    // Validate unit struct if using struct syntax
    if let Err(e) = input.validate_unit_struct() {
        return e.to_compile_error().into();
    }

    let input_ident = input.ident().clone();
    let input_name = input_ident.to_string();
    let visibility = input
        .visibility()
        .cloned()
        .unwrap_or_else(|| syn::parse_quote!(pub));
    let imports = &config.imports;
    let controllers = config.controllers;
    let providers = &config.providers;
    let exports = &config.exports;
    let is_global = config.global;

    // Extract configure_middleware method from impl items if present
    let configure_middleware_impl = {
        let mut middleware_impl = quote! {};
        for item in input.impl_items() {
            if let ImplItem::Fn(method) = item {
                if method.sig.ident == "configure_middleware" {
                    let block = &method.block;
                    let sig = &method.sig;

                    if sig.inputs.len() != 2 {
                        return quote! {
                            compile_error!("configure_middleware must have signature: fn configure_middleware(&self, consumer: &mut MiddlewareConsumer)");
                        }.into();
                    }

                    middleware_impl = quote! {
                        #sig #block
                    };
                    break;
                }
            }
        }
        middleware_impl
    };

    // Detect lifecycle hooks from impl items and generate ModuleMetadata overrides.
    // Module hooks are sync (unlike provider/controller hooks which are async).
    let module_lifecycle_impl = if let ModuleInput::Impl(impl_block) = &input {
        let hooks = detect_lifecycle_hooks(impl_block);
        let mut methods = Vec::new();

        if let Some(method) = hooks.on_module_init {
            methods.push(quote! {
                fn on_module_init(&self, _container: ::std::rc::Rc<::std::cell::RefCell<::toni::injector::ToniContainer>>) -> anyhow::Result<()> {
                    self.#method();
                    Ok(())
                }
            });
        }
        if let Some(method) = hooks.on_application_bootstrap {
            methods.push(quote! {
                fn on_application_bootstrap(&self, _container: ::std::rc::Rc<::std::cell::RefCell<::toni::injector::ToniContainer>>) -> anyhow::Result<()> {
                    self.#method();
                    Ok(())
                }
            });
        }
        if let Some(method) = hooks.on_module_destroy {
            methods.push(quote! {
                fn on_module_destroy(&self, _container: ::std::rc::Rc<::std::cell::RefCell<::toni::injector::ToniContainer>>) -> anyhow::Result<()> {
                    self.#method();
                    Ok(())
                }
            });
        }
        if let Some(method) = hooks.before_application_shutdown {
            methods.push(quote! {
                fn before_application_shutdown(&self, signal: Option<String>, _container: ::std::rc::Rc<::std::cell::RefCell<::toni::injector::ToniContainer>>) -> anyhow::Result<()> {
                    self.#method(signal);
                    Ok(())
                }
            });
        }
        if let Some(method) = hooks.on_application_shutdown {
            methods.push(quote! {
                fn on_application_shutdown(&self, signal: Option<String>, _container: ::std::rc::Rc<::std::cell::RefCell<::toni::injector::ToniContainer>>) -> anyhow::Result<()> {
                    self.#method(signal);
                    Ok(())
                }
            });
        }

        quote! { #(#methods)* }
    } else {
        quote! {}
    };

    // Emit user's impl block (minus lifecycle attrs and configure_middleware) so methods are
    // available on the struct. configure_middleware is consumed by the macro and re-emitted as
    // a ModuleMetadata override, so keeping it here would produce a dead_code warning.
    let user_impl_block = if let ModuleInput::Impl(impl_block) = &input {
        let mut cleaned = strip_lifecycle_attrs(impl_block);
        cleaned.items.retain(|item| match item {
            syn::ImplItem::Fn(method) => method.sig.ident != "configure_middleware",
            _ => true,
        });
        quote! { #cleaned }
    } else {
        quote! {}
    };

    let generated = quote! {
        #visibility struct #input_ident;

        #user_impl_block

        impl #input_ident {
            pub fn module_definition() -> ::toni::module_helpers::module_enum::ModuleDefinition {
                let app_module = Self;
                ::toni::module_helpers::module_enum::ModuleDefinition::DefaultModule(Box::new(app_module))
            }
            pub fn new() -> Self {
                Self
            }
        }

        // Implement From to allow passing module directly without .module_definition()
        impl From<#input_ident> for ::toni::module_helpers::module_enum::ModuleDefinition {
            fn from(module: #input_ident) -> Self {
                Self::DefaultModule(Box::new(module))
            }
        }

        impl ::toni::traits_helpers::ModuleMetadata for #input_ident {
            fn get_id(&self) -> String {
                #input_name.to_string()
            }
            fn get_name(&self) -> String {
                #input_name.to_string()
            }
            fn is_global(&self) -> bool {
                #is_global
            }
            fn imports(&self) -> Option<Vec<Box<dyn ::toni::traits_helpers::ModuleMetadata>>> {
                Some(vec![#(Box::new(#imports)),*])
            }
            fn controllers(&self) -> Option<Vec<Box<dyn ::toni::traits_helpers::ControllerFactory>>> {
                Some(vec![#(Box::new(#controllers)),*])
            }
            fn providers(&self) -> Option<Vec<Box<dyn ::toni::traits_helpers::ProviderFactory>>> {
                Some(vec![#(Box::new(#providers)),*])
            }
            fn exports(&self) -> Option<Vec<String>> {
                Some(vec![#(::std::any::type_name::<#exports>().to_string()),*])
            }

            // Include user-defined configure_middleware if present
            #configure_middleware_impl

            // Include user-defined lifecycle hooks if present
            #module_lifecycle_impl
        }
    };

    generated.into()
}
