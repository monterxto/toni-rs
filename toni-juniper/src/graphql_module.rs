use crate::context_builder::ContextBuilder;
use crate::graphql_controller::GraphQLControllerManager;
use crate::graphql_service_manager::GraphQLServiceManager;
use juniper::{
    DefaultScalarValue, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, RootNode,
    ScalarValue,
};
use std::sync::Arc;
use toni::traits_helpers::{ControllerFactory, ModuleMetadata, ProviderFactory};

/// GraphQL module for Toni.
///
/// This module registers:
/// - GraphQLService (injectable service for executing queries)
/// - GraphQLPostController (handles POST /graphql)
/// - GraphQLGetController (handles GET /graphql - playground)
///
/// # Example
///
/// ```rust
/// use juniper::{EmptyMutation, EmptySubscription, RootNode, graphql_object};
/// use toni::{module, ToniFactory, HttpAdapter};
/// use toni_axum::AxumAdapter;
/// use toni_juniper::{GraphQLModule, DefaultContextBuilder, DefaultContext};
///
/// struct Query;
///
/// #[graphql_object(context = DefaultContext)]
/// impl Query {
///     fn hello() -> &'static str {
///         "Hello, world!"
///     }
/// }
///
/// fn build_graphql_module() -> GraphQLModule<Query, EmptyMutation<DefaultContext>, EmptySubscription<DefaultContext>, DefaultContextBuilder> {
///     let schema = RootNode::new(
///         Query,
///         EmptyMutation::new(),
///         EmptySubscription::new(),
///     );
///     GraphQLModule::for_root(schema, DefaultContextBuilder)
/// }
///
/// #[module(
///     imports: [build_graphql_module()],
///     controllers: [],
///     providers: [],
///     exports: []
/// )]
/// impl AppModule {}
/// ```
pub struct GraphQLModule<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
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
    path: String,
    playground: bool,
}

impl<Query, Mutation, Subscription, Ctx, S> GraphQLModule<Query, Mutation, Subscription, Ctx, S>
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
    /// Create a new GraphQL module with the given schema and context builder.
    ///
    /// # Arguments
    ///
    /// * `schema` - Juniper RootNode defining Query, Mutation, and Subscription types
    /// * `context_builder` - User-defined context builder (can inject Toni services!)
    ///
    /// # Returns
    ///
    /// A new GraphQLModule with default configuration:
    /// - Path: `/graphql`
    /// - Playground: Enabled in debug builds, disabled in release builds
    pub fn for_root(
        schema: RootNode<'static, Query, Mutation, Subscription, S>,
        context_builder: Ctx,
    ) -> Self {
        Self {
            schema: Arc::new(schema),
            context_builder: Arc::new(context_builder),
            path: "/graphql".to_string(),
            playground: cfg!(debug_assertions),
        }
    }

    /// Set the GraphQL endpoint path.
    ///
    /// Default: `/graphql`
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Enable or disable GraphQL Playground.
    ///
    /// Default: Enabled in debug builds, disabled in release builds
    pub fn with_playground(mut self, enabled: bool) -> Self {
        self.playground = enabled;
        self
    }
}

impl<Query, Mutation, Subscription, Ctx, S> ModuleMetadata
    for GraphQLModule<Query, Mutation, Subscription, Ctx, S>
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
        Some(vec![Box::new(GraphQLServiceManager::new(
            self.schema.clone(),
            self.context_builder.clone(),
        ))])
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        Some(vec![Box::new(GraphQLControllerManager::<
            Query,
            Mutation,
            Subscription,
            Ctx,
            S,
        >::new(
            self.path.clone(), self.playground
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
