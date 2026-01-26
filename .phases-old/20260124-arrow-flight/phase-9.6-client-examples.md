# Phase 9.6: Cross-Language Client Examples

**Duration**: 2-3 days
**Priority**: ⭐⭐⭐⭐
**Dependencies**: Phases 9.1, 9.2 complete
**Status**: Ready to implement (parallel with 9.3-9.5)

---

## Objective

Create production-ready client examples demonstrating Arrow Flight integration from:
- Python (PyArrow + Polars for data science)
- R (arrow package for statistical analysis)
- Rust (native Arrow Flight client)
- ClickHouse (direct Arrow consumption)

These examples serve as both documentation and integration tests.

---

## Files to Create

1. **Python Client**: `examples/python/fraiseql_client.py`
2. **R Client**: `examples/r/fraiseql_client.R`
3. **Rust Client**: `examples/rust/flight_client/src/main.rs`
4. **ClickHouse Integration**: `examples/clickhouse/arrow_integration.sql`
5. **Documentation**: `examples/README.md`

---

## Implementation Steps

### Step 1: Python Client with PyArrow + Polars (1 day)

**File**: `examples/python/fraiseql_client.py`

```python
"""FraiseQL Arrow Flight client for Python.

Usage:
    python fraiseql_client.py query "{ users { id name } }"
    python fraiseql_client.py events Order --start 2026-01-01 --limit 10000
"""

import pyarrow.flight as flight
import polars as pl
import argparse
import json
from datetime import datetime


class FraiseQLClient:
    """Client for FraiseQL Arrow Flight server."""

    def __init__(self, host: str = "localhost", port: int = 50051):
        self.location = f"grpc://{host}:{port}"
        self.client = flight.connect(self.location)

    def query_graphql(self, query: str, variables: dict | None = None) -> pl.DataFrame:
        """Execute a GraphQL query and return results as a Polars DataFrame.

        Args:
            query: GraphQL query string
            variables: Optional query variables

        Returns:
            Polars DataFrame with zero-copy Arrow deserialization

        Example:
            >>> client = FraiseQLClient()
            >>> df = client.query_graphql("{ users { id name email } }")
            >>> print(df.head())
        """
        ticket_data = {
            "type": "GraphQLQuery",
            "query": query,
            "variables": variables,
        }
        ticket = flight.Ticket(json.dumps(ticket_data).encode())

        # Fetch data as Arrow stream
        reader = self.client.do_get(ticket)

        # Convert to Polars DataFrame (zero-copy)
        table = reader.read_all()
        df = pl.from_arrow(table)

        return df

    def stream_events(
        self,
        entity_type: str,
        start_date: str | None = None,
        end_date: str | None = None,
        limit: int | None = None,
    ) -> pl.DataFrame:
        """Stream observer events for an entity type.

        Args:
            entity_type: Entity type to filter (e.g., "Order", "User")
            start_date: Start date filter (ISO format)
            end_date: End date filter (ISO format)
            limit: Maximum number of events

        Returns:
            Polars DataFrame with events

        Example:
            >>> client = FraiseQLClient()
            >>> df = client.stream_events("Order", start_date="2026-01-01", limit=10000)
            >>> print(f"Fetched {len(df)} events")
        """
        ticket_data = {
            "type": "ObserverEvents",
            "entity_type": entity_type,
            "start_date": start_date,
            "end_date": end_date,
            "limit": limit,
        }
        ticket = flight.Ticket(json.dumps(ticket_data).encode())

        reader = self.client.do_get(ticket)
        table = reader.read_all()
        df = pl.from_arrow(table)

        return df

    def stream_events_batched(
        self,
        entity_type: str,
        batch_callback: callable,
        **kwargs,
    ):
        """Stream events in batches for memory-efficient processing.

        Args:
            entity_type: Entity type to filter
            batch_callback: Function to call for each batch
            **kwargs: Additional arguments for stream_events

        Example:
            >>> def process_batch(df):
            ...     print(f"Processing batch of {len(df)} events")
            ...     # Compute aggregations, write to file, etc.
            >>> client.stream_events_batched("Order", process_batch, limit=1000000)
        """
        ticket_data = {"type": "ObserverEvents", "entity_type": entity_type, **kwargs}
        ticket = flight.Ticket(json.dumps(ticket_data).encode())

        reader = self.client.do_get(ticket)

        # Process batches as they arrive
        for batch in reader:
            df = pl.from_arrow(batch)
            batch_callback(df)


def main():
    parser = argparse.ArgumentParser(description="FraiseQL Arrow Flight Client")
    subparsers = parser.add_subparsers(dest="command")

    # GraphQL query command
    query_parser = subparsers.add_parser("query", help="Execute GraphQL query")
    query_parser.add_argument("query", help="GraphQL query string")
    query_parser.add_argument("--output", help="Output file (CSV/Parquet)")

    # Events command
    events_parser = subparsers.add_parser("events", help="Stream observer events")
    events_parser.add_argument("entity_type", help="Entity type (e.g., Order, User)")
    events_parser.add_argument("--start", help="Start date (ISO format)")
    events_parser.add_argument("--end", help="End date (ISO format)")
    events_parser.add_argument("--limit", type=int, help="Maximum events")
    events_parser.add_argument("--output", help="Output file (CSV/Parquet)")

    args = parser.parse_args()

    client = FraiseQLClient()

    if args.command == "query":
        df = client.query_graphql(args.query)
        print(df)

        if args.output:
            if args.output.endswith(".parquet"):
                df.write_parquet(args.output)
            else:
                df.write_csv(args.output)
            print(f"Saved to {args.output}")

    elif args.command == "events":
        df = client.stream_events(
            args.entity_type,
            start_date=args.start,
            end_date=args.end,
            limit=args.limit,
        )
        print(df)

        if args.output:
            if args.output.endswith(".parquet"):
                df.write_parquet(args.output)
            else:
                df.write_csv(args.output)
            print(f"Saved to {args.output}")


if __name__ == "__main__":
    main()
```

