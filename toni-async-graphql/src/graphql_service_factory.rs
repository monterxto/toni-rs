use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_graphql::{ObjectType, Schema, SubscriptionType};
use async_trait::async_trait;
use std::sync::Arc;
use toni::traits_helpers::{Provider, ProviderFactory};
use toni::FxHashMap;

/// `ProviderFactory` for `GraphQLService` — registered during module scanning.
pub struct GraphQLServiceFactory<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    schema: Arc<Schema<Query, Mutation, Subscription>>,
    context_builder: Arc<Ctx>,
}

impl<Query, Mutation, Subscription, Ctx> GraphQLServiceFactory<Query, Mutation, Subscription, Ctx>
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
    for GraphQLServiceFactory<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    fn get_token(&self) -> String {
        "GraphQLService".to_string()
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        Arc::new(Box::new(GraphQLService::new(
            self.schema.clone(),
            self.context_builder.clone(),
        )) as Box<dyn Provider>)
    }
}
