#!/usr/bin/env python3
"""Basic example of DDL generation for simple User entity.

This example demonstrates:
    - Loading a schema from JSON
    - Generating a table-backed JSON view (tv_user)
    - Validating the generated DDL
    - Outputting to SQL file
"""

import sys
from pathlib import Path

# Add tools to path
tools_path = Path(__file__).parent.parent.parent / "tools"
sys.path.insert(0, str(tools_path))

from fraiseql_tools import (
    generate_tv_ddl,
    load_schema,
    validate_generated_ddl,
)

def main():
    """Generate DDL for User entity."""
    # Load schema
    schema_path = Path(__file__).parent / "test_schemas" / "user.json"
    print(f"Loading schema from {schema_path.absolute()}")
    schema = load_schema(str(schema_path))

    print(f"✓ Schema loaded successfully")
    print(f"  Version: {schema['version']}")
    print(f"  Types: {[t['name'] for t in schema['types']]}")
    print()

    # Generate JSON view DDL
    print("Generating DDL for tv_user (JSON materialized view)...")
    tv_ddl = generate_tv_ddl(
        schema,
        entity="User",
        view="user",
        refresh_strategy="trigger-based",
        include_composition_views=False,
        include_monitoring_functions=True,
    )

    print(f"✓ TV_USER DDL generated ({len(tv_ddl)} bytes)")
    print()

    # Validate generated DDL
    print("Validating generated DDL...")
    errors = validate_generated_ddl(tv_ddl)

    if errors:
        print("⚠ Validation warnings:")
        for error in errors:
            print(f"  - {error}")
    else:
        print("✓ DDL validation passed (no issues detected)")
    print()

    # Output to file
    output_path = Path(__file__).parent / "ddl_user_view.sql"
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(tv_ddl)

    print(f"✓ DDL written to {output_path.absolute()}")
    print()

    # Show a sample of the generated DDL
    lines = tv_ddl.split("\n")
    print("Generated DDL (first 30 lines):")
    print("-" * 80)
    for line in lines[:30]:
        print(line)
    print("-" * 80)
    print(f"... ({len(lines) - 30} more lines)")


if __name__ == "__main__":
    main()
