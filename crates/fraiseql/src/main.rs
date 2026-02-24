//! FraiseQL CLI entry point — delegates to fraiseql-cli.

#[tokio::main]
async fn main() {
    fraiseql_cli::run().await;
}
