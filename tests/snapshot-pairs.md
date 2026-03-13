# SQL Snapshot Pairing Registry

This file maps every `.snap` file in
`crates/fraiseql-core/tests/snapshots/` to its behavioral counterpart.
See [docs/testing.md](../docs/testing.md) for the full pairing policy and
the meaning of each status value.

**Enforcement**: `tools/check-snapshot-pairing.sh` (pre-commit hook) verifies
that every snapshot found on disk is listed here and has a non-empty status.

---

## Registry

| Snapshot (short name) | Status | Paired test |
|-----------------------|--------|-------------|
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
| `generated_sql__generated_sqlite_eq` | `generator` | `sql_snapshots::generated_sql::generated_sqlite_eq` |
| `generated_sql__generated_sqlite_gt` | `generator` | `sql_snapshots::generated_sql::generated_sqlite_gt` |
| `generated_sql__generated_sqlite_like` | `generator` | `sql_snapshots::generated_sql::generated_sqlite_like` |
| `snapshot_aggregate_query_sum` | `behavioral` | `sql_behavioral::aggregate_sum_produces_correct_sql` |
| `snapshot_aggregate_query_with_group_by` | `behavioral` | `sql_behavioral::aggregate_group_by_produces_correct_sql` |
| `snapshot_boolean_literal` | `behavioral` | `sql_behavioral::boolean_literal_eq_clause` |
| `snapshot_mysql_basic_select` | `doc-only` | Legacy alias for `snapshot_parity_mysql_basic_select`; no generator to call directly. |
| `snapshot_mysql_function_call` | `doc-only` | Legacy alias for `snapshot_parity_mysql_function_call`; static documentation. |
| `snapshot_mysql_where_like` | `doc-only` | Legacy alias for `snapshot_parity_mysql_like`; static documentation. |
| `snapshot_null_handling_is_null` | `behavioral` | `sql_behavioral::null_handling_is_null_clause` |
| `snapshot_parity_mysql_basic_select` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_mysql_function_call` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_mysql_like` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_mysql_offset_pagination` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_postgres_basic_select` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_postgres_function_call` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_postgres_keyset_pagination` | `behavioral` | `sql_behavioral::keyset_pagination_where_clause` |
| `snapshot_parity_postgres_like` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlite_basic_select` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlite_like` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlserver_basic_select` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlserver_function_call` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlserver_like` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_parity_sqlserver_offset_pagination` | `cross-db-parity` | `cross_database_test` suite |
| `snapshot_postgres_basic_select` | `doc-only` | No WHERE clause or generator to call; documents the base SELECT template. |
| `snapshot_postgres_function_call_create` | `behavioral` | `sql_behavioral::mutation_create_sql_shape` |
| `snapshot_postgres_function_call_delete` | `behavioral` | `sql_behavioral::mutation_delete_sql_shape` |
| `snapshot_postgres_function_call_update` | `behavioral` | `sql_behavioral::mutation_update_sql_shape` |
| `snapshot_postgres_multiple_where_clauses` | `behavioral` | `sql_behavioral::multiple_where_clauses_and` |
| `snapshot_postgres_rls_only` | `behavioral` | `sql_behavioral::rls_only_clause` |
| `snapshot_postgres_select_with_limit` | `doc-only` | Documents LIMIT template; no WHERE generator. |
| `snapshot_postgres_select_with_offset` | `doc-only` | Documents LIMIT/OFFSET template; no WHERE generator. |
| `snapshot_postgres_select_with_order_by_asc` | `doc-only` | Documents ORDER BY ASC template. |
| `snapshot_postgres_select_with_order_by_desc` | `doc-only` | Documents ORDER BY DESC + LIMIT template. |
| `snapshot_postgres_where_eq_operator` | `behavioral` | `sql_behavioral::where_eq_operator` |
| `snapshot_postgres_where_gt_operator` | `behavioral` | `sql_behavioral::where_gt_with_cast` |
| `snapshot_postgres_where_in_operator` | `behavioral` | `sql_behavioral::where_in_operator` |
| `snapshot_postgres_where_is_not_null` | `behavioral` | `sql_behavioral::where_is_not_null` |
| `snapshot_postgres_where_is_null` | `behavioral` | `sql_behavioral::where_is_null` |
| `snapshot_postgres_where_like_operator` | `behavioral` | `sql_behavioral::where_ilike_operator` |
| `snapshot_postgres_with_field_projection` | `behavioral` | `sql_behavioral::field_projection_sql` |
| `snapshot_postgres_with_rls_where_clause` | `behavioral` | `sql_behavioral::rls_combined_where_clause` |
| `snapshot_relay_pagination_keyset` | `behavioral` | `sql_behavioral::keyset_pagination_where_clause` |
| `snapshot_relay_pagination_offset_fallback` | `doc-only` | Documents MySQL/SQLite offset fallback; no PostgreSQL generator. |
| `snapshot_reserved_keywords_quoted` | `doc-only` | Documents identifier quoting; no WHERE generator involved. |
| `snapshot_special_characters_in_like` | `behavioral` | `sql_behavioral::special_chars_ilike_clause` |
| `snapshot_sqlite_basic_select` | `doc-only` | Legacy alias; documents SQLite SELECT template. |
| `snapshot_sqlserver_basic_select` | `doc-only` | Documents SQL Server SELECT template. |
| `snapshot_sqlserver_where_like` | `doc-only` | Documents SQL Server LIKE syntax. |
| `snapshot_type_casting_timestamp` | `behavioral` | `sql_behavioral::type_cast_gt_timestamp` |
| `snapshot_type_casting_uuid` | `behavioral` | `sql_behavioral::where_in_operator` |
| `mysql_relay__snapshot_mysql_relay_backward_bigint_cursor` | `doc-only` | MySQL relay — no PostgreSQL generator path; static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_backward_cursor_with_where` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_backward_custom_order_by` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_backward_no_cursor` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_count_no_where` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_count_with_where` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_forward_bigint_cursor` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_forward_cursor_with_where` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_forward_custom_order_by` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_forward_no_cursor` | `doc-only` | MySQL relay — static dialect doc. |
| `mysql_relay__snapshot_mysql_relay_forward_uuid_cursor` | `doc-only` | MySQL relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_backward_custom_order_by_asc` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_backward_custom_order_by_multi_column` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_backward_int64_cursor` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_backward_no_cursor_no_order` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_backward_uuid_cursor` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_count_query_no_where` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_count_query_with_where` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_forward_custom_order_by` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_forward_int64_cursor` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_forward_no_cursor_no_order` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_forward_uuid_cursor` | `doc-only` | SQL Server relay — static dialect doc. |
| `sqlserver_relay__snapshot_sqlserver_relay_forward_where_clause` | `doc-only` | SQL Server relay — static dialect doc. |

---

## Coverage Summary

| Status | Count |
|--------|-------|
| `generator` | 24 |
| `behavioral` | 22 |
| `cross-db-parity` | 13 |
| `doc-only` | 36 |
| **Total** | **95** |
