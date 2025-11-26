use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    Ident, ImplItem, ItemImpl, Token, Type, TypePath, bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

#[derive(Default)]
struct ModuleConfig {
    imports: Vec<syn::Expr>,
    controllers: Vec<Ident>,
    providers: Vec<syn::Expr>,
    exports: Vec<Ident>,
    global: bool,
}

struct ConfigParser {
    imports: Vec<syn::Expr>,
    controllers: Vec<Ident>,
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
                        .map(|field| Ident::new(&format!("{}Manager", field), field.span()))
                        .collect()
                }
                "providers" => {
                    // Parse providers as expressions (allows macro calls like provider_value!(...))
                    let fields = Punctuated::<syn::Expr, Token![,]>::parse_terminated(&content)?;
                    config.providers = fields
                        .into_iter()
                        .map(|expr| {
                            // Smart detection: if it's a simple identifier/path, append "Manager"
                            // This keeps backward compatibility with: providers: [ConfigService]
                            if let syn::Expr::Path(ref expr_path) = expr {
                                // Check if it's a simple path (not a macro call or complex expression)
                                if expr_path.attrs.is_empty() && expr_path.qself.is_none() {
                                    let path = &expr_path.path;
                                    // Get the last segment (the actual type name)
                                    if let Some(last_segment) = path.segments.last() {
                                        let type_name = &last_segment.ident;
                                        // Create the Manager variant
                                        let manager_ident = Ident::new(
                                            &format!("{}Manager", type_name),
                                            type_name.span(),
                                        );

                                        // Reconstruct the path with Manager suffix
                                        let mut manager_path = path.clone();
                                        if let Some(last) = manager_path.segments.last_mut() {
                                            last.ident = manager_ident;
                                        }

                                        // Return new expression with the Manager path
                                        return syn::Expr::Path(syn::ExprPath {
                                            attrs: vec![],
                                            qself: None,
                                            path: manager_path,
                                        });
                                    }
                                }
                            }
                            // Otherwise use the expression as-is (for macro calls like provider_value!(...))
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

/// Extract configure_middleware method from impl block if present
fn extract_configure_middleware_method(input: &ItemImpl) -> TokenStream2 {
    // Look for a method named "configure_middleware" in the impl block
    for item in &input.items {
        if let ImplItem::Fn(method) = item {
            if method.sig.ident == "configure_middleware" {
                // Found it! Extract the method body and signature
                let block = &method.block;
                let sig = &method.sig;

                // Validate signature matches expected pattern
                // Expected: fn configure_middleware(&self, consumer: &mut MiddlewareConsumer)
                if sig.inputs.len() != 2 {
                    return quote! {
                        compile_error!("configure_middleware must have signature: fn configure_middleware(&self, consumer: &mut MiddlewareConsumer)");
                    };
                }

                // Return the method implementation
                return quote! {
                    #sig #block
                };
            }
        }
    }

    // No configure_middleware method found, return empty (use default trait impl)
    quote! {}
}

pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let config = match ModuleConfig::try_from(attr) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemImpl);
    let input_type = input.self_ty.as_ref();
    let input_ident = match input_type {
        Type::Path(TypePath { path, .. }) => path.segments.last().unwrap().ident.clone(),
        _ => {
            return syn::Error::new(Span::call_site(), "Invalid input type")
                .to_compile_error()
                .into();
        }
    };
    let input_name = input_ident.to_string();
    let imports = &config.imports;
    let controllers = config.controllers;
    let providers = &config.providers;
    let exports = &config.exports;
    let exports_string: Vec<String> = exports.iter().map(|e| e.to_string()).collect();
    let is_global = config.global;

    // Extract configure_middleware method from impl block if present
    let configure_middleware_impl = extract_configure_middleware_method(&input);

    // Debug: uncomment to see what was extracted
    // eprintln!("configure_middleware_impl (len={}): {}", configure_middleware_impl.to_string().len(), configure_middleware_impl);

    // Generate a unique ModuleRefManager for this module
    let module_ref_manager_name = Ident::new(
        &format!("__ToniModuleRefManager_{}", input_name),
        Span::call_site(),
    );

    let generated = quote! {
        pub struct #input_ident;

        // Generate unique ModuleRefManager for this module
        pub struct #module_ref_manager_name {
            module_token: String,
        }

        impl #module_ref_manager_name {
            fn new() -> Self {
                Self {
                    module_token: #input_name.to_string(),
                }
            }
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Provider for #module_ref_manager_name {
            async fn get_all_providers(
                &self,
                _dependencies: &::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ProviderTrait>>
                >,
            ) -> ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ProviderTrait>>
            > {
                // Return the pre-configured ModuleRefManager
                let mut providers = ::toni::FxHashMap::default();
               // NOTE: The container is accessed via thread-local storage when ModuleRef methods are called
               // The module token is used to scope provider resolution to this module
                providers.insert(
                    ::std::any::type_name::<::toni::ModuleRef>().to_string(),
                    ::std::sync::Arc::new(Box::new(
                        ::toni::injector::ModuleRefProvider::new(self.module_token.clone())
                    ) as Box<dyn ::toni::traits_helpers::ProviderTrait>)
                );
                providers
            }

            fn get_name(&self) -> String {
                ::std::any::type_name::<::toni::ModuleRef>().to_string()
            }

            fn get_token(&self) -> String {
                ::std::any::type_name::<::toni::ModuleRef>().to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![]
            }
        }

        impl #input_ident {
            pub fn module_definition() -> ::toni::module_helpers::module_enum::ModuleDefinition {
                let app_module = Self;
                ::toni::module_helpers::module_enum::ModuleDefinition::DefaultModule(Box::new(app_module))
            }
            pub fn new() -> Self {
                Self
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
            fn controllers(&self) -> Option<Vec<Box<dyn ::toni::traits_helpers::Controller>>> {
                Some(vec![#(Box::new(#controllers)),*])
            }
            fn providers(&self) -> Option<Vec<Box<dyn ::toni::traits_helpers::Provider>>> {
                let mut providers_vec: Vec<Box<dyn ::toni::traits_helpers::Provider>> = vec![
                    #(Box::new(#providers)),*
                ];
                // Auto-inject ModuleRefManager for this module
                providers_vec.push(Box::new(#module_ref_manager_name::new()));
                Some(providers_vec)
            }
            fn exports(&self) -> Option<Vec<String>> {
                Some(vec![#(#exports_string.to_string()),*])
            }

            // Include user-defined configure_middleware if present
            #configure_middleware_impl
        }
    };

    generated.into()
}
