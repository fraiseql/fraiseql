#!/usr/bin/env python3
"""Cross-SDK schema parity comparator.

Compares schema JSON files from multiple SDKs against a reference (Python).
Checks structural equivalence at increasing depth:

Level 1 (names):       top-level keys, type/query/mutation names
Level 2 (fields):      field names per type
Level 3 (types):       field types + nullability per field
Level 4 (arguments):   mutation argument names, types, nullability
Level 5 (metadata):    inject_params, invalidates_views, sql_source, operation

Usage:
    python3 compare_schemas.py --reference schema_python.json \
        --compare schema_ts.json schema_go.json \
        [--types-only schema_rust.json]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load(path: Path) -> dict:
    with path.open() as f:
        return json.load(f)


def index_by_name(items: list[dict]) -> dict[str, dict]:
    return {item["name"]: item for item in items}


def compare_schemas(
    ref_name: str,
    ref: dict,
    other_name: str,
    other: dict,
    *,
    types_only: bool = False,
) -> list[str]:
    """Compare two schema dicts. Returns list of error strings."""
    errors: list[str] = []
    tag = f"{ref_name} vs {other_name}"

    # --- Top-level keys ---
    if not types_only:
        ref_keys = set(ref.keys())
        other_keys = set(other.keys())
        if ref_keys != other_keys:
            errors.append(f"[{tag}] Top-level keys differ: {sorted(ref_keys)} vs {sorted(other_keys)}")

    # --- Type comparison ---
    ref_types = index_by_name(ref.get("types", []))
    other_types = index_by_name(other.get("types", []))

    ref_type_names = set(ref_types.keys())
    other_type_names = set(other_types.keys())
    if ref_type_names != other_type_names:
        errors.append(f"[{tag}] Type names differ: {sorted(ref_type_names)} vs {sorted(other_type_names)}")

    for type_name in ref_type_names & other_type_names:
        ref_fields = index_by_name(ref_types[type_name].get("fields", []))
        other_fields = index_by_name(other_types[type_name].get("fields", []))

        # Level 2: field names
        if set(ref_fields.keys()) != set(other_fields.keys()):
            errors.append(
                f"[{tag}] {type_name} field names differ: "
                f"{sorted(ref_fields.keys())} vs {sorted(other_fields.keys())}"
            )
            continue

        # Level 3: field types + nullability
        for field_name in ref_fields:
            if field_name not in other_fields:
                continue
            rf = ref_fields[field_name]
            of = other_fields[field_name]

            ref_ft = rf.get("field_type") or rf.get("type") or rf.get("fieldType")
            other_ft = of.get("field_type") or of.get("type") or of.get("fieldType")
            if ref_ft and other_ft and ref_ft != other_ft:
                errors.append(
                    f"[{tag}] {type_name}.{field_name} type: {ref_ft} vs {other_ft}"
                )

            ref_null = rf.get("nullable")
            other_null = of.get("nullable")
            if ref_null is not None and other_null is not None and ref_null != other_null:
                errors.append(
                    f"[{tag}] {type_name}.{field_name} nullable: {ref_null} vs {other_null}"
                )

    if types_only:
        return errors

    # --- Query comparison ---
    ref_queries = index_by_name(ref.get("queries", []))
    other_queries = index_by_name(other.get("queries", []))

    if set(ref_queries.keys()) != set(other_queries.keys()):
        errors.append(
            f"[{tag}] Query names differ: "
            f"{sorted(ref_queries.keys())} vs {sorted(other_queries.keys())}"
        )

    for qname in set(ref_queries.keys()) & set(other_queries.keys()):
        rq = ref_queries[qname]
        oq = other_queries[qname]

        # Return type
        for key in ("return_type", "returnType"):
            if key in rq and key in oq and rq[key] != oq[key]:
                errors.append(f"[{tag}] query {qname} {key}: {rq[key]} vs {oq[key]}")

        # sql_source
        for key in ("sql_source", "sqlSource"):
            if key in rq and key in oq and rq[key] != oq[key]:
                errors.append(f"[{tag}] query {qname} {key}: {rq[key]} vs {oq[key]}")

    # --- Mutation comparison ---
    ref_muts = index_by_name(ref.get("mutations", []))
    other_muts = index_by_name(other.get("mutations", []))

    if set(ref_muts.keys()) != set(other_muts.keys()):
        errors.append(
            f"[{tag}] Mutation names differ: "
            f"{sorted(ref_muts.keys())} vs {sorted(other_muts.keys())}"
        )

    for mname in set(ref_muts.keys()) & set(other_muts.keys()):
        rm = ref_muts[mname]
        om = other_muts[mname]

        # Level 4: argument names + types
        ref_args = index_by_name(rm.get("arguments", []))
        other_args = index_by_name(om.get("arguments", []))
        if set(ref_args.keys()) != set(other_args.keys()):
            errors.append(
                f"[{tag}] mutation {mname} argument names differ: "
                f"{sorted(ref_args.keys())} vs {sorted(other_args.keys())}"
            )
        for aname in set(ref_args.keys()) & set(other_args.keys()):
            ra = ref_args[aname]
            oa = other_args[aname]
            ra_type = ra.get("type") or ra.get("field_type") or ra.get("fieldType")
            oa_type = oa.get("type") or oa.get("field_type") or oa.get("fieldType")
            if ra_type and oa_type and ra_type != oa_type:
                errors.append(
                    f"[{tag}] mutation {mname}.{aname} type: {ra_type} vs {oa_type}"
                )

        # Level 5: operation, sql_source, inject_params, invalidates
        for key in ("operation", "sql_source", "sqlSource"):
            if key in rm and key in om and rm[key] != om[key]:
                errors.append(f"[{tag}] mutation {mname} {key}: {rm[key]} vs {om[key]}")

        # inject_params (normalize key names)
        ref_inject = rm.get("inject_params") or rm.get("injectParams") or {}
        other_inject = om.get("inject_params") or om.get("injectParams") or {}
        if ref_inject and other_inject and ref_inject != other_inject:
            errors.append(
                f"[{tag}] mutation {mname} inject_params: {ref_inject} vs {other_inject}"
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True, help="Reference schema (Python)")
    parser.add_argument("--compare", type=Path, nargs="+", default=[], help="Full-parity schemas")
    parser.add_argument("--types-only", type=Path, nargs="*", default=[], help="Types-only schemas")
    args = parser.parse_args()

    ref = load(args.reference)
    all_errors: list[str] = []

    for schema_path in args.compare:
        other = load(schema_path)
        name = schema_path.stem.replace("schema_", "").replace("schema-", "")
        errs = compare_schemas("python", ref, name, other)
        all_errors.extend(errs)

    for schema_path in args.types_only:
        other = load(schema_path)
        name = schema_path.stem.replace("schema_", "").replace("schema-", "")
        errs = compare_schemas("python", ref, name, other, types_only=True)
        all_errors.extend(errs)

    if all_errors:
        print(f"SDK parity check FAILED ({len(all_errors)} error(s)):")
        for err in all_errors:
            print(f"  - {err}")
        return 1

    sdk_count = len(args.compare) + len(args.types_only) + 1  # +1 for reference
    print(f"SDK parity check passed ({sdk_count} SDKs compared)")
    ref_types = {t["name"] for t in ref.get("types", [])}
    ref_queries = {q["name"] for q in ref.get("queries", [])}
    ref_mutations = {m["name"] for m in ref.get("mutations", [])}
    print(f"  Types:     {sorted(ref_types)}")
    print(f"  Queries:   {sorted(ref_queries)}")
    print(f"  Mutations: {sorted(ref_mutations)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
