//! FraiseQL CLI - Schema compilation and development tools

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    fraiseql_cli::run().await;
}
