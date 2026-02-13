"""FraiseQL DDL Generation Tools.

Production-ready Python helper library for generating DDL for FraiseQL
table-backed views (tv_* and ta_*).

This package provides utilities to:
    - Load and parse FraiseQL schema.json files
    - Generate complete PostgreSQL DDL for JSON and Arrow views
    - Suggest optimal refresh strategies based on workload
    - Validate generated DDL for common issues
    - Create composition views for nested relationships

Example usage:

    from fraiseql_tools import load_schema, generate_tv_ddl, suggest_refresh_strategy

    # Load your schema
    schema = load_schema("schema.json")

    # Generate JSON view with trigger-based refresh
    tv_ddl = generate_tv_ddl(schema, entity="User", view="user")

    # Generate Arrow columnar view
    ta_ddl = generate_ta_ddl(schema, entity="User", view="user")

    # Get refresh strategy recommendation
    strategy = suggest_refresh_strategy(
        write_volume=100,
        latency_requirement_ms=500,
        read_volume=10000
    )
    print(f"Recommended refresh: {strategy}")

    # Save to file
    with open("ddl_user_view.sql", "w") as f:
        f.write(tv_ddl)
"""

from fraiseql_tools.views import (
    generate_composition_views,
    generate_ta_ddl,
    generate_tv_ddl,
    load_schema,
    suggest_refresh_strategy,
    validate_generated_ddl,
)

__version__ = "1.0.0"

__all__ = [
    "load_schema",
    "generate_tv_ddl",
    "generate_ta_ddl",
    "generate_composition_views",
    "suggest_refresh_strategy",
    "validate_generated_ddl",
]
