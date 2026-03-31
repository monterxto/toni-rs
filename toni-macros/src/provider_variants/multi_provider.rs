use std::sync::atomic::{AtomicU64, Ordering};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Expr, ExprPath, Result, Type, TypeTraitObject};

use super::unified_provide::ProviderVariant;
use crate::shared::TokenType;

// Each provide!(..., multi(...)) call gets a unique numeric suffix so that
// async_trait's hoisted helper types don't collide across multiple calls in
// the same module.
static MULTI_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> u64 {
    MULTI_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Generate a multi-provider contribution factory.
///
/// The factory:
/// - Returns a synthetic token unique to this (base_token, inner_type) pair
/// - Overrides `get_multi_base_token()` so the scanner registers it correctly
/// - On `build()`, constructs the inner value, coerces it to `Arc<dyn Trait + Send + Sync>`,
///   wraps it in a double-Arc (`Arc<Arc<dyn Trait+Send+Sync>>` stored as `Arc<dyn Any+Send+Sync>`)
///   so the injection-site codegen can recover the trait pointer via `Arc::downcast`.
pub fn handle_provide_multi(
    token: &TokenType,
    inner: ProviderVariant,
    trait_path: Option<TypeTraitObject>,
) -> Result<TokenStream> {
    let trait_ty = trait_path.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "multi requires an explicit trait annotation: `multi(dyn Trait + Send + Sync)`. \
             Example: provide!(\"PLUGINS\", PluginA, multi(dyn Plugin + Send + Sync))",
        )
    })?;

    let base_token_expr = token.to_token_expr();
    let id = unique_id();

    match inner {
        ProviderVariant::Value(expr) => {
            if let Expr::Path(ExprPath { ref path, .. }) = expr {
                // Type-path variant: provide!(TOKEN, PluginA, multi(dyn Trait))
                // Delegates to PluginA's registered ProviderFactory.
                let concrete_type = Type::Path(syn::TypePath {
                    qself: None,
                    path: path.clone(),
                });
                let factory_ident = format_ident!(
                    "{}ProviderFactory",
                    path.segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                );
                Ok(generate_type_multi(
                    id,
                    base_token_expr,
                    &concrete_type,
                    factory_ident,
                    &trait_ty,
                ))
            } else {
                // Raw-value variant: provide!(TOKEN, some_expr(), multi(dyn Trait))
                Ok(generate_value_multi(id, base_token_expr, &expr, &trait_ty))
            }
        }
        ProviderVariant::Factory(closure_expr) => {
            // Factory-closure variant: provide!(TOKEN, || PluginB::new(), multi(dyn Trait))
            Ok(generate_factory_multi(id, base_token_expr, &closure_expr, &trait_ty))
        }
        ProviderVariant::Alias(_) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "`existing(...)` is not yet supported with `multi`. \
             Register the type directly: provide!(TOKEN, PluginA, multi(dyn Trait))",
        )),
        ProviderVariant::TokenProvider(_) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "`provider(...)` cannot be combined with `multi`",
        )),
        ProviderVariant::Multi { .. } => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "nested `multi` is not supported",
        )),
    }
}

/// Generate per-call unique struct/fn names for contrib + factory.
fn make_names(id: u64) -> (proc_macro2::Ident, proc_macro2::Ident, proc_macro2::Ident) {
    let contrib = format_ident!("__ToniMultiContrib_{}", id);
    let make_item = format_ident!("__toni_make_multi_item_{}", id);
    let factory = format_ident!("__ToniMultiFactory_{}", id);
    (contrib, make_item, factory)
}

