//! FraiseQL Arrow Flight Client
//!
//! Native Rust client demonstrating direct Arrow Flight consumption.

use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use arrow_flight::{flight_service_client::FlightServiceClient, Ticket};
use prost::bytes::Bytes;
use serde_json::json;
use std::io::Cursor;
use tokio::sync::mpsc;
use tonic::transport::Uri;
use tracing::{error, info};

/// FraiseQL Flight client
pub struct FraiseQLFlightClient {
    uri: Uri,
}

impl FraiseQLFlightClient {
    /// Create a new client pointing to FraiseQL server
    pub fn new(host: &str, port: u16) -> Self {
        let uri = format!("http://{}:{}", host, port)
            .parse::<Uri>()
            .expect("Invalid URI");

        Self { uri }
    }

    /// Execute a GraphQL query and stream results
    pub async fn query_graphql(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<Vec<RecordBatch>, Box<dyn std::error::Error>> {
        let ticket_data = json!({
            "type": "GraphQLQuery",
            "query": query,
            "variables": variables,
        });

        let ticket = Ticket {
            ticket: Bytes::from(ticket_data.to_string().into_bytes()),
        };

        self.fetch_data(ticket).await
    }

    /// Stream observer events for an entity type
    pub async fn stream_events(
        &self,
        entity_type: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<RecordBatch>, Box<dyn std::error::Error>> {
        let ticket_data = json!({
            "type": "ObserverEvents",
            "entity_type": entity_type,
            "start_date": start_date,
            "end_date": end_date,
            "limit": limit,
        });

        let ticket = Ticket {
            ticket: Bytes::from(ticket_data.to_string().into_bytes()),
        };

        self.fetch_data(ticket).await
    }

    /// Fetch data from server
    async fn fetch_data(&self, ticket: Ticket) -> Result<Vec<RecordBatch>, Box<dyn std::error::Error>> {
        let mut client = FlightServiceClient::connect(self.uri.clone()).await?;

        info!("Requesting data from FraiseQL Flight server");

        let mut stream = client.do_get(ticket).await?.into_inner();

        let mut batches = Vec::new();
        let (tx, mut rx) = mpsc::channel(100);

        // Spawn task to receive from stream
        let recv_task = tokio::spawn(async move {
            while let Ok(Some(message)) = stream.message().await {
                if let Err(e) = tx.send(message).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }
        });

        // Process batches as they arrive
        while let Some(message) = rx.recv().await {
            let batch_data = &message.app_metadata;
            if !batch_data.is_empty() {
                let cursor = Cursor::new(batch_data.clone());
                let reader = StreamReader::try_new(cursor, None)?;

                for result in reader {
                    match result {
                        Ok(batch) => {
                            let row_count = batch.num_rows();
                            let col_count = batch.num_columns();
                            info!(
                                "Received batch with {} rows, {} columns",
                                row_count, col_count
                            );
                            batches.push(batch);
                        }
                        Err(e) => {
                            error!("Failed to read batch: {}", e);
                        }
                    }
                }
            }
        }

        // Wait for receive task to complete
        recv_task.await?;

        info!("Received {} batches total", batches.len());
        Ok(batches)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("FraiseQL Arrow Flight Client Example");
    println!("=====================================\n");

    // Create client
    let client = FraiseQLFlightClient::new("localhost", 50051);
    println!("✅ Connected to FraiseQL server at localhost:50051\n");

    // Example 1: GraphQL Query
    println!("Example 1: Execute GraphQL Query");
    println!("---------------------------------");
    match client
        .query_graphql("{ users { id name email } }", None)
        .await
    {
        Ok(batches) => {
            println!("✅ Query successful!");
            for (i, batch) in batches.iter().enumerate() {
                println!(
                    "  Batch {}: {} rows × {} columns",
                    i + 1,
                    batch.num_rows(),
                    batch.num_columns()
                );
                println!("  Schema: {:?}", batch.schema());
            }
        }
        Err(e) => eprintln!("❌ Query failed: {}", e),
    }
    println!();

    // Example 2: Stream Events
    println!("Example 2: Stream Observer Events");
    println!("---------------------------------");
    match client
        .stream_events("Order", Some("2026-01-01"), Some("2026-01-31"), Some(1000))
        .await
    {
        Ok(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            println!("✅ Events streamed successfully!");
            println!("  Total batches: {}", batches.len());
            println!("  Total rows: {}", total_rows);
        }
        Err(e) => eprintln!("❌ Event streaming failed: {}", e),
    }
    println!();

    println!("✅ Examples completed!");
    println!("=====================================");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = FraiseQLFlightClient::new("localhost", 50051);
        assert_eq!(client.uri.host(), Some("localhost"));
    }
}
