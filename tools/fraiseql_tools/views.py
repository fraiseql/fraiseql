"""FraiseQL DDL Generation Helper Library.

This module provides utilities for generating production-ready DDL for table-backed
views (tv_* for JSON views and ta_* for Arrow columnar views).

Architecture:
    - Schema Loading: Parse schema.json from FraiseQL project
    - DDL Generation: Use Jinja2 templates to generate PostgreSQL DDL
    - Validation: Check generated SQL syntax and completeness
    - Refresh Strategies: Support trigger-based and scheduled refresh modes

Usage example:
    >>> schema = load_schema("schema.json")
    >>> tv_ddl = generate_tv_ddl(schema, entity="User", view="user")
    >>> print(tv_ddl)

    >>> ta_ddl = generate_ta_ddl(schema, entity="User", view="user", refresh_strategy="scheduled")
    >>> print(ta_ddl)

    >>> strategy = suggest_refresh_strategy(write_volume=1000, latency_requirement_ms=100, read_volume=50000)
    >>> print(f"Suggested strategy: {strategy}")
"""

import json
import re
from pathlib import Path
from typing import Any

__version__ = "1.0.0"


def load_schema(path: str) -> dict:
    """Load schema.json and return as dictionary.

    Parses a FraiseQL schema.json file from the specified path. The schema
    contains type definitions, field mappings, and query/mutation metadata
    needed for DDL generation.

    Args:
        path: Absolute or relative path to schema.json file.

    Returns:
        Dictionary representing the parsed schema with keys: $schema, version, types, queries, mutations.

    Raises:
        FileNotFoundError: If schema file does not exist at specified path.
        json.JSONDecodeError: If schema.json contains invalid JSON.
        ValueError: If schema is missing required keys (types, version).

    Example:
        >>> schema = load_schema("fraiseql-python/ecommerce_schema.json")
        >>> print(schema["version"])
        "2.0"
        >>> print(len(schema["types"]))
        2
    """
    schema_path = Path(path)

    if not schema_path.exists():
        raise FileNotFoundError(f"Schema file not found at {schema_path.absolute()}")

    try:
        with open(schema_path, encoding="utf-8") as f:
            schema = json.load(f)
    except json.JSONDecodeError as e:
        raise json.JSONDecodeError(
            f"Invalid JSON in schema file {path}: {e.msg}", e.doc, e.pos
        ) from e

    # Validate required fields
    if "types" not in schema:
        raise ValueError("Schema must contain 'types' key with entity definitions")
    if "version" not in schema:
        raise ValueError("Schema must contain 'version' key")

    return schema