/// Generate the contrib provider struct + impl.
fn contrib_provider_tokens(
    id: u64,
    trait_ty: &TypeTraitObject,
) -> (TokenStream, proc_macro2::Ident, proc_macro2::Ident) {
    let (contrib_name, make_item_name, _) = make_names(id);
    let contrib_name2 = contrib_name.clone();
    let make_item_name2 = make_item_name.clone();

    let tokens = quote! {
        struct #contrib_name {
            item: ::std::sync::Arc<dyn ::std::any::Any + Send + Sync>,
            synthetic_token: ::std::string::String,
            base_token: ::std::string::String,
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Provider for #contrib_name {
            fn get_token(&self) -> ::std::string::String {
                self.synthetic_token.clone()
            }
            fn get_token_factory(&self) -> ::std::string::String {
                self.synthetic_token.clone()
            }
            fn get_scope(&self) -> ::toni::ProviderScope {
                ::toni::ProviderScope::Singleton
            }
            fn get_multi_base_token(&self) -> ::std::option::Option<::std::string::String> {
                ::std::option::Option::Some(self.base_token.clone())
            }
            fn as_multi_item(
                &self,
            ) -> ::std::option::Option<
                ::std::sync::Arc<dyn ::std::any::Any + Send + Sync>,
            > {
                ::std::option::Option::Some(self.item.clone())
            }
            async fn execute(
                &self,
                _params: ::std::vec::Vec<::std::boxed::Box<dyn ::std::any::Any + Send>>,
                _ctx: ::toni::ProviderContext<'_>,
            ) -> ::std::boxed::Box<dyn ::std::any::Any + Send> {
                ::std::boxed::Box::new(self.item.clone())
            }
        }

        fn #make_item_name(
            trait_arc: ::std::sync::Arc<#trait_ty>,
        ) -> ::std::sync::Arc<dyn ::std::any::Any + Send + Sync> {
            ::std::sync::Arc::new(trait_arc)
        }
    };

    (tokens, contrib_name2, make_item_name2)
}

/// Type-path case: `provide!(TOKEN, PluginA, multi(dyn Trait))`.
fn generate_type_multi(
    id: u64,
    base_token_expr: TokenStream,
    concrete_type: &Type,
    factory_ident: proc_macro2::Ident,
    trait_ty: &TypeTraitObject,
) -> TokenStream {
    let (contrib_tokens, contrib_name, make_item_name) = contrib_provider_tokens(id, trait_ty);
    let (_, _, factory_name) = make_names(id);

    quote! {
        {
            #contrib_tokens

            struct #factory_name;

            #[::toni::async_trait]
            impl ::toni::traits_helpers::ProviderFactory for #factory_name {
                fn get_token(&self) -> ::std::string::String {
                    format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        ::std::any::type_name::<#concrete_type>()
                    )
                }

                fn get_multi_base_token(&self) -> ::std::option::Option<::std::string::String> {
                    ::std::option::Option::Some(#base_token_expr)
                }

                fn get_dependencies(&self) -> ::std::vec::Vec<::std::string::String> {
                    #factory_ident.get_dependencies()
                }

                async fn build(
                    &self,
                    deps: ::toni::FxHashMap<
                        ::std::string::String,
                        ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>>,
                    >,
                ) -> ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>> {
                    let inner_provider = #factory_ident.build(deps).await;
                    let any_box = inner_provider
                        .execute(vec![], ::toni::ProviderContext::None)
                        .await;
                    let concrete = *any_box
                        .downcast::<#concrete_type>()
                        .unwrap_or_else(|_| panic!(
                            "Multi-provider build: downcast to {} failed",
                            ::std::any::type_name::<#concrete_type>()
                        ));
                    let trait_arc: ::std::sync::Arc<#trait_ty> =
                        ::std::sync::Arc::new(concrete);
                    let erased = #make_item_name(trait_arc);
                    let synthetic_token = format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        ::std::any::type_name::<#concrete_type>()
                    );
                    let base_token = #base_token_expr;
                    ::std::sync::Arc::new(
                        ::std::boxed::Box::new(#contrib_name { item: erased, synthetic_token, base_token })
                            as ::std::boxed::Box<dyn ::toni::traits_helpers::Provider>,
                    )
                }
            }

            #factory_name
        }
    }
}

