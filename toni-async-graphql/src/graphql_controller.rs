use crate::context_builder::ContextBuilder;
use crate::graphql_service::GraphQLService;
use async_graphql::{ObjectType, SubscriptionType};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use toni::traits_helpers::{Controller, ControllerFactory, Guard, Interceptor, Pipe, Provider};
use toni::{Body, FxHashMap, HttpMethod, HttpRequest, HttpResponse, ToResponse};

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
pub struct GraphQLControllerFactory<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    path: String,
    playground_enabled: bool,
    _phantom: std::marker::PhantomData<(Query, Mutation, Subscription, Ctx)>,
}

impl<Query, Mutation, Subscription, Ctx>
    GraphQLControllerFactory<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
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
impl<Query, Mutation, Subscription, Ctx> ControllerFactory
    for GraphQLControllerFactory<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
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
struct GraphQLPostController<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    path: String,
    graphql_service: Arc<Box<dyn Provider>>,
    _phantom: std::marker::PhantomData<(Query, Mutation, Subscription, Ctx)>,
}

#[async_trait]
impl<Query, Mutation, Subscription, Ctx> Controller
    for GraphQLPostController<Query, Mutation, Subscription, Ctx>
where
    Query: ObjectType + 'static,
    Mutation: ObjectType + 'static,
    Subscription: SubscriptionType + 'static,
    Ctx: ContextBuilder,
{
    fn get_token(&self) -> String {
        format!("GraphQLPostController_{}", self.path)
    }

    async fn execute(
        &self,
        req: HttpRequest,
    ) -> Box<dyn ToResponse<Response = HttpResponse> + Send> {
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
            .downcast_ref::<GraphQLService<Query, Mutation, Subscription, Ctx>>()
            .expect("Failed to downcast to GraphQLService");

        // Execute the query
        let response = service
            .execute(
                gql_request.query,
                gql_request.operation_name,
                gql_request.variables,
                &req,
            )
            .await;

        // Convert to JSON response
        let response_json = serde_json::to_value(&response).unwrap();

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
impl Controller for GraphQLPlaygroundController {
    fn get_token(&self) -> String {
        format!("GraphQLPlaygroundController_{}", self.path)
    }

    async fn execute(
        &self,
        _req: HttpRequest,
    ) -> Box<dyn ToResponse<Response = HttpResponse> + Send> {
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
        None // Playground doesn't use DTO validation
    }
}
