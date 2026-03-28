use crate::context_builder::ContextBuilder;
use async_graphql::{ObjectType, Schema, SubscriptionType};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use toni::traits_helpers::{ProviderContext, Provider};
use toni::{ProviderScope, RequestPart};

/// Injectable GraphQL service that executes GraphQL queries.
///
/// This service is automatically provided by the `GraphQLModule` and can be
/// injected into controllers or other services.
#[derive(Clone)]
pub struct GraphQLService<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    schema: Arc<Schema<Query, Mutation, Subscription>>,
    context_builder: Arc<Ctx>,
}

impl<Query, Mutation, Subscription, Ctx> GraphQLService<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    /// Create a new GraphQL service.
    pub fn new(
        schema: Arc<Schema<Query, Mutation, Subscription>>,
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
    /// 1. Builds the context using the context builder
    /// 2. Parses the GraphQL request
    /// 3. Executes the query against the schema
    /// 4. Returns the response
    ///
    /// # Arguments
    ///
    /// * `query` - The GraphQL query string
    /// * `operation_name` - Optional operation name
    /// * `variables` - Optional variables as JSON
    /// * `http_req` - The HTTP request (used for context building)
    pub async fn execute(
        &self,
        query: String,
        operation_name: Option<String>,
        variables: Option<Value>,
        http_req: &RequestPart,
    ) -> async_graphql::Response {
        let context_data = self.context_builder.build(http_req).await;

        // Build GraphQL request
        let mut request = async_graphql::Request::new(query);

        if let Some(op_name) = operation_name {
            request = request.operation_name(op_name);
        }

        if let Some(vars) = variables {
            let variables = async_graphql::Variables::from_json(vars);
            request = request.variables(variables);
        }

        // Set context data directly on the request
        // Note: We assign directly to request.data instead of using .data() method
        // because .data() expects individual items, not a Data container
        request.data = context_data;

        // Execute query
        self.schema.execute(request).await
    }

    /// Get reference to the schema.
    pub fn schema(&self) -> &Schema<Query, Mutation, Subscription> {
        &self.schema
    }
}

// Implement Provider to make it injectable
#[async_trait]
impl<Query, Mutation, Subscription, Ctx> Provider
    for GraphQLService<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        //Box::new(self.clone())

        let service: GraphQLService<Query, Mutation, Subscription, Ctx> = GraphQLService {
            schema: self.schema.clone(),
            context_builder: self.context_builder.clone(),
        };
        Box::new(service)
    }

    fn get_token(&self) -> String {
        "GraphQLService".to_string()
    }

    fn get_token_factory(&self) -> String {
        "GraphQLServiceFactory".to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        // Schema is singleton - created once and shared
        ProviderScope::Singleton
    }
}
