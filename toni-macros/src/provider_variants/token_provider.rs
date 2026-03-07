use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Result, Token, Type, TypePath,
    parse::{Parse, ParseStream},
};

use crate::shared::TokenType;

/// Parse provider_token! macro input
/// Syntax: provider_token!("TOKEN", Type)
/// or provider_token!(TOKEN_CONST, Type)
///
/// This creates a provider with a custom token, similar to NestJS's useClass pattern:
/// ```typescript
/// {
///   provide: 'CUSTOM_TOKEN',
///   useClass: SomeClass
/// }
/// ```
/// The type is registered ONLY under the custom token, NOT under its type name.
/// This is different from provider_alias! which requires the type to be pre-registered.
/// Note: Scope is inherited from the type's #[injectable(scope = "...")] attribute.
/// Scope override is NOT supported.
pub struct ProviderTokenInput {
    pub token: TokenType,
    pub provider_type: Type,
}

impl Parse for ProviderTokenInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let provider_type: Type = input.parse()?;

        Ok(ProviderTokenInput {
            token,
            provider_type,
        })
    }
}

pub fn handle_provider_token(input: TokenStream) -> Result<TokenStream> {
    let ProviderTokenInput {
        token,
        provider_type,
    } = syn::parse2(input)?;

    // Generate token expression for runtime
    let token_expr = token.to_token_expr();

    // Extract the type path to construct the Manager name
    // Eg. fsor DatabaseService, we need DatabaseServiceManager
    let type_path = match &provider_type {
        Type::Path(TypePath { path, .. }) => path.clone(),
        _ => {
            return Err(syn::Error::new_spanned(
                provider_type,
                "provider_token! only supports simple type paths (e.g., DatabaseService or mymodule::DatabaseService)",
            ));
        }
    };

    // Build the Manager path by appending "Manager" to the last segment
    let mut manager_path = type_path.clone();
    if let Some(last_segment) = manager_path.segments.last_mut() {
        let type_ident = &last_segment.ident;
        let manager_ident = format_ident!("{}Manager", type_ident);
        last_segment.ident = manager_ident;
    }

    // Generate unique struct names based on token
    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let wrapper_manager_name = format_ident!("__ToniTokenProviderManager_{}", sanitized_name);

    // Generate the provider struct and implementation
    // The type is registered ONLY under the custom token
    let expanded = quote! {
        {
            // Manager struct for ProviderFactory trait implementation
            struct #wrapper_manager_name;

            // Implement ProviderFactory trait for the manager (used by module system)
            #[toni::async_trait]
            impl toni::traits_helpers::ProviderFactory for #wrapper_manager_name {
                async fn get_all_providers(
                    &self,
                    _dependencies: &toni::FxHashMap<
                        String,
                        std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                    >,
                ) -> toni::FxHashMap<
                    String,
                    std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                > {
                    let mut providers = toni::FxHashMap::default();

                    // Get the type's Manager to create the actual provider
                    // The #[injectable] macro generates a TypeManager struct
                    // Eg. for DatabaseService, it generates DatabaseServiceManager
                    let type_manager = #manager_path {};

                    // Get the provider instance from the type's manager
                    let type_providers = type_manager.get_all_providers(_dependencies).await;
                    let type_token = std::any::type_name::<#provider_type>().to_string();
                    let type_provider = type_providers
                        .get(&type_token)
                        .expect(&format!(
                            "Failed to create provider for type {}. Make sure it has #[injectable] attribute.",
                            type_token
                        ))
                        .clone();

                    // Create a wrapper that uses the custom token
                    #[derive(Clone)]
                    struct CustomTokenProvider {
                        custom_token: String,
                        inner_provider: std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                    }

                    #[toni::async_trait]
                    impl toni::traits_helpers::Provider for CustomTokenProvider {
                        fn get_token(&self) -> String {
                            self.custom_token.clone()
                        }

                        fn get_token_manager(&self) -> String {
                            self.custom_token.clone()
                        }

                        fn get_scope(&self) -> toni::ProviderScope {
                            // Use the scope from the inner provider
                            self.inner_provider.get_scope()
                        }

                        async fn execute(
                            &self,
                            params: Vec<Box<dyn std::any::Any + Send>>,
                            req: Option<&toni::HttpRequest>,
                        ) -> Box<dyn std::any::Any + Send> {
                            // Delegate to the inner provider
                            self.inner_provider.execute(params, req).await
                        }

                        // Delegate enhancer methods to inner provider
                        fn as_guard(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Guard>> {
                            self.inner_provider.as_guard()
                        }

                        fn as_interceptor(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Interceptor>> {
                            self.inner_provider.as_interceptor()
                        }

                        fn as_pipe(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Pipe>> {
                            self.inner_provider.as_pipe()
                        }

                        fn as_middleware(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::middleware::Middleware>> {
                            self.inner_provider.as_middleware()
                        }
                    }

                    // Register under the custom token only
                    let custom_token = #token_expr;
                    let provider_wrapper = CustomTokenProvider {
                        custom_token: custom_token.clone(),
                        inner_provider: type_provider,
                    };

                    providers.insert(
                        custom_token,
                        std::sync::Arc::new(
                            Box::new(provider_wrapper) as Box<dyn toni::traits_helpers::Provider>
                        ),
                    );

                    providers
                }

                fn get_name(&self) -> String {
                    #token_expr
                }

                fn get_token(&self) -> String {
                    #token_expr
                }

                fn get_dependencies(&self) -> Vec<String> {
                    // Delegate to the type's manager to get its dependencies
                    let type_manager = #manager_path {};
                    type_manager.get_dependencies()
                }
            }

            // Return the manager instance
            #wrapper_manager_name
        }
    };

    Ok(expanded)
}
