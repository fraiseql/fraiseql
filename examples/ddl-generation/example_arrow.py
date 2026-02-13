#!/usr/bin/env python3
"""Arrow view generation example.

This example demonstrates:
    - Generating a table-backed Arrow columnar view (ta_order)
    - Scheduled refresh strategy for high-volume workloads
    - Monitoring and health check functions
"""

import sys
from pathlib import Path

# Add tools to path
tools_path = Path(__file__).parent.parent.parent / "tools"
sys.path.insert(0, str(tools_path))

from fraiseql_tools import (
    generate_ta_ddl,
    load_schema,
    validate_generated_ddl,
)

def main():
    """Generate DDL for Order entity with Arrow columnar storage."""
    # Load schema with relationships
    schema_path = Path(__file__).parent / "test_schemas" / "orders.json"
    print(f"Loading schema from {schema_path.absolute()}")
    schema = load_schema(str(schema_path))

    print(f"✓ Schema loaded successfully")
    print(f"  Version: {schema['version']}")
    print(f"  Types: {[t['name'] for t in schema['types']]}")
    print()

    # Generate Arrow view DDL
    print("Generating DDL for ta_order (Arrow columnar materialized view)...")
    ta_ddl = generate_ta_ddl(
        schema,
        entity="Order",
        view="order",
        refresh_strategy="scheduled",
        include_monitoring_functions=True,
    )

    print(f"✓ TA_ORDER DDL generated ({len(ta_ddl)} bytes)")
    print()

    # Validate generated DDL
    print("Validating generated DDL...")
    errors = validate_generated_ddl(ta_ddl)

    if errors:
        print("⚠ Validation warnings:")
        for error in errors:
            print(f"  - {error}")
    else:
        print("✓ DDL validation passed (no issues detected)")
    print()

    # Output to file
    output_path = Path(__file__).parent / "ddl_order_arrow_view.sql"
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(ta_ddl)

    print(f"✓ DDL written to {output_path.absolute()}")
    print()

    # Show key sections
    print("Generated DDL structure:")
    print("-" * 80)
    for line in ta_ddl.split("\n"):
        if "CREATE" in line or "COMMENT ON" in line or "-- " in line:
            print(line[:80])
    print("-" * 80)


if __name__ == "__main__":
    main()
