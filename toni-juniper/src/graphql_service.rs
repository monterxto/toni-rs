use crate::context_builder::ContextBuilder;
use async_trait::async_trait;
use juniper::{
    DefaultScalarValue, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, RootNode,
    ScalarValue, Variables,
};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use toni::traits_helpers::{Provider, ProviderContext};
use toni::RequestPart;

/// Injectable GraphQL service.
///
/// This service holds the Juniper schema and context builder.
/// It can be injected into controllers or other services.
pub struct GraphQLService<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
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

impl<Query, Mutation, Subscription, Ctx, S> GraphQLService<Query, Mutation, Subscription, Ctx, S>
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

    /// Execute a GraphQL query.
    ///
    /// This method:
    /// 1. Builds context from HTTP request (calling user's ContextBuilder)
    /// 2. Parses variables from JSON
    /// 3. Executes the query via Juniper
    /// 4. Returns response as JSON Value
    pub async fn execute(
        &self,
        query: String,
        operation_name: Option<String>,
        variables: Option<Value>,
        http_req: &RequestPart,
    ) -> Value {
        let context = self.context_builder.build(http_req).await;

        // Parse variables
        let vars: Variables<S> = variables
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Execute query using Juniper's execute function
        match juniper::execute(
            &query,
            operation_name.as_deref(),
            &*self.schema,
            &vars,
            &context,
        )
        .await
        {
            Ok((result, errors)) => {
                // Success - return data and execution errors
                let mut response = serde_json::json!({
                    "data": result,
                });

                if !errors.is_empty() {
                    response["errors"] = serde_json::json!(errors);
                }

                response
            }
            Err(e) => {
                // Parse error - return error in GraphQL format
                serde_json::json!({
                    "errors": [{
                        "message": format!("{}", e)
                    }]
                })
            }
        }
    }

    pub fn schema(&self) -> &RootNode<'static, Query, Mutation, Subscription, S> {
        &self.schema
    }
}

impl<Query, Mutation, Subscription, Ctx, S> Clone
    for GraphQLService<Query, Mutation, Subscription, Ctx, S>
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
    fn clone(&self) -> Self {
        Self {
            schema: self.schema.clone(),
            context_builder: self.context_builder.clone(),
        }
    }
}

impl<Query, Mutation, Subscription, Ctx, S> fmt::Debug
    for GraphQLService<Query, Mutation, Subscription, Ctx, S>
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphQLService")
            .field("schema", &"RootNode")
            .field("context_builder", &"ContextBuilder")
            .finish()
    }
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx, S> Provider
    for GraphQLService<Query, Mutation, Subscription, Ctx, S>
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
    fn get_token(&self) -> String {
        "GraphQLService".to_string()
    }

    fn get_token_factory(&self) -> String {
        "GraphQLServiceFactory".to_string()
    }

    async fn execute(
        &self,
        _deps: Vec<Box<dyn std::any::Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn std::any::Any + Send> {
        let service: GraphQLService<Query, Mutation, Subscription, Ctx, S> = GraphQLService {
            schema: self.schema.clone(),
            context_builder: self.context_builder.clone(),
        };
        Box::new(service)
    }
}
