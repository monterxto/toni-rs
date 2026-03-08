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

    let type_path = match &provider_type {
        Type::Path(TypePath { path, .. }) => path.clone(),
        _ => {
            return Err(syn::Error::new_spanned(
                provider_type,
                "provider_token! only supports simple type paths (e.g., DatabaseService or mymodule::DatabaseService)",
            ));
        }
    };

    // Generate unique struct names based on token
    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let wrapper_factory_name = format_ident!("__ToniTokenProviderFactory_{}", sanitized_name);

    let expanded = quote! {
        {
            struct #wrapper_factory_name;

            #[toni::async_trait]
            impl toni::traits_helpers::ProviderFactory for #wrapper_factory_name {
                fn get_token(&self) -> String {
                    #token_expr
                }

                fn get_dependencies(&self) -> Vec<String> {
                    #type_path::__toni_provider_factory().get_dependencies()
                }

                async fn build(
                    &self,
                    deps: toni::FxHashMap<
                        String,
                        std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                    >,
                ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                    let inner_provider = #type_path::__toni_provider_factory().build(deps).await;

                    // Wrap it under the custom token
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

                        fn get_token_factory(&self) -> String {
                            self.custom_token.clone()
                        }

                        fn get_scope(&self) -> toni::ProviderScope {
                            self.inner_provider.get_scope()
                        }

                        async fn execute(
                            &self,
                            params: Vec<Box<dyn std::any::Any + Send>>,
                            req: Option<&toni::HttpRequest>,
                        ) -> Box<dyn std::any::Any + Send> {
                            self.inner_provider.execute(params, req).await
                        }

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

                    std::sync::Arc::new(Box::new(CustomTokenProvider {
                        custom_token: #token_expr,
                        inner_provider,
                    }) as Box<dyn toni::traits_helpers::Provider>)
                }
            }

            #wrapper_factory_name
        }
    };

    Ok(expanded)
}
