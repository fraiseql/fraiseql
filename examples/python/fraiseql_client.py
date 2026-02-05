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
        batch_callback,
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
