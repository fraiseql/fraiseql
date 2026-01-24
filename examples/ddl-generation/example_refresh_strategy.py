#!/usr/bin/env python3
"""Refresh strategy suggestion example.

This example demonstrates:
    - Analyzing workload characteristics
    - Getting recommendations for refresh strategy
    - Generating DDL with optimal strategy
"""

import sys
from pathlib import Path

# Add tools to path
tools_path = Path(__file__).parent.parent.parent / "tools"
sys.path.insert(0, str(tools_path))

from fraiseql_tools import (
    generate_tv_ddl,
    load_schema,
    suggest_refresh_strategy,
)

def main():
    """Demonstrate refresh strategy selection."""
    schema_path = Path(__file__).parent / "test_schemas" / "user_with_posts.json"
    schema = load_schema(str(schema_path))

    print("FraiseQL DDL Generation - Refresh Strategy Selection")
    print("=" * 80)
    print()

    # Scenario 1: High-read, low-write OLTP
    print("Scenario 1: Blog Platform (Read-Heavy OLTP)")
    print("-" * 80)
    write_vol_1 = 10  # Few new posts per minute
    latency_1 = 100  # Must show latest posts within 100ms
    read_vol_1 = 5000  # Thousands of page views per minute
    strategy_1 = suggest_refresh_strategy(write_vol_1, latency_1, read_vol_1)
    print(f"  Write volume: {write_vol_1} writes/min")
    print(f"  Latency requirement: {latency_1}ms")
    print(f"  Read volume: {read_vol_1} reads/min")
    print(f"  ➜ Suggested strategy: {strategy_1}")
    print()

    # Scenario 2: Batch import system
    print("Scenario 2: Data Warehouse (Batch Import)")
    print("-" * 80)
    write_vol_2 = 5000  # Bulk imports
    latency_2 = 3600000  # OK with hourly updates
    read_vol_2 = 100  # Few analytical queries
    strategy_2 = suggest_refresh_strategy(write_vol_2, latency_2, read_vol_2)
    print(f"  Write volume: {write_vol_2} writes/min")
    print(f"  Latency requirement: {latency_2}ms (1 hour)")
    print(f"  Read volume: {read_vol_2} reads/min")
    print(f"  ➜ Suggested strategy: {strategy_2}")
    print()

    # Scenario 3: Moderate balanced workload
    print("Scenario 3: Balanced Workload")
    print("-" * 80)
    write_vol_3 = 100
    latency_3 = 500
    read_vol_3 = 1000
    strategy_3 = suggest_refresh_strategy(write_vol_3, latency_3, read_vol_3)
    print(f"  Write volume: {write_vol_3} writes/min")
    print(f"  Latency requirement: {latency_3}ms")
    print(f"  Read volume: {read_vol_3} reads/min")
    print(f"  ➜ Suggested strategy: {strategy_3}")
    print()

    # Generate DDL with suggested strategy
    print("Generating User view with suggested refresh strategy...")
    print("-" * 80)
    tv_ddl = generate_tv_ddl(
        schema,
        entity="User",
        view="user",
        refresh_strategy=strategy_1,
        include_composition_views=True,
        include_monitoring_functions=True,
    )

    print(f"✓ DDL generated with {strategy_1} refresh")
    print(f"  Size: {len(tv_ddl)} bytes")
    print()

    # Summary
    print("Summary")
    print("=" * 80)
    print("""
The refresh strategy recommendation considers:
  - Read-to-write ratio: High reads + low writes → trigger-based
  - Latency requirements: Strict (<100ms) → trigger-based
  - Write patterns: Bulk (>1000/min) → scheduled

Use trigger-based when:
  ✓ Latency is critical (real-time data needed)
  ✓ Read-heavy workloads (many queries, few updates)
  ✓ Low write volume (<10 writes/sec)
  ✓ Small payloads per update

Use scheduled when:
  ✓ Bulk imports or batch operations
  ✓ Acceptable staleness window (30min+)
  ✓ High write volume (>1000 writes/min)
  ✓ Limited trigger overhead acceptable
""")


if __name__ == "__main__":
    main()