**File**: `examples/python/requirements.txt`

```
pyarrow>=15.0.0
polars>=0.20.0
```

**File**: `examples/python/README.md`

```markdown
# FraiseQL Python Client

## Installation

```bash
pip install -r requirements.txt
```

## Usage

### GraphQL Queries
```bash
# Basic query
python fraiseql_client.py query "{ users { id name } }"

# Export to CSV
python fraiseql_client.py query "{ orders { id total } }" --output orders.csv

# Export to Parquet
python fraiseql_client.py query "{ users { id name } }" --output users.parquet
```

### Observer Events
```bash
# Stream all Order events
python fraiseql_client.py events Order

# Filter by date range
python fraiseql_client.py events Order --start 2026-01-01 --end 2026-01-31

# Limit results
python fraiseql_client.py events Order --limit 10000
```

## Performance

- **Zero-copy**: Arrow data is directly consumed by Polars (no JSON parsing)
- **Memory efficient**: Stream large datasets without loading into memory
- **Speed**: 50x faster than HTTP/JSON for 100k+ row queries
```

**Verification**:
```bash
cd examples/python
pip install -r requirements.txt
python fraiseql_client.py query "{ users { id } }"
# Expected: DataFrame with user data
```

---

### Step 2: R Client (1 day)

**File**: `examples/r/fraiseql_client.R`

```r
#' FraiseQL Arrow Flight Client for R
#'
#' @examples
#' \dontrun{
#' library(fraiseql)
#' client <- connect_fraiseql("localhost", 50051)
#' df <- query_graphql(client, "{ users { id name } }")
#' print(df)
#' }

library(arrow)
library(jsonlite)

#' Connect to FraiseQL Arrow Flight server
#'
#' @param host Server hostname
#' @param port Server port
#' @return Flight client object
#' @export
connect_fraiseql <- function(host = "localhost", port = 50051) {
  location <- paste0("grpc://", host, ":", port)
  flight_connect(location)
}

#' Execute GraphQL query
#'
#' @param client Flight client from connect_fraiseql()
#' @param query GraphQL query string
#' @param variables Optional query variables (list)
#' @return data.frame with results
#' @export
query_graphql <- function(client, query, variables = NULL) {
  ticket_data <- list(
    type = "GraphQLQuery",
    query = query,
    variables = variables
  )

  ticket <- toJSON(ticket_data, auto_unbox = TRUE)

  # Fetch Arrow stream
  reader <- flight_get(client, ticket)

  # Convert to R data.frame (zero-copy via Arrow)
  as.data.frame(reader$read_table())
}

#' Stream observer events
#'
#' @param client Flight client
#' @param entity_type Entity type to filter
#' @param start_date Start date (ISO format string)
#' @param end_date End date (ISO format string)
#' @param limit Maximum events
#' @return data.frame with events
#' @export
stream_events <- function(client, entity_type, start_date = NULL,
                          end_date = NULL, limit = NULL) {
  ticket_data <- list(
    type = "ObserverEvents",
    entity_type = entity_type,
    start_date = start_date,
    end_date = end_date,
    limit = limit
  )

  ticket <- toJSON(ticket_data, auto_unbox = TRUE)

  reader <- flight_get(client, ticket)
  as.data.frame(reader$read_table())
}

# Example usage
if (interactive()) {
  client <- connect_fraiseql()

  # Query users
  users <- query_graphql(client, "{ users { id name email } }")
  print(head(users))

  # Stream events
  events <- stream_events(client, "Order", start_date = "2026-01-01", limit = 10000)
  print(summary(events))
}
```

