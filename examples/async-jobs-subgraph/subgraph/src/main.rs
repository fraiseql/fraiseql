//! Async-jobs federation subgraph.
//!
//! A minimal Apollo Federation v2 subgraph that handles a *non-SQL* mutation:
//! it accepts work, returns a job handle immediately, and runs the work in the
//! background. It is composed alongside a FraiseQL (SQL-backed) subgraph by a
//! federation router so clients see one unified GraphQL API.
//!
//! See `examples/async-jobs-subgraph/README.md` and
//! `docs/guides/non-sql-mutations.md` for the why and the full request flow.

mod store;

use async_graphql::http::GraphiQLSource;
use async_graphql::{Context, EmptySubscription, Object, Schema, ID};
use async_graphql_axum::GraphQL;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use store::{JobHandle, JobStore};

/// Root query type.
struct Query;

#[Object]
impl Query {
    /// Poll the status of a previously enqueued job.
    async fn job_status(&self, ctx: &Context<'_>, id: ID) -> Option<JobHandle> {
        ctx.data_unchecked::<JobStore>().get(id.as_str())
    }

    /// Federation entity resolver: lets a router resolve a `JobHandle`
    /// reference (`@key(fields: "id")`) that originated in another subgraph.
    #[graphql(entity)]
    async fn find_job_handle_by_id(&self, ctx: &Context<'_>, id: ID) -> Option<JobHandle> {
        ctx.data_unchecked::<JobStore>().get(id.as_str())
    }
}

/// Root mutation type.
struct Mutation;

#[Object]
impl Mutation {
    /// Enqueue a unit of async work and return a handle immediately.
    ///
    /// This is the non-SQL mutation FraiseQL itself cannot express. The work
    /// here is a toy "uppercase after 2s"; in production it would be an HTTP
    /// call, ML inference, payment request, etc.
    async fn enqueue_job(&self, ctx: &Context<'_>, input: String) -> JobHandle {
        ctx.data_unchecked::<JobStore>().enqueue(input)
    }
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

async fn health() -> impl IntoResponse {
    "ok"
}

#[tokio::main]
async fn main() {
    let store = JobStore::new();
    let schema = Schema::build(Query, Mutation, EmptySubscription)
        .data(store)
        .enable_federation()
        .finish();

    let app = Router::new()
        .route("/graphql", get(graphiql).post_service(GraphQL::new(schema)))
        .route("/health", get(health));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4001);
    let addr = format!("0.0.0.0:{port}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    println!("async-jobs subgraph listening on http://{addr}/graphql");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| panic!("server error: {e}"));
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