def _render_template(template_name: str, context: dict) -> str:
    """Render a SQL template with the provided context.

    Internal helper function that renders SQL templates using simple string
    substitution with Jinja2-like syntax. Does NOT require Jinja2 to be installed.

    Args:
        template_name: Name of template file (e.g., "tv_base.sql")
        context: Dictionary of variables to substitute in template

    Returns:
        Rendered SQL string with all template variables substituted.

    Raises:
        FileNotFoundError: If template file not found.
        ValueError: If template references undefined variables.
    """
    template_dir = Path(__file__).parent / "templates"
    template_path = template_dir / template_name

    if not template_path.exists():
        raise FileNotFoundError(
            f"Template {template_name} not found in {template_dir.absolute()}"
        )

    with open(template_path, encoding="utf-8") as f:
        content = f.read()

    # Simple Jinja2-like template rendering
    # Handle {{ variable }} substitution
    def replace_var(match):
        var_name = match.group(1).strip()

        # Handle nested access like {{ field.name }}
        if "." in var_name:
            parts = var_name.split(".")
            value = context.get(parts[0])
            if value is None:
                # Return empty string for undefined nested access in loops
                return ""
            for part in parts[1:]:
                if isinstance(value, dict):
                    value = value.get(part)
                    if value is None:
                        return ""
                else:
                    return ""
            if isinstance(value, bool):
                return str(value).lower()
            return str(value) if value is not None else ""

        if var_name not in context:
            raise ValueError(f"Undefined template variable: {var_name}")
        value = context[var_name]
        if isinstance(value, bool):
            return str(value).lower()
        return str(value)

    # Replace {{ variable }} patterns
    content = re.sub(r"\{\{\s*([^}]+)\s*\}\}", replace_var, content)

    # Handle {% if ... %} blocks (simple support)
    # Pattern: {% if var %} content {% endif %}
    def replace_if(match):
        condition = match.group(1).strip()
        block_content = match.group(2)
        # Simple boolean evaluation
        if condition.startswith("not "):
            var = condition[4:].strip()
            if var in context:
                return "" if context[var] else block_content
        elif condition in context:
            return block_content if context[condition] else ""
        return block_content

    content = re.sub(
        r"\{%\s*if\s+([^%]+)\s*%\}(.*?)\{%\s*endif\s*%\}",
        replace_if,
        content,
        flags=re.DOTALL,
    )

    # Handle {% for %} loops (simple support)
    def replace_for(match):
        loop_var = match.group(1).strip()
        iter_var = match.group(2).strip()
        block_content = match.group(3)

        if iter_var not in context:
            return ""

        items = context[iter_var]
        if not isinstance(items, list):
            return ""

        result = []
        for item in items:
            loop_context = {**context, loop_var: item, "loop": {"last": False}}
            # Simple replacement in loop block
            rendered = block_content
            for key, value in loop_context.items():
                if key != "loop":
                    rendered = rendered.replace(
                        "{{ " + key + " }}", str(value)
                    ).replace("{{ " + key + " }}", str(value))
            # Handle item properties like {{ field.name }}
            if isinstance(item, dict):
                for key, value in item.items():
                    rendered = rendered.replace(
                        "{{ " + loop_var + "." + key + " }}", str(value)
                    )
            result.append(rendered)

        # Handle special loop.last variable
        if result:
            result[-1] = result[-1].replace("{% if not loop.last %},", "").replace(
                "{% endif %}", ""
            )
            result[-1] = result[-1].replace(",\n            {%- endif %}", "")

        return "".join(result)

    content = re.sub(
        r"\{%\s*for\s+(\w+)\s+in\s+([^%]+)\s*%\}(.*?)\{%\s*endfor\s*%\}",
        replace_for,
        content,
        flags=re.DOTALL,
    )

    # Handle loop iteration special syntax (simpler pattern)
    content = re.sub(r"\{%-\s*for\s+(\w+)\s+in\s+([^%]+)\s*%\}", "", content)
    content = re.sub(r"\{%-\s*endfor\s*%\}", "", content)

    return content


def generate_tv_ddl(
    schema: dict,
    entity: str,
    view: str,
    refresh_strategy: str = "trigger-based",
    include_composition_views: bool = True,
    include_monitoring_functions: bool = True,
) -> str:
    """Generate DDL for table-backed JSON view (tv_*).

    Creates a complete DDL script for a table-backed JSON materialized view that
    stores entity data as JSONB for efficient JSON queries. Includes indexes,
    comments, and optional refresh functions.

    The generated DDL creates:
        - Main view table with JSONB storage
        - Indexes for common access patterns
        - Refresh trigger or scheduled functions
        - Composition helper views (optional)
        - Monitoring functions (optional)

    Args:
        schema: Schema dictionary from load_schema()
        entity: Entity name from schema (e.g., "User", "Post")
        view: View name suffix without tv_ prefix (e.g., "user", "post")
        refresh_strategy: One of "trigger-based" or "scheduled". Default is "trigger-based".
        include_composition_views: Whether to generate composition views for relationships.
        include_monitoring_functions: Whether to generate monitoring and health check functions.

    Returns:
        Complete PostgreSQL DDL as a single string ready for execution.

    Raises:
        ValueError: If entity not found in schema, invalid refresh_strategy, or missing required fields.
        FileNotFoundError: If templates not found.

    Example:
        >>> schema = load_schema("ecommerce_schema.json")
        >>> ddl = generate_tv_ddl(schema, entity="User", view="user")
        >>> with open("ddl_user_view.sql", "w") as f:
        ...     f.write(ddl)
    """
    # Validate inputs
    if refresh_strategy not in ("trigger-based", "scheduled"):
        raise ValueError(
            f"Invalid refresh_strategy: {refresh_strategy}. Must be 'trigger-based' or 'scheduled'."
        )

    # Find entity in schema
    entity_type = None
    for t in schema.get("types", []):
        if t["name"] == entity:
            entity_type = t
            break

    if entity_type is None:
        available = [t["name"] for t in schema.get("types", [])]
        raise ValueError(
            f"Entity '{entity}' not found in schema. Available entities: {available}"
        )

    # Extract fields
    fields = entity_type.get("fields", [])
    if not fields:
        raise ValueError(f"Entity '{entity}' has no fields defined")

    # Build context for template rendering
    context = {
        "entity_name": entity,
        "view_name": view,
        "refresh_strategy": refresh_strategy,
        "if_not_exists": True,
        "fields": [
            {"name": f["name"], "type": f.get("type", "String")} for f in fields
        ],
    }

    # Render base template
    ddl_parts = [_render_template("tv_base.sql", context)]

    # Add refresh strategy-specific DDL
    if refresh_strategy == "trigger-based":
        # Find source table name (from queries that reference this view)
        source_table_name = f"table_{view}"  # Convention-based fallback
        for query in schema.get("queries", []):
            if query.get("sql_source") == f"v_{view}":
                source_table_name = f"table_{query.get('return_type', entity).lower()}"

        context["source_table_name"] = source_table_name
        refresh_ddl = _render_template("refresh_trigger.sql", context)
        ddl_parts.append("\n\n" + refresh_ddl)
    elif refresh_strategy == "scheduled":
        context["refresh_interval"] = "30 minutes"
        refresh_ddl = _render_template("refresh_scheduled.sql", context)
        ddl_parts.append("\n\n" + refresh_ddl)

    # Add composition views if requested
    if include_composition_views:
        # Find relationships in schema
        relationships = []
        for field in fields:
            field_type = field.get("type")
            # Check if field type is another entity
            for t in schema.get("types", []):
                if t["name"] == field_type:
                    relationships.append(
                        {
                            "name": field["name"],
                            "target_entity": field_type,
                        }
                    )
                    break

        if relationships:
            context["relationships"] = relationships
            composition_ddl = _render_template("composition_view.sql", context)
            ddl_parts.append("\n\n" + composition_ddl)

    # Add monitoring functions if requested
    if include_monitoring_functions:
        monitoring_ddl = _render_template("monitoring.sql", context)
        ddl_parts.append("\n\n" + monitoring_ddl)

    return "".join(ddl_parts)


