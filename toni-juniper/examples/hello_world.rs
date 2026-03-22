use juniper::{graphql_object, EmptyMutation, EmptySubscription, RootNode};
use toni::{module, toni_factory::ToniFactory, HttpAdapter};
use toni_axum::AxumAdapter;
use toni_juniper::{DefaultContext, DefaultContextBuilder, GraphQLModule};

// Define Query type
struct Query;

#[graphql_object(context = DefaultContext)]
impl Query {
    /// Simple hello world query
    fn hello() -> &'static str {
        "Hello, world!"
    }

    /// Query with arguments
    fn greet(name: String) -> String {
        format!("Hello, {}!", name)
    }

    /// Query that returns a custom type
    fn user(id: i32) -> User {
        User {
            id,
            name: format!("User {}", id),
            email: format!("user{}@example.com", id),
        }
    }
}

// Define a custom GraphQL type
#[derive(Clone)]
struct User {
    id: i32,
    name: String,
    email: String,
}

#[graphql_object(context = DefaultContext)]
impl User {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn email(&self) -> &str {
        &self.email
    }
}

fn build_graphql_module() -> GraphQLModule<
    Query,
    EmptyMutation<DefaultContext>,
    EmptySubscription<DefaultContext>,
    DefaultContextBuilder,
> {
    let schema = RootNode::new(Query, EmptyMutation::new(), EmptySubscription::new());
    GraphQLModule::for_root(schema, DefaultContextBuilder)
        .with_path("/graphql")
        .with_playground(true)
}

// Define the app module
#[module(
    imports: [build_graphql_module()],
    controllers: [],
    providers: [],
    exports: []
)]
impl AppModule {}

#[tokio::main]
async fn main() {
    println!("Starting GraphQL Hello World example...");
    println!("GraphQL endpoint: http://localhost:3000/graphql");
    println!("GraphQL Playground: http://localhost:3000/graphql (open in browser)");

    // Create Toni app
    let mut app = ToniFactory::create(AppModule::module_definition()).await;

    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000)).unwrap();

    app.start().await;
}
