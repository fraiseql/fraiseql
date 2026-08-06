# SQL Snapshot Pairing Registry

This file maps every `.snap` file in
`crates/fraiseql-core/tests/snapshots/` to its behavioral counterpart.
See [docs/testing.md](../docs/testing.md) for the full pairing policy and
the meaning of each status value.

**Enforcement**: `tools/check-snapshot-pairing.sh` (preflight ShellGates and the
pre-commit hook) verifies both directions: every snapshot found on disk is listed
here with a non-empty status, and every row here names a snapshot that exists.

---

## Registry

| Snapshot (short name) | Status | Paired test |
|-----------------------|--------|-------------|
| `aggregate_dialect_variants__snapshot_postgres_array_agg` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_array_agg` — hand-written PG spec pin, no generator to call |
| `aggregate_dialect_variants__snapshot_postgres_bool_and` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_bool_and` — hand-written PG spec pin, no generator to call |
| `aggregate_dialect_variants__snapshot_postgres_date_trunc_day` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_date_trunc_day` — hand-written PG spec pin, no generator to call |
| `aggregate_dialect_variants__snapshot_postgres_stddev` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_stddev` — hand-written PG spec pin, no generator to call |
| `aggregate_dialect_variants__snapshot_postgres_string_agg` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_string_agg` — hand-written PG spec pin, no generator to call |
| `aggregate_dialect_variants__snapshot_postgres_variance` | `doc-only` | `sql_snapshots::aggregate_dialect_variants::snapshot_postgres_variance` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_basic_select` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_basic_select` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_multiple_where_clauses` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_multiple_where_clauses` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_select_with_limit` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_select_with_limit` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_select_with_offset` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_select_with_offset` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_select_with_order_by_asc` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_select_with_order_by_asc` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_select_with_order_by_desc` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_select_with_order_by_desc` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_eq_operator` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_eq_operator` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_gt_operator` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_gt_operator` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_in_operator` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_in_operator` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_is_not_null` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_is_not_null` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_is_null` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_is_null` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_where_like_operator` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_where_like_operator` — hand-written PG spec pin, no generator to call |
| `basic__snapshot_postgres_with_field_projection` | `doc-only` | `sql_snapshots::basic::snapshot_postgres_with_field_projection` — hand-written PG spec pin, no generator to call |
| `cte_queries__snapshot_postgres_cte_basic` | `doc-only` | `sql_snapshots::cte_queries::snapshot_postgres_cte_basic` — hand-written PG spec pin, no generator to call |
| `cte_queries__snapshot_postgres_cte_multiple` | `doc-only` | `sql_snapshots::cte_queries::snapshot_postgres_cte_multiple` — hand-written PG spec pin, no generator to call |
| `cte_queries__snapshot_postgres_cte_recursive` | `doc-only` | `sql_snapshots::cte_queries::snapshot_postgres_cte_recursive` — hand-written PG spec pin, no generator to call |
| `cte_queries__snapshot_postgres_cte_with_aggregation` | `doc-only` | `sql_snapshots::cte_queries::snapshot_postgres_cte_with_aggregation` — hand-written PG spec pin, no generator to call |
| `cumulative_sum__postgres` | `generator` | `window_function_snapshots::cumulative_sum::postgres` |
| `dense_rank__postgres` | `generator` | `window_function_snapshots::dense_rank::postgres` |
| `edge_cases__snapshot_boolean_literal` | `doc-only` | `sql_snapshots::edge_cases::snapshot_boolean_literal` — hand-written PG spec pin, no generator to call |
| `edge_cases__snapshot_null_handling_is_null` | `doc-only` | `sql_snapshots::edge_cases::snapshot_null_handling_is_null` — hand-written PG spec pin, no generator to call |
| `edge_cases__snapshot_reserved_keywords_quoted` | `doc-only` | `sql_snapshots::edge_cases::snapshot_reserved_keywords_quoted` — hand-written PG spec pin, no generator to call |
| `edge_cases__snapshot_special_characters_in_like` | `doc-only` | `sql_snapshots::edge_cases::snapshot_special_characters_in_like` — hand-written PG spec pin, no generator to call |
| `edge_cases__snapshot_type_casting_timestamp` | `doc-only` | `sql_snapshots::edge_cases::snapshot_type_casting_timestamp` — hand-written PG spec pin, no generator to call |
| `edge_cases__snapshot_type_casting_uuid` | `doc-only` | `sql_snapshots::edge_cases::snapshot_type_casting_uuid` — hand-written PG spec pin, no generator to call |
| `frame_exclusion__exclude_current_row_postgres` | `generator` | `window_function_snapshots::frame_exclusion::exclude_current_row_postgres` |
| `fts_parity__snapshot_postgres_fts_matches` | `doc-only` | `sql_snapshots::fts_parity::snapshot_postgres_fts_matches` — hand-written PG spec pin, no generator to call |
| `fts_parity__snapshot_postgres_fts_phrase_query` | `doc-only` | `sql_snapshots::fts_parity::snapshot_postgres_fts_phrase_query` — hand-written PG spec pin, no generator to call |
| `fts_parity__snapshot_postgres_fts_plain_query` | `doc-only` | `sql_snapshots::fts_parity::snapshot_postgres_fts_plain_query` — hand-written PG spec pin, no generator to call |
| `fts_parity__snapshot_postgres_fts_websearch_query` | `doc-only` | `sql_snapshots::fts_parity::snapshot_postgres_fts_websearch_query` — hand-written PG spec pin, no generator to call |
| `generated_sql__generated_pg_and_two_fields` | `generator` | `sql_snapshots::generated_sql::generated_pg_and_two_fields` |
| `generated_sql__generated_pg_contains` | `generator` | `sql_snapshots::generated_sql::generated_pg_contains` |
| `generated_sql__generated_pg_deep_nested_path` | `generator` | `sql_snapshots::generated_sql::generated_pg_deep_nested_path` |
| `generated_sql__generated_pg_endswith` | `generator` | `sql_snapshots::generated_sql::generated_pg_endswith` |
| `generated_sql__generated_pg_eq` | `generator` | `sql_snapshots::generated_sql::generated_pg_eq` |
| `generated_sql__generated_pg_gt` | `generator` | `sql_snapshots::generated_sql::generated_pg_gt` |
| `generated_sql__generated_pg_gte` | `generator` | `sql_snapshots::generated_sql::generated_pg_gte` |
| `generated_sql__generated_pg_icontains` | `generator` | `sql_snapshots::generated_sql::generated_pg_icontains` |
| `generated_sql__generated_pg_ilike` | `generator` | `sql_snapshots::generated_sql::generated_pg_ilike` |
| `generated_sql__generated_pg_in_operator` | `generator` | `sql_snapshots::generated_sql::generated_pg_in_operator` |
| `generated_sql__generated_pg_is_null_false` | `generator` | `sql_snapshots::generated_sql::generated_pg_is_null_false` |
| `generated_sql__generated_pg_is_null_true` | `generator` | `sql_snapshots::generated_sql::generated_pg_is_null_true` |
| `generated_sql__generated_pg_like` | `generator` | `sql_snapshots::generated_sql::generated_pg_like` |
| `generated_sql__generated_pg_lt` | `generator` | `sql_snapshots::generated_sql::generated_pg_lt` |
| `generated_sql__generated_pg_lte` | `generator` | `sql_snapshots::generated_sql::generated_pg_lte` |
| `generated_sql__generated_pg_neq` | `generator` | `sql_snapshots::generated_sql::generated_pg_neq` |
| `generated_sql__generated_pg_nested_and_or` | `generator` | `sql_snapshots::generated_sql::generated_pg_nested_and_or` |
| `generated_sql__generated_pg_nin_operator` | `generator` | `sql_snapshots::generated_sql::generated_pg_nin_operator` |
| `generated_sql__generated_pg_or_two_fields` | `generator` | `sql_snapshots::generated_sql::generated_pg_or_two_fields` |
| `generated_sql__generated_pg_param_offset_two` | `generator` | `sql_snapshots::generated_sql::generated_pg_param_offset_two` |
| `generated_sql__generated_pg_startswith` | `generator` | `sql_snapshots::generated_sql::generated_pg_startswith` |
| `json_access_parity__snapshot_postgres_json_deep_nested` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_json_deep_nested` — hand-written PG spec pin, no generator to call |
| `json_access_parity__snapshot_postgres_json_nested` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_json_nested` — hand-written PG spec pin, no generator to call |
| `json_access_parity__snapshot_postgres_json_single_level` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_json_single_level` — hand-written PG spec pin, no generator to call |
| `json_access_parity__snapshot_postgres_jsonb_contained_by` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_jsonb_contained_by` — hand-written PG spec pin, no generator to call |
| `json_access_parity__snapshot_postgres_jsonb_contains` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_jsonb_contains` — hand-written PG spec pin, no generator to call |
| `json_access_parity__snapshot_postgres_jsonb_overlap` | `doc-only` | `sql_snapshots::json_access_parity::snapshot_postgres_jsonb_overlap` — hand-written PG spec pin, no generator to call |
| `lag__postgres` | `generator` | `window_function_snapshots::lag::postgres` |
| `last_value__postgres` | `generator` | `window_function_snapshots::last_value::postgres` |
| `lead__postgres` | `generator` | `window_function_snapshots::lead::postgres` |
| `moving_average__postgres` | `generator` | `window_function_snapshots::moving_average::postgres` |
| `multiple_windows__postgres` | `generator` | `window_function_snapshots::multiple_windows::postgres` |
| `ntile__postgres` | `generator` | `window_function_snapshots::ntile::postgres` |
| `rank__postgres` | `generator` | `window_function_snapshots::rank::postgres` |
| `relay_aggregation__snapshot_aggregate_query_sum` | `doc-only` | `sql_snapshots::relay_aggregation::snapshot_aggregate_query_sum` — hand-written PG spec pin, no generator to call |
| `relay_aggregation__snapshot_aggregate_query_with_group_by` | `doc-only` | `sql_snapshots::relay_aggregation::snapshot_aggregate_query_with_group_by` — hand-written PG spec pin, no generator to call |
| `relay_aggregation__snapshot_relay_pagination_keyset` | `doc-only` | `sql_snapshots::relay_aggregation::snapshot_relay_pagination_keyset` — hand-written PG spec pin, no generator to call |
| `row_number__postgres` | `generator` | `window_function_snapshots::row_number::postgres` |
| `stddev_variance__stddev_postgres` | `generator` | `window_function_snapshots::stddev_variance::stddev_postgres` |
| `stddev_variance__variance_postgres` | `generator` | `window_function_snapshots::stddev_variance::variance_postgres` |
| `with_limit_offset__postgres` | `generator` | `window_function_snapshots::with_limit_offset::postgres` |

---

## Status summary

| Status | Count |
|--------|-------|
| `doc-only` | 42 |
| `generator` | 35 |
| **total** | **78** |