/// Raw-value case: `provide!(TOKEN, some_expr(), multi(dyn Trait))`.
fn generate_value_multi(
    id: u64,
    base_token_expr: TokenStream,
    value_expr: &Expr,
    trait_ty: &TypeTraitObject,
) -> TokenStream {
    let (contrib_tokens, contrib_name, make_item_name) = contrib_provider_tokens(id, trait_ty);
    let (_, _, factory_name) = make_names(id);

    quote! {
        {
            #contrib_tokens

            struct #factory_name;

            #[::toni::async_trait]
            impl ::toni::traits_helpers::ProviderFactory for #factory_name {
                fn get_token(&self) -> ::std::string::String {
                    format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        concat!(file!(), ":", line!(), ":", column!())
                    )
                }

                fn get_multi_base_token(&self) -> ::std::option::Option<::std::string::String> {
                    ::std::option::Option::Some(#base_token_expr)
                }

                fn get_dependencies(&self) -> ::std::vec::Vec<::std::string::String> {
                    vec![]
                }

                async fn build(
                    &self,
                    _deps: ::toni::FxHashMap<
                        ::std::string::String,
                        ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>>,
                    >,
                ) -> ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>> {
                    let value = #value_expr;
                    let trait_arc: ::std::sync::Arc<#trait_ty> =
                        ::std::sync::Arc::new(value);
                    let erased = #make_item_name(trait_arc);
                    let synthetic_token = format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        concat!(file!(), ":", line!(), ":", column!())
                    );
                    let base_token = #base_token_expr;
                    ::std::sync::Arc::new(
                        ::std::boxed::Box::new(#contrib_name { item: erased, synthetic_token, base_token })
                            as ::std::boxed::Box<dyn ::toni::traits_helpers::Provider>,
                    )
                }
            }

            #factory_name
        }
    }
}

/// Factory-closure case: `provide!(TOKEN, || Impl::new(), multi(dyn Trait))`.
fn generate_factory_multi(
    id: u64,
    base_token_expr: TokenStream,
    closure_expr: &Expr,
    trait_ty: &TypeTraitObject,
) -> TokenStream {
    let (contrib_tokens, contrib_name, make_item_name) = contrib_provider_tokens(id, trait_ty);
    let (_, _, factory_name) = make_names(id);

    quote! {
        {
            #contrib_tokens

            struct #factory_name;

            #[::toni::async_trait]
            impl ::toni::traits_helpers::ProviderFactory for #factory_name {
                fn get_token(&self) -> ::std::string::String {
                    format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        concat!(file!(), ":", line!(), ":", column!())
                    )
                }

                fn get_multi_base_token(&self) -> ::std::option::Option<::std::string::String> {
                    ::std::option::Option::Some(#base_token_expr)
                }

                fn get_dependencies(&self) -> ::std::vec::Vec<::std::string::String> {
                    vec![]
                }

                async fn build(
                    &self,
                    _deps: ::toni::FxHashMap<
                        ::std::string::String,
                        ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>>,
                    >,
                ) -> ::std::sync::Arc<::std::boxed::Box<dyn ::toni::traits_helpers::Provider>> {
                    let factory = #closure_expr;
                    let value = factory();
                    let trait_arc: ::std::sync::Arc<#trait_ty> =
                        ::std::sync::Arc::new(value);
                    let erased = #make_item_name(trait_arc);
                    let synthetic_token = format!(
                        "__toni_multi__{}__{}",
                        #base_token_expr,
                        concat!(file!(), ":", line!(), ":", column!())
                    );
                    let base_token = #base_token_expr;
                    ::std::sync::Arc::new(
                        ::std::boxed::Box::new(#contrib_name { item: erased, synthetic_token, base_token })
                            as ::std::boxed::Box<dyn ::toni::traits_helpers::Provider>,
                    )
                }
            }

            #factory_name
        }
    }
}
