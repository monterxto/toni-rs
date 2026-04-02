use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_trait::async_trait;
use juniper::{
    DefaultScalarValue, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, ScalarValue,
};
use serde::Deserialize;
use std::sync::Arc;
use toni::traits_helpers::{Controller, ControllerFactory, Guard, Interceptor, Pipe, Provider};
use toni::{http_helpers::Body, FxHashMap, HttpMethod, HttpRequest, HttpResponse};

/// GraphQL request payload
#[derive(Debug, Deserialize)]
struct GraphQLRequest {
    query: String,
    #[serde(rename = "operationName")]
    operation_name: Option<String>,
    variables: Option<serde_json::Value>,
}

/// `ControllerFactory` for GraphQL endpoints.
///
/// This creates two endpoints:
/// - POST /graphql - Execute GraphQL queries
/// - GET /graphql - Serve GraphQL Playground (if enabled)
pub struct GraphQLControllerFactory<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
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
    path: String,
    playground_enabled: bool,
    _phantom: std::marker::PhantomData<(Query, Mutation, Subscription, Ctx, S)>,
}

impl<Query, Mutation, Subscription, Ctx, S>
    GraphQLControllerFactory<Query, Mutation, Subscription, Ctx, S>
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
    pub fn new(path: String, playground_enabled: bool) -> Self {
        Self {
            path,
            playground_enabled,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx, S> ControllerFactory
    for GraphQLControllerFactory<Query, Mutation, Subscription, Ctx, S>
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
        format!("GraphQLController_{}", self.path)
    }

    fn get_dependencies(&self) -> Vec<String> {
        vec!["GraphQLService".to_string()]
    }

    async fn build(
        &self,
        dependencies: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Vec<Arc<Box<dyn Controller>>> {
        let mut controllers = Vec::new();

        let graphql_service = dependencies
            .get("GraphQLService")
            .expect("GraphQLService not found in dependencies")
            .clone();

        controllers.push(Arc::new(Box::new(GraphQLPostController::<
            Query,
            Mutation,
            Subscription,
            Ctx,
            S,
        > {
            path: self.path.clone(),
            graphql_service: graphql_service.clone(),
            _phantom: std::marker::PhantomData,
        }) as Box<dyn Controller>));

        if self.playground_enabled {
            controllers.push(Arc::new(Box::new(GraphQLPlaygroundController {
                path: self.path.clone(),
                playground_html: include_str!("playground.html").to_string(),
            }) as Box<dyn Controller>));
        }

        controllers
    }
}

/// POST controller for executing GraphQL queries
struct GraphQLPostController<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
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
    path: String,
    graphql_service: Arc<Box<dyn Provider>>,
    _phantom: std::marker::PhantomData<(Query, Mutation, Subscription, Ctx, S)>,
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx, S> Controller
    for GraphQLPostController<Query, Mutation, Subscription, Ctx, S>
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
        format!("GraphQLPostController_{}", self.path)
    }

    async fn execute(&self, req: HttpRequest) -> HttpResponse {
        let (parts, body) = req.into_parts();
        let body_bytes = match body.collect().await {
            Ok(b) => b,
            Err(e) => {
                return HttpResponse {
                    status: 400,
                    body: Some(Body::json(serde_json::json!({
                        "errors": [{"message": format!("Failed to read request body: {}", e)}]
                    }))),
                    headers: vec![],
                };
            }
        };

        // Parse GraphQL request from body
        let gql_request: GraphQLRequest = match serde_json::from_slice(&body_bytes) {
            Ok(req) => req,
            Err(e) => {
                return HttpResponse {
                    status: 400,
                    body: Some(Body::json(serde_json::json!({
                        "errors": [{"message": format!("Invalid GraphQL request: {}", e)}]
                    }))),
                    headers: vec![],
                };
            }
        };

        let service_any = self
            .graphql_service
            .execute(vec![], toni::ProviderContext::Http(&parts))
            .await;

        let service = service_any
            .downcast_ref::<GraphQLService<Query, Mutation, Subscription, Ctx, S>>()
            .expect("Failed to downcast to GraphQLService");

        let response_json = service
            .execute(
                gql_request.query,
                gql_request.operation_name,
                gql_request.variables,
                &parts,
            )
            .await;

        HttpResponse {
            status: 200,
            body: Some(Body::json(response_json)),
            headers: vec![],
        }
    }

    fn get_path(&self) -> String {
        self.path.clone()
    }

    fn get_method(&self) -> HttpMethod {
        HttpMethod::POST
    }

    fn get_guards(&self) -> Vec<Arc<dyn Guard>> {
        vec![]
    }

    fn get_pipes(&self) -> Vec<Arc<dyn Pipe>> {
        vec![]
    }

    fn get_interceptors(&self) -> Vec<Arc<dyn Interceptor>> {
        vec![]
    }

    fn get_body_dto(
        &self,
        _req: &toni::RequestPart,
    ) -> Option<Box<dyn toni::traits_helpers::validate::Validatable>> {
        None // GraphQL doesn't use DTO validation (uses GraphQL schema validation)
    }
}

/// GET controller for serving GraphQL Playground
struct GraphQLPlaygroundController {
    path: String,
    playground_html: String,
}

#[async_trait]
impl Controller for GraphQLPlaygroundController {
    fn get_token(&self) -> String {
        format!("GraphQLPlaygroundController_{}", self.path)
    }

    async fn execute(&self, _req: HttpRequest) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: Some(Body::text(self.playground_html.clone())),
            headers: vec![],
        }
    }

    fn get_path(&self) -> String {
        self.path.clone()
    }

    fn get_method(&self) -> HttpMethod {
        HttpMethod::GET
    }

    fn get_guards(&self) -> Vec<Arc<dyn Guard>> {
        vec![]
    }

    fn get_pipes(&self) -> Vec<Arc<dyn Pipe>> {
        vec![]
    }

    fn get_interceptors(&self) -> Vec<Arc<dyn Interceptor>> {
        vec![]
    }

    fn get_body_dto(
        &self,
        _req: &toni::RequestPart,
    ) -> Option<Box<dyn toni::traits_helpers::validate::Validatable>> {
        None
    }
}