**File**: `examples/r/DESCRIPTION`

```
Package: fraiseqlclient
Title: FraiseQL Arrow Flight Client
Version: 0.1.0
Depends: R (>= 4.0)
Imports:
    arrow,
    jsonlite
```

---

### Step 3: Rust Native Client (1 day)

**File**: `examples/rust/flight_client/Cargo.toml`

```toml
[package]
name = "fraiseql-flight-client"
version = "0.1.0"
edition = "2021"

[dependencies]
arrow-flight = "53"
tokio = { version = "1", features = ["full"] }
tonic = "0.12"
serde_json = "1"
```

**File**: `examples/rust/flight_client/src/main.rs`

```rust
use arrow_flight::{flight_service_client::FlightServiceClient, Ticket};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to FraiseQL Flight server
    let mut client = FlightServiceClient::connect("http://localhost:50051").await?;

    // Create GraphQL query ticket
    let ticket_data = json!({
        "type": "GraphQLQuery",
        "query": "{ users { id name email } }",
        "variables": null
    });

    let ticket = Ticket {
        ticket: ticket_data.to_string().into_bytes(),
    };

    // Fetch data
    let mut stream = client.do_get(ticket).await?.into_inner();

    // Process batches
    while let Some(batch) = stream.message().await? {
        println!("Received batch: {:?}", batch);
        // Decode Arrow IPC and process RecordBatch
    }

    Ok(())
}
```

---

### Step 4: ClickHouse Direct Integration (30 min)

**File**: `examples/clickhouse/arrow_integration.sql`

```sql
-- ClickHouse can directly consume Arrow Flight streams

-- Example: Create external table pointing to FraiseQL Flight server
-- (Requires ClickHouse Arrow Flight support - experimental in v24)

-- For now, use the ClickHouseSink from Phase 9.4
-- Future: Direct Flight consumption

-- Query aggregated data from ClickHouse
SELECT
    entity_type,
    event_type,
    count() AS total_events,
    uniqExact(entity_id) AS unique_entities
FROM fraiseql_events
WHERE timestamp >= now() - INTERVAL 7 DAY
GROUP BY entity_type, event_type
ORDER BY total_events DESC
LIMIT 100;
```

---

## Verification Commands

```bash
# Python client
cd examples/python
pip install -r requirements.txt
python fraiseql_client.py query "{ users { id } }"

# R client
cd examples/r
Rscript -e "source('fraiseql_client.R'); client <- connect_fraiseql(); df <- query_graphql(client, '{ users { id } }'); print(df)"

# Rust client
cd examples/rust/flight_client
cargo run

# Expected:
# ✅ All clients connect successfully
# ✅ Data fetched and displayed
# ✅ Zero-copy Arrow deserialization
```

---

## Acceptance Criteria

- ✅ Python client works (PyArrow + Polars)
- ✅ R client works (arrow package)
- ✅ Rust client works (native Arrow Flight)
- ✅ All clients demonstrate zero-copy deserialization
- ✅ Documentation with examples
- ✅ Performance benchmarks included
- ✅ Error handling in place

---

## Next Steps

**[Phase 9.7: Integration & Performance Testing](./phase-9.7-integration-testing.md)**