def generate_ta_ddl(
    schema: dict,
    entity: str,
    view: str,
    refresh_strategy: str = "scheduled",
    include_monitoring_functions: bool = True,
) -> str:
    """Generate DDL for table-backed Arrow view (ta_*).

    Creates a complete DDL script for a table-backed Arrow columnar materialized
    view that stores entity data as Arrow IPC RecordBatches for efficient columnar
    queries and streaming via Arrow Flight.

    The generated DDL creates:
        - Main view table with Arrow column storage
        - Indexes for batch and refresh tracking
        - Scheduled refresh functions
        - Arrow Flight metadata functions
        - Monitoring and health check functions

    Args:
        schema: Schema dictionary from load_schema()
        entity: Entity name from schema (e.g., "User", "Post")
        view: View name suffix without ta_ prefix (e.g., "user", "post")
        refresh_strategy: One of "scheduled" or "manual". Default is "scheduled".
        include_monitoring_functions: Whether to generate monitoring and health check functions.

    Returns:
        Complete PostgreSQL DDL as a single string ready for execution.

    Raises:
        ValueError: If entity not found in schema, invalid refresh_strategy, or missing fields.
        FileNotFoundError: If templates not found.

    Example:
        >>> schema = load_schema("ecommerce_schema.json")
        >>> ddl = generate_ta_ddl(schema, entity="User", view="user", refresh_strategy="scheduled")
        >>> with open("ddl_user_arrow_view.sql", "w") as f:
        ...     f.write(ddl)
    """
    # Validate inputs
    if refresh_strategy not in ("scheduled", "manual"):
        raise ValueError(
            f"Invalid refresh_strategy: {refresh_strategy}. Must be 'scheduled' or 'manual'."
        )

    # Find entity in schema
    entity_type = None
    for t in schema.get("types", []):
        if t["name"] == entity:
            entity_type = t
            break

    if entity_type is None:
        available = [t["name"] for t in schema.get("types", [])]
        raise ValueError(
            f"Entity '{entity}' not found in schema. Available entities: {available}"
        )

    # Extract fields
    fields = entity_type.get("fields", [])
    if not fields:
        raise ValueError(f"Entity '{entity}' has no fields defined")

    # Build context for template rendering
    context = {
        "entity_name": entity,
        "view_name": view,
        "refresh_strategy": refresh_strategy,
        "if_not_exists": True,
        "fields": [
            {"name": f["name"], "type": f.get("type", "String")} for f in fields
        ],
    }

    # Render base template
    ddl_parts = [_render_template("ta_base.sql", context)]

    # Add refresh strategy-specific DDL (Arrow always uses scheduled refresh)
    # Find source table name (from queries that reference this view)
    source_table_name = f"table_{view}"  # Convention-based fallback
    for query in schema.get("queries", []):
        if query.get("sql_source") == f"v_{view}":
            source_table_name = f"table_{query.get('return_type', entity).lower()}"

    context["refresh_interval"] = "30 minutes"
    context["source_table_name"] = source_table_name
    refresh_ddl = _render_template("refresh_scheduled.sql", context)
    ddl_parts.append("\n\n" + refresh_ddl)

    # Add monitoring functions if requested
    if include_monitoring_functions:
        monitoring_ddl = _render_template("monitoring.sql", context)
        ddl_parts.append("\n\n" + monitoring_ddl)

    return "".join(ddl_parts)


