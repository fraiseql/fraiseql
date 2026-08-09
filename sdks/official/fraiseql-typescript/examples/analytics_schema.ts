/**
 * Analytics: declaring a fact table.
 *
 * A fact table is a denormalized, append-only table FraiseQL maintains for you: numeric
 * **measures** to aggregate, a JSONB **dimension** column with extracted paths to group
 * by, and **denormalized filters** materialised as real columns so `WHERE` stays fast.
 * Mutations declare `invalidates_fact_tables` to keep it current.
 *
 * **Aggregate queries are not declared here.** `aggregate_queries` in an authored schema
 * reached no compiled artifact and is now refused outright (#956); declare each one as an
 * `[[analytics.queries]]` entry in `fraiseql.toml` instead (#624), where it becomes an
 * ordinary list-returning, view-backed query. The example below stops at the fact table,
 * which is the half that compiles.
 *
 * `registerFactTable` is a `SchemaRegistry` static rather than a top-level export.
 *
 * Usage:
 *   npx tsx examples/analytics_schema.ts
 *   fraiseql-cli compile schema.json
 */

import {
  SchemaRegistry,
  exportSchema,
  registerMutation,
  registerTypeFields,
} from "../src/index";

// ============================================================================
// The transactional type the facts are derived from
// ============================================================================

registerTypeFields(
  "Sale",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "revenue", type: "Float", nullable: false },
    { name: "quantity", type: "Int", nullable: false },
    { name: "cost", type: "Float", nullable: false },
    { name: "customerId", type: "ID", nullable: false },
    { name: "occurredAt", type: "DateTime", nullable: false },
  ],
  "A single sale event",
  { sqlSource: "v_sale" }
);

// ============================================================================
// The fact table
// ============================================================================

SchemaRegistry.registerFactTable(
  "tf_sale",
  // Measures: the numeric columns aggregations run over.
  [
    { name: "revenue", sql_type: "Float", nullable: false },
    { name: "quantity", sql_type: "Int", nullable: false },
    { name: "cost", sql_type: "Float", nullable: false },
  ],
  // Dimensions: paths extracted from the JSONB column to GROUP BY.
  {
    name: "data",
    paths: [
      { name: "category", json_path: "data->>'category'", data_type: "text" },
      { name: "product_name", json_path: "data->>'product_name'", data_type: "text" },
    ],
  },
  // Denormalized filters: real columns, so WHERE does not have to open the JSONB.
  [
    { name: "customer_id", sql_type: "Text", indexed: true },
    { name: "occurred_at", sql_type: "Timestamp", indexed: true },
  ]
);

// ============================================================================
// Keeping it current
// ============================================================================

// `invalidates_fact_tables` is how a write tells the engine which facts went stale.
// Without it the fact table drifts from the transactional data with nothing to notice.
registerMutation(
  "recordSale",
  "Sale",
  false,
  false,
  [
    { name: "customerId", type: "ID", nullable: false },
    { name: "revenue", type: "Float", nullable: false },
    { name: "quantity", type: "Int", nullable: false },
  ],
  "Record a sale and refresh the sales facts",
  {
    sql_source: "fn_record_sale",
    operation: "insert",
    invalidates_views: ["v_sale"],
    invalidates_fact_tables: ["tf_sale"],
  }
);

// ============================================================================
// Export
// ============================================================================

exportSchema("schema.json");
console.log("   Fact table: tf_sale (3 measures, 2 dimensions, 2 filters)");
console.log("   Aggregate queries: declare as [[analytics.queries]] in fraiseql.toml");
