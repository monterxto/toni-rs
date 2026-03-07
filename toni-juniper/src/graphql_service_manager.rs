use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_trait::async_trait;
use juniper::{
    DefaultScalarValue, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, RootNode,
    ScalarValue,
};
use std::sync::Arc;
use toni::traits_helpers::{Provider, ProviderFactory};
use toni::FxHashMap;

/// ProviderFactory manager for GraphQLService.
///
/// This follows Toni's two-tier provider pattern:
/// - Manager (implements ProviderFactory) - registered during module scanning
/// - Service (implements Provider) - actual injectable instance
pub struct GraphQLServiceManager<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
where
    Query: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Mutation: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Subscription: GraphQLType<S, Context = Ctx::Context>
        + GraphQLSubscriptionType<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Ctx: ContextBuilder,
    Ctx::Context: Send + Sync,
    S: ScalarValue + Send + Sync + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation::TypeInfo: Send + Sync,
    Subscription::TypeInfo: Send + Sync,
{
    schema: Arc<RootNode<'static, Query, Mutation, Subscription, S>>,
    context_builder: Arc<Ctx>,
}

impl<Query, Mutation, Subscription, Ctx, S>
    GraphQLServiceManager<Query, Mutation, Subscription, Ctx, S>
where
    Query: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Mutation: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Subscription: GraphQLType<S, Context = Ctx::Context>
        + GraphQLSubscriptionType<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Ctx: ContextBuilder,
    Ctx::Context: Send + Sync,
    S: ScalarValue + Send + Sync + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation::TypeInfo: Send + Sync,
    Subscription::TypeInfo: Send + Sync,
{
    pub fn new(
        schema: Arc<RootNode<'static, Query, Mutation, Subscription, S>>,
        context_builder: Arc<Ctx>,
    ) -> Self {
        Self {
            schema,
            context_builder,
        }
    }
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx, S> ProviderFactory
    for GraphQLServiceManager<Query, Mutation, Subscription, Ctx, S>
where
    Query: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Mutation: GraphQLType<S, Context = Ctx::Context>
        + GraphQLTypeAsync<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Subscription: GraphQLType<S, Context = Ctx::Context>
        + GraphQLSubscriptionType<S, Context = Ctx::Context>
        + Send
        + Sync
        + 'static,
    Ctx: ContextBuilder,
    Ctx::Context: Send + Sync,
    S: ScalarValue + Send + Sync + 'static,
    Query::TypeInfo: Send + Sync,
    Mutation::TypeInfo: Send + Sync,
    Subscription::TypeInfo: Send + Sync,
{
    async fn get_all_providers(
        &self,
        _dependencies: &FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> FxHashMap<String, Arc<Box<dyn Provider>>> {
        let service = GraphQLService::new(self.schema.clone(), self.context_builder.clone());

        let mut providers = FxHashMap::default();
        providers.insert(
            "GraphQLService".to_string(),
            Arc::new(Box::new(service) as Box<dyn Provider>),
        );

        providers
    }

    fn get_name(&self) -> String {
        "GraphQLServiceManager".to_string()
    }

    fn get_token(&self) -> String {
        "GraphQLService".to_string()
    }

    fn get_dependencies(&self) -> Vec<String> {
        // No dependencies - schema and context builder are already provided
        vec![]
    }
}
