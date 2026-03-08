use crate::context_builder::ContextBuilder;
use crate::graphql_controller::GraphQLControllerFactory;
use crate::graphql_service_factory::GraphQLServiceFactory;
use async_graphql::{ObjectType, Schema, SubscriptionType};
use std::sync::Arc;
use toni::traits_helpers::{ControllerFactory, ModuleMetadata, ProviderFactory};

/// GraphQL module for integrating async-graphql with Toni.
///
/// This module provides GraphQL functionality to your Toni application.
/// It registers:
/// - A `GraphQLService` provider (injectable)
/// - GraphQL endpoints (POST for queries, GET for playground)
///
/// # Examples
///
/// ## Basic usage
///
/// ```rust
/// use toni_async_graphql::{GraphQLModule, DefaultContextBuilder, async_graphql::*};
///
/// struct Query;
///
/// #[Object]
/// impl Query {
///     async fn hello(&self) -> &str {
///         "Hello, world!"
///     }
/// }
///
/// let schema = Schema::build(Query, EmptyMutation, EmptySubscription).finish();
/// let graphql_module = GraphQLModule::for_root(schema, DefaultContextBuilder);
/// ```
///
/// ## With custom context builder
///
/// ```ignore
/// use toni_async_graphql::{GraphQLModule, ContextBuilder, async_graphql::*};
/// use toni::HttpRequest;
/// use async_trait::async_trait;
///
/// struct MyContextBuilder {
///     // ... injected services
/// }
///
/// #[async_trait]
/// impl ContextBuilder for MyContextBuilder {
///     async fn build(&self, req: &HttpRequest) -> Data {
///         // Build context from request
///         let mut data = Data::default();
///         data.insert(req.clone());
///         data
///     }
/// }
///
/// let schema = Schema::build(Query, EmptyMutation, EmptySubscription).finish();
/// let graphql_module = GraphQLModule::for_root(schema, MyContextBuilder { /* ... */ });
/// ```
pub struct GraphQLModule<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    schema: Arc<Schema<Query, Mutation, Subscription>>,
    context_builder: Arc<Ctx>,
    path: String,
    playground_enabled: bool,
}

impl<Query, Mutation, Subscription, Ctx> GraphQLModule<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    /// Create a GraphQL module with a schema and context builder.
    ///
    /// # Arguments
    ///
    /// * `schema` - The async-graphql schema
    /// * `context_builder` - Your context builder implementation
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni_async_graphql::{GraphQLModule, DefaultContextBuilder, async_graphql::*};
    ///
    /// struct Query;
    ///
    /// #[Object]
    /// impl Query {
    ///   async fn hello(&self) -> &str {
    ///    "Hello, world!"
    ///   }
    /// }
    ///
    /// let schema = Schema::build(Query, EmptyMutation, EmptySubscription).finish();
    /// let module = GraphQLModule::for_root(schema, DefaultContextBuilder);
    /// ```
    pub fn for_root(schema: Schema<Query, Mutation, Subscription>, context_builder: Ctx) -> Self {
        Self {
            schema: Arc::new(schema),
            context_builder: Arc::new(context_builder),
            path: "/graphql".to_string(),
            playground_enabled: cfg!(debug_assertions), // Enabled in debug mode by default
        }
    }

    /// Set the GraphQL endpoint path.
    ///
    /// Default: `/graphql`
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let module = GraphQLModule::for_root(schema, context_builder)
    ///     .with_path("/api/graphql");
    /// ```
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Enable or disable GraphQL Playground.
    ///
    /// Default: Enabled in debug builds, disabled in release builds.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let module = GraphQLModule::for_root(schema, context_builder)
    ///     .with_playground(true);  // Always enable
    /// ```
    pub fn with_playground(mut self, enabled: bool) -> Self {
        self.playground_enabled = enabled;
        self
    }

    /// Get the GraphQL endpoint path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Check if playground is enabled.
    pub fn playground_enabled(&self) -> bool {
        self.playground_enabled
    }
}

impl<Query, Mutation, Subscription, Ctx> ModuleMetadata
    for GraphQLModule<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    fn get_id(&self) -> String {
        format!(
            "GraphQLModule<{},{},{}>",
            std::any::type_name::<Query>(),
            std::any::type_name::<Mutation>(),
            std::any::type_name::<Subscription>(),
        )
    }

    fn get_name(&self) -> String {
        "GraphQLModule".to_string()
    }

    fn providers(&self) -> Option<Vec<Box<dyn ProviderFactory>>> {
        Some(vec![Box::new(GraphQLServiceFactory::new(
            self.schema.clone(),
            self.context_builder.clone(),
        ))])
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        Some(vec![Box::new(GraphQLControllerFactory::<
            Query,
            Mutation,
            Subscription,
            Ctx,
        >::new(
            self.path.clone(), self.playground_enabled
        ))])
    }

    fn exports(&self) -> Option<Vec<String>> {
        // Export GraphQLService so other modules can inject it
        Some(vec!["GraphQLService".to_string()])
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }
}
