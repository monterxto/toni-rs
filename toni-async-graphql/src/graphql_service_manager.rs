use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_graphql::{ObjectType, Schema, SubscriptionType};
use async_trait::async_trait;
use std::sync::Arc;
use toni::traits_helpers::{Provider, ProviderFactory};
use toni::FxHashMap;

/// ProviderFactory manager for GraphQLService.
///
/// This follows Toni's two-tier provider pattern:
/// - Manager (implements ProviderFactory) - registered during module scanning
/// - Service (implements Provider) - actual injectable instance
pub struct GraphQLServiceManager<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    schema: Arc<Schema<Query, Mutation, Subscription>>,
    context_builder: Arc<Ctx>,
}

impl<Query, Mutation, Subscription, Ctx> GraphQLServiceManager<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    pub fn new(
        schema: Arc<Schema<Query, Mutation, Subscription>>,
        context_builder: Arc<Ctx>,
    ) -> Self {
        Self {
            schema,
            context_builder,
        }
    }
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx> ProviderFactory
    for GraphQLServiceManager<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
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