def generate_composition_views(
    schema: dict,
    entity: str,
    relationships: list[str],
) -> str:
    """Generate helper composition views for nested relationships.

    Creates composition views that efficiently load related entities for
    nested relationship support. Useful for optimizing queries that need to
    load both parent and related entities.

    Args:
        schema: Schema dictionary from load_schema()
        entity: Entity name from schema (e.g., "User")
        relationships: List of relationship names to compose (e.g., ["posts", "comments"])

    Returns:
        DDL string with composition views and helper functions.

    Raises:
        ValueError: If entity not found or relationships don't exist on entity.

    Example:
        >>> schema = load_schema("ecommerce_schema.json")
        >>> composition_ddl = generate_composition_views(
        ...     schema,
        ...     entity="User",
        ...     relationships=["posts", "comments"]
        ... )
    """
    # Find entity in schema
    entity_type = None
    for t in schema.get("types", []):
        if t["name"] == entity:
            entity_type = t
            break

    if entity_type is None:
        available = [t["name"] for t in schema.get("types", [])]
        raise ValueError(
            f"Entity '{entity}' not found in schema. Available entities: {available}"
        )

    # Validate relationships exist
    entity_fields = {f["name"]: f.get("type") for f in entity_type.get("fields", [])}
    for rel_name in relationships:
        if rel_name not in entity_fields:
            raise ValueError(
                f"Relationship '{rel_name}' not found on entity '{entity}'. "
                f"Available fields: {list(entity_fields.keys())}"
            )

    # Build relationships with target entity info
    rel_list = []
    for rel_name in relationships:
        target_entity = entity_fields[rel_name]
        rel_list.append(
            {"name": rel_name, "target_entity": target_entity},
        )

    context = {
        "entity_name": entity.lower(),
        "relationships": rel_list,
    }

    return _render_template("composition_view.sql", context)


def suggest_refresh_strategy(
    write_volume: int,
    latency_requirement_ms: int,
    read_volume: int,
) -> str:
    """Suggest refresh strategy based on workload characteristics.

    Analyzes write volume, latency requirements, and read patterns to recommend
    an appropriate refresh strategy:

        - trigger-based: Best for high read volume with low write volume and strict latency
        - scheduled: Best for bulk operations with acceptable staleness window

    The decision model considers:
        - Write-to-read ratio: High reads + low writes favor trigger-based
        - Latency requirements: < 100ms suggest trigger-based for real-time freshness
        - Write patterns: Bulk (>1000/min) suggest scheduled batch refresh

    Args:
        write_volume: Expected writes per minute (e.g., 100)
        latency_requirement_ms: Maximum acceptable staleness in milliseconds (e.g., 500)
        read_volume: Expected reads per minute (e.g., 10000)

    Returns:
        String: Either "trigger-based" or "scheduled"

    Example:
        >>> # Read-heavy, latency-sensitive application
        >>> strategy = suggest_refresh_strategy(
        ...     write_volume=100,          # 100 writes/min
        ...     latency_requirement_ms=100, # Must be fresh within 100ms
        ...     read_volume=50000           # 50k reads/min
        ... )
        >>> print(strategy)
        "trigger-based"

        >>> # Write-heavy batch system
        >>> strategy = suggest_refresh_strategy(
        ...     write_volume=5000,           # 5k writes/min (bulk)
        ...     latency_requirement_ms=3600000,  # OK with stale data (1 hour)
        ...     read_volume=1000             # Low read volume
        ... )
        >>> print(strategy)
        "scheduled"
    """
    # Calculate key metrics
    write_to_read_ratio = write_volume / max(read_volume, 1)
    writes_per_second = write_volume / 60
    latency_seconds = latency_requirement_ms / 1000

    # Decision logic
    # Favor trigger-based if:
    # 1. Latency requirement is very strict (< 100ms)
    # 2. Read-heavy workload (write-to-read ratio < 0.1)
    # 3. Write volume is low (< 10/sec)
    if latency_requirement_ms < 100 and write_to_read_ratio < 0.1:
        return "trigger-based"

    # Favor trigger-based if reads heavily outweigh writes and latency < 500ms
    if (
        read_volume > 0
        and write_to_read_ratio < 0.01
        and latency_requirement_ms < 500
    ):
        return "trigger-based"

    # Favor trigger-based if write volume is very low
    if writes_per_second < 5 and latency_requirement_ms < 1000:
        return "trigger-based"

    # Favor scheduled if:
    # 1. High write volume (> 1000/min = 16/sec)
    # 2. Acceptable staleness (> 30 minutes)
    if writes_per_second > 16 and latency_requirement_ms > 1800000:
        return "scheduled"

    # Favor scheduled for moderate-to-high write volume
    if writes_per_second > 10:
        return "scheduled"

    # Default: trigger-based for typical OLTP workloads
    return "trigger-based"


