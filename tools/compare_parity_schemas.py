#!/usr/bin/env python3
"""Compare schema.json outputs from all SDKs and report any divergence.

Each SDK should produce identical values for the required fields below.
The script normalises both list and dict formats so SDKs that emit
``{"queries": {"users": {...}}}`` are compared correctly against SDKs
that emit ``{"queries": [{"name": "users", ...}]}``.

Usage:
    tools/compare_parity_schemas.py SDK1.json SDK2.json ... golden.json
"""

from __future__ import annotations

import json
import sys

# Fields compared per schema category.
# Intentionally excludes fields that only some SDKs support
# (e.g. ``auto_params``, which varies in shape between Python and others).
REQUIRED_FIELDS: dict[str, list[str]] = {
    "queries": [
        "name",
        "sql_source",
        "returns_list",
        "inject_params",
        "cache_ttl_seconds",
        "requires_role",
    ],
    "mutations": [
        "name",
        "sql_source",
        "operation",
        "inject_params",
        "invalidates_views",
        "invalidates_fact_tables",
    ],
    "types": [
        "name",
        "sql_source",
        "is_error",
    ],
}


def normalise(obj: object) -> object:
    """Recursively sort dicts and order-independent lists for stable comparison."""
    if isinstance(obj, dict):
        return {k: normalise(v) for k, v in sorted(obj.items())}
    if isinstance(obj, list):
        try:
            return sorted([normalise(i) for i in obj], key=str)
        except TypeError:
            return [normalise(i) for i in obj]
    return obj


def section_to_list(section: object) -> list[dict]:
    """Convert a schema section to a list of item dicts.

    Handles both formats:
    - Array: ``[{"name": "users", ...}, ...]``
    - Object: ``{"users": {"name": "users", ...}, ...}``
    """
    if isinstance(section, list):
        return [item for item in section if isinstance(item, dict)]
    if isinstance(section, dict):
        result: list[dict] = []
        for name, item in section.items():
            if isinstance(item, dict):
                if "name" not in item:
                    item = {"name": name, **item}
                result.append(item)
        return result
    return []


def extract_fields(
    schema: dict,
    category: str,
    fields: list[str],
) -> dict[str, dict]:
    """Return {item_name: {field: value, ...}} for every item in the category."""
    results: dict[str, dict] = {}
    for item in section_to_list(schema.get(category, [])):
        key = item.get("name")
        if key:
            results[key] = {f: item.get(f) for f in fields}
    return results


def main() -> None:
    paths = sys.argv[1:]
    if len(paths) < 2:
        print("Usage: compare_parity_schemas.py SDK1.json SDK2.json ...", file=sys.stderr)
        sys.exit(1)

    schemas: dict[str, dict] = {}
    for path in paths:
        with open(path, encoding="utf-8") as fh:
            schemas[path] = json.load(fh)

    errors: list[str] = []

    for category, fields in REQUIRED_FIELDS.items():
        reference_path = paths[0]
        reference = extract_fields(schemas[reference_path], category, fields)

        for path in paths[1:]:
            candidate = extract_fields(schemas[path], category, fields)

            # Only check items that the reference has; skip items absent in
            # either side so partial SDKs (e.g. empty queries) don't fail.
            for item_name, ref_values in reference.items():
                if item_name not in candidate:
                    # Item missing entirely from this SDK's output — skip.
                    continue
                cand_values = candidate[item_name]

                # Compare only fields that are non-null in the reference to
                # avoid failing when an SDK legitimately omits an optional field.
                filtered_ref  = {k: v for k, v in ref_values.items()  if v is not None}
                filtered_cand = {k: cand_values.get(k) for k in filtered_ref}

                if normalise(filtered_ref) != normalise(filtered_cand):
                    errors.append(
                        f"DIVERGENCE in {category}/{item_name}:\n"
                        f"  {reference_path}: {filtered_ref}\n"
                        f"  {path}: {filtered_cand}"
                    )

    if errors:
        for err in errors:
            print(f"ERROR: {err}", file=sys.stderr)
        sys.exit(1)

    print(
        f"OK: All {len(paths)} SDK schemas are identical for required fields "
        f"across {', '.join(REQUIRED_FIELDS.keys())}."
    )


if __name__ == "__main__":
    main()
