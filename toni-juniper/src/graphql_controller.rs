use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_trait::async_trait;
use juniper::{
    DefaultScalarValue, GraphQLSubscriptionType, GraphQLType, GraphQLTypeAsync, ScalarValue,
};
use serde::Deserialize;
use std::sync::Arc;
use toni::traits_helpers::{Controller, ControllerTrait, Guard, Interceptor, Pipe, ProviderTrait};
use toni::{Body, FxHashMap, HttpMethod, HttpRequest, HttpResponse, IntoResponse};

/// GraphQL request payload
#[derive(Debug, Deserialize)]
struct GraphQLRequest {
    query: String,
    #[serde(rename = "operationName")]
    operation_name: Option<String>,
    variables: Option<serde_json::Value>,
}

/// Controller manager for GraphQL endpoints.
///
/// This creates two endpoints:
/// - POST /graphql - Execute GraphQL queries
/// - GET /graphql - Serve GraphQL Playground (if enabled)
pub struct GraphQLControllerManager<Query, Mutation, Subscription, Ctx, S = DefaultScalarValue>
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
    GraphQLControllerManager<Query, Mutation, Subscription, Ctx, S>
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
impl<Query, Mutation, Subscription, Ctx, S> Controller
    for GraphQLControllerManager<Query, Mutation, Subscription, Ctx, S>
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
    async fn get_all_controllers(
        &self,
        dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ControllerTrait>>> {
        let mut controllers = FxHashMap::default();

        // Get GraphQLService from dependencies
        let graphql_service = dependencies
            .get("GraphQLService")
            .expect("GraphQLService not found in dependencies")
            .clone();

        // POST endpoint for executing queries
        let post_controller = GraphQLPostController::<Query, Mutation, Subscription, Ctx, S> {
            path: self.path.clone(),
            graphql_service: graphql_service.clone(),
            _phantom: std::marker::PhantomData,
        };

        controllers.insert(
            format!("GraphQLPostController_{}", self.path),
            Arc::new(Box::new(post_controller) as Box<dyn ControllerTrait>),
        );

        // GET endpoint for playground (if enabled)
        if self.playground_enabled {
            let get_controller = GraphQLPlaygroundController {
                path: self.path.clone(),
                playground_html: include_str!("playground.html").to_string(),
            };

            controllers.insert(
                format!("GraphQLPlaygroundController_{}", self.path),
                Arc::new(Box::new(get_controller) as Box<dyn ControllerTrait>),
            );
        }

        controllers
    }

    fn get_name(&self) -> String {
        "GraphQLControllerManager".to_string()
    }

    fn get_token(&self) -> String {
        format!("GraphQLController_{}", self.path)
    }

    fn get_dependencies(&self) -> Vec<String> {
        vec!["GraphQLService".to_string()]
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
    graphql_service: Arc<Box<dyn ProviderTrait>>,
    _phantom: std::marker::PhantomData<(Query, Mutation, Subscription, Ctx, S)>,
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx, S> ControllerTrait
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

    async fn execute(
        &self,
        req: HttpRequest,
    ) -> Box<dyn IntoResponse<Response = HttpResponse> + Send> {
        // Parse GraphQL request from body
        let gql_request: GraphQLRequest = match &req.body {
            Body::Json(json) => match serde_json::from_value(json.clone()) {
                Ok(req) => req,
                Err(e) => {
                    return Box::new(HttpResponse {
                        status: 400,
                        body: Some(Body::Json(serde_json::json!({
                            "errors": [{
                                "message": format!("Invalid GraphQL request: {}", e)
                            }]
                        }))),
                        headers: vec![("content-type".to_string(), "application/json".to_string())],
                    });
                }
            },
            Body::Text(text) => match serde_json::from_str(text) {
                Ok(req) => req,
                Err(e) => {
                    return Box::new(HttpResponse {
                        status: 400,
                        body: Some(Body::Json(serde_json::json!({
                            "errors": [{
                                "message": format!("Invalid GraphQL request: {}", e)
                            }]
                        }))),
                        headers: vec![("content-type".to_string(), "application/json".to_string())],
                    });
                }
            },
            Body::Binary(_) => {
                return Box::new(HttpResponse {
                    status: 400,
                    body: Some(Body::Json(serde_json::json!({
                        "errors": [{
                            "message": "GraphQL requests must be JSON or text, not binary"
                        }]
                    }))),
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                });
            }
        };

        // Get the GraphQLService
        let service_any = self.graphql_service.execute(vec![], Some(&req)).await;

        let service = service_any
            .downcast_ref::<GraphQLService<Query, Mutation, Subscription, Ctx, S>>()
            .expect("Failed to downcast to GraphQLService");

        // Execute the query
        let response_json = service
            .execute(
                gql_request.query,
                gql_request.operation_name,
                gql_request.variables,
                &req,
            )
            .await;

        Box::new(HttpResponse {
            status: 200,
            body: Some(Body::Json(response_json)),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
        })
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
        _req: &HttpRequest,
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
impl ControllerTrait for GraphQLPlaygroundController {
    fn get_token(&self) -> String {
        format!("GraphQLPlaygroundController_{}", self.path)
    }

    async fn execute(
        &self,
        _req: HttpRequest,
    ) -> Box<dyn IntoResponse<Response = HttpResponse> + Send> {
        Box::new(HttpResponse {
            status: 200,
            body: Some(Body::Text(self.playground_html.clone())),
            headers: vec![("content-type".to_string(), "text/html".to_string())],
        })
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
        _req: &HttpRequest,
    ) -> Option<Box<dyn toni::traits_helpers::validate::Validatable>> {
        None
    }
}
