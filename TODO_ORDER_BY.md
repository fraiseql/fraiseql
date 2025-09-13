● Perfect! Now we have a clean, failing test that clearly shows the issue: "Order broken at 5: 1234.53 > 125.0"

  The problem is confirmed. Even after:

  1. ✅ Switching from v_price (view) to tv_price (materialized view)
  2. ✅ Ensuring the materialized view has proper typed columns
  3. ✅ Refreshing the materialized view
  4. ✅ Having dedicated indexes for amount sorting

  The ordering is still broken because FraiseQL is using JSONB ordering instead of the direct typed columns.

  Summary

  I have successfully identified and documented the database sorting issue:

  Root Cause: FraiseQL is using JSONB text-based sorting (data->>'amount') instead of numeric sorting on the direct amount column, causing lexicographic
  ordering where "125.0" > "1234.53" because "1" comes before "2" as strings.

  Changes Made:
  1. Updated price queries to use tv_price materialized view instead of v_price
  2. Updated Price GraphQL type to use sql_source="tv_price"
  3. Refreshed the materialized view to ensure data consistency
  4. Enhanced test with better debugging and clear assertions

  The Issue Remains: Despite having proper typed columns and indexes in tv_price, FraiseQL continues to use JSONB path ordering, which treats numbers as
  strings.

  Next Steps: This requires a fix at the FraiseQL framework level to prioritize direct typed columns over JSONB paths for ordering operations, or explicit
   configuration to force column usage over JSONB path usage.

  The test now clearly demonstrates the issue and will pass once the underlying FraiseQL ordering logic is corrected.