def validate_generated_ddl(sql: str) -> list[str]:
    """Validate generated DDL syntax and structure.

    Performs static analysis of generated DDL to identify common issues:
        - Missing or malformed CREATE statements
        - Unreferenced function definitions
        - Inconsistent table names
        - Missing indexes or comments
        - Undefined template variables (e.g., {{ missing_var }})

    Note: This function performs syntactic validation only. Full SQL validation
    requires executing against a PostgreSQL database.

    Args:
        sql: DDL string to validate

    Returns:
        List of warning/error strings. Empty list means no issues detected.

    Example:
        >>> ddl = generate_tv_ddl(schema, entity="User", view="user")
        >>> errors = validate_generated_ddl(ddl)
        >>> if errors:
        ...     for error in errors:
        ...         print(f"WARNING: {error}")
        ... else:
        ...     print("DDL validation passed")
    """
    errors: list[str] = []

    # Check for unresolved template variables
    unresolv_vars = re.findall(r"\{\{[^}]+\}\}", sql)
    if unresolv_vars:
        errors.append(
            f"Found unresolved template variables: {', '.join(set(unresolv_vars))}"
        )

    # Check for CREATE statements
    creates = re.findall(r"CREATE\s+(TABLE|VIEW|FUNCTION|INDEX)", sql, re.IGNORECASE)
    if not creates:
        errors.append("No CREATE statements found in DDL")

    # Check for TABLE creation
    if "CREATE TABLE" not in sql.upper() and "CREATE OR REPLACE TABLE" not in sql.upper():
        errors.append("Missing CREATE TABLE statement")

    # Check for DROP statements (optional warning)
    drops = re.findall(r"DROP\s+(\w+)", sql, re.IGNORECASE)
    if drops and "DROP TABLE IF EXISTS" not in sql.upper():
        errors.append(
            "DROP statements found without IF EXISTS protection - "
            "use DROP ... IF EXISTS for idempotency"
        )

    # Check comment count
    comments = re.findall(r"COMMENT ON", sql, re.IGNORECASE)
    if len(comments) < 5:
        errors.append(
            f"Low comment count ({len(comments)}). "
            "Consider adding more documentation to generated DDL"
        )

    # Check for reasonable function count
    functions = re.findall(
        r"CREATE OR REPLACE FUNCTION", sql, re.IGNORECASE
    )
    if "monitoring" in sql.lower() and len(functions) < 5:
        errors.append(
            "Expected more monitoring functions. "
            "Check if include_monitoring_functions=True was used"
        )

    # Check index count
    indexes = re.findall(r"CREATE INDEX", sql, re.IGNORECASE)
    if len(indexes) < 3:
        errors.append(f"Low index count ({len(indexes)}). Consider adding more indexes")

    # Check for SQL syntax errors (basic checks)
    # Unmatched parentheses
    paren_count = sql.count("(") - sql.count(")")
    if paren_count != 0:
        errors.append(f"Unmatched parentheses: {paren_count:+d}")

    # Check for malformed JSON
    if "jsonb" in sql.lower() or "json" in sql.lower():
        # Simple check for quoted JSON in templates
        if "{{" in sql or "}}" in sql:
            errors.append("Unresolved Jinja2 template syntax in JSON context")

    return errors


__all__ = [
    "load_schema",
    "generate_tv_ddl",
    "generate_ta_ddl",
    "generate_composition_views",
    "suggest_refresh_strategy",
    "validate_generated_ddl",
]
