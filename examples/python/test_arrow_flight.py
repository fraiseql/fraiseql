#!/usr/bin/env python3
"""
Arrow Flight client example for FraiseQL.

This script demonstrates how to query FraiseQL's ta_* materialized tables
via the Arrow Flight protocol and convert results to Polars DataFrames.

# Prerequisites

Install dependencies:
    pip install pyarrow polars

# Usage

Start FraiseQL server with Arrow Flight:
    cargo run --release --features arrow

Then run this script:
    python examples/python/test_arrow_flight.py

# Expected Output

The script will:
1. Connect to Arrow Flight server on localhost:50051
2. Request ta_users table schema
3. Fetch user data as Arrow RecordBatches
4. Convert to Polars DataFrame
5. Display results
"""

import json
import sys

import pyarrow.flight as flight


def test_arrow_flight():
    """Test Arrow Flight connection to FraiseQL server."""
    print("🚀 FraiseQL Arrow Flight Client Test")
    print("=" * 50)

    # Connect to Arrow Flight server
    print("\n📡 Connecting to Arrow Flight server...")
    try:
        client = flight.connect("grpc://localhost:50051")
        print("✓ Connected to grpc://localhost:50051")
    except Exception as e:
        print(f"✗ Failed to connect: {e}")
        print("\n💡 Make sure FraiseQL server is running with Arrow Flight:")
        print("   cargo run --release --features arrow")
        return False

    # Test 1: Query ta_users
    print("\n📊 Test 1: Query ta_users table")
    print("-" * 50)
    try:
        # Create ticket for ta_users query
        ticket_data = {
            "type": "OptimizedView",
            "view": "ta_users",
            "limit": 10,
        }
        ticket = flight.Ticket(json.dumps(ticket_data).encode())

        # Execute query
        print(f"Executing query: {ticket_data}")
        reader = client.do_get(ticket)
        table = reader.read_all()

        print(f"✓ Received {len(table)} rows from ta_users")
        print(f"✓ Schema: {table.schema}")

        # Display first few rows
        if len(table) > 0:
            print(f"\nFirst {min(5, len(table))} rows:")
            print(table.slice(0, 5))
        else:
            print("⚠ No rows returned (table may be empty)")

    except Exception as e:
        print(f"✗ Query failed: {e}")
        import traceback

        traceback.print_exc()
        return False

    # Test 2: Query ta_orders
    print("\n📊 Test 2: Query ta_orders table")
    print("-" * 50)
    try:
        ticket_data = {
            "type": "OptimizedView",
            "view": "ta_orders",
            "limit": 10,
        }
        ticket = flight.Ticket(json.dumps(ticket_data).encode())

        print(f"Executing query: {ticket_data}")
        reader = client.do_get(ticket)
        table = reader.read_all()

        print(f"✓ Received {len(table)} rows from ta_orders")
        print(f"✓ Schema: {table.schema}")

        if len(table) > 0:
            print(f"\nFirst {min(5, len(table))} rows:")
            print(table.slice(0, 5))
        else:
            print("⚠ No rows returned (table may be empty)")

    except Exception as e:
        print(f"✗ Query failed: {e}")
        import traceback

        traceback.print_exc()
        return False

    # Test 3: Convert to Polars (if available)
    print("\n🔄 Test 3: Convert to Polars DataFrame")
    print("-" * 50)
    try:
        import polars as pl

        # Query ta_users again for Polars conversion
        ticket_data = {
            "type": "OptimizedView",
            "view": "ta_users",
            "limit": 100,
        }
        ticket = flight.Ticket(json.dumps(ticket_data).encode())
        reader = client.do_get(ticket)
        table = reader.read_all()

        df = pl.from_arrow(table)
        print(f"✓ Converted to Polars DataFrame with shape {df.shape}")
        print("\nDataFrame preview:")
        print(df)

    except ImportError:
        print("⚠ Polars not installed (optional)")
        print("  Install with: pip install polars")
    except Exception as e:
        print(f"✗ Conversion failed: {e}")
        import traceback

        traceback.print_exc()
        return False

    print("\n" + "=" * 50)
    print("✓ All tests passed!")
    print("\n💡 Next steps:")
    print("   - Verify data matches ta_users and ta_orders tables")
    print("   - Test with filters: add 'filter' to ticket_data")
    print("   - Test with ORDER BY: add 'order_by' to ticket_data")
    print("   - Test pagination: adjust 'limit' and 'offset'")

    return True


if __name__ == "__main__":
    success = test_arrow_flight()
    sys.exit(0 if success else 1)
