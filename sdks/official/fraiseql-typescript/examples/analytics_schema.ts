/**
 * Example FraiseQL analytics schema with fact tables.
 *
 * This example demonstrates:
 * - Fact table definitions with @FactTable()
 * - Aggregate query definitions with @AggregateQuery()
 * - Dimension paths and measure definitions
 * - Schema export with analytics support
 *
 * Usage:
 *   npx tsx examples/analytics_schema.ts
 *   # Creates schema.json with analytics fact tables
 */

import * as fraiseql from "../src/index";

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * Sale event type with analytics dimensions.
 */
@fraiseql.type()
class Sale {
  id!: number;
  revenue!: number;
  quantity!: number;
  cost!: number;
  customerId!: string;
  occurredAt!: string;
}

// ============================================================================
// Field Registration
// ============================================================================

fraiseql.registerTypeFields("Sale", [
  { name: "id", type: "Int", nullable: false },
  { name: "revenue", type: "Float", nullable: false },
  { name: "quantity", type: "Int", nullable: false },
  { name: "cost", type: "Float", nullable: false },
  { name: "customerId", type: "String", nullable: false },
  { name: "occurredAt", type: "String", nullable: false },
]);

// ============================================================================
// Fact Table Definition
// ============================================================================

/**
 * Sales fact table for analytics.
 *
 * This fact table stores:
 * - Measures: revenue, quantity, cost (numeric aggregations)
 * - Dimensions: category, product_name (GROUP BY dimensions)
 * - Filters: customer_id, occurred_at (denormalized for fast WHERE)
 */
fraiseql.registerFactTableManual(
  "tf_sales",
  [
    { name: "revenue", sql_type: "Float", nullable: false },
    { name: "quantity", sql_type: "Int", nullable: false },
    { name: "cost", sql_type: "Float", nullable: false },
  ],
  {
    name: "data",
    paths: [
      {
        name: "category",
        json_path: "data->>'category'",
        data_type: "text",
      },
      {
        name: "product_name",
        json_path: "data->>'product_name'",
        data_type: "text",
      },
    ],
  },
  [
    { name: "customer_id", sql_type: "Text", indexed: true },
    { name: "occurred_at", sql_type: "Timestamp", indexed: true },
    { name: "id", sql_type: "Int", indexed: false },
  ]
);

// ============================================================================
// Aggregate Queries
// ============================================================================

/**
 * Sales aggregate query with flexible grouping and aggregation.
 *
 * Supports:
 * - groupBy: { category: true, occurred_at_day: true }
 * - aggregates: { count: true, revenue_sum: true, revenue_avg: true }
 * - where: { customer_id: { _eq: "uuid-123" } }
 * - having: { revenue_sum_gt: 1000 }
 * - orderBy: [{ field: "revenue_sum", direction: "DESC" }]
 * - limit: 100, offset: 0
 */
fraiseql.registerAggregateQuery(
  "salesAggregate",
  "tf_sales",
  true, // autoGroupBy
  true // autoAggregates
);

fraiseql.registerQuery(
  "salesAggregate",
  "Record",
  true, // returns list
  false, // not nullable
  [],
  "Aggregate sales data with flexible grouping and aggregation"
);

/**
 * Monthly sales trend query.
 *
 * Aggregates sales by month and category.
 */
fraiseql.registerAggregateQuery(
  "monthlySalesTrend",
  "tf_sales",
  true, // autoGroupBy
  true // autoAggregates
);

fraiseql.registerQuery(
  "monthlySalesTrend",
  "Record",
  true, // returns list
  false, // not nullable
  [{ name: "category", type: "String", nullable: true }],
  "Monthly sales trend by category"
);

/**
 * Customer revenue summary.
 *
 * Shows total and average revenue per customer.
 */
fraiseql.registerAggregateQuery(
  "customerRevenueSummary",
  "tf_sales",
  true, // autoGroupBy
  true // autoAggregates
);

fraiseql.registerQuery(
  "customerRevenueSummary",
  "Record",
  true, // returns list
  false, // not nullable
  [
    { name: "limit", type: "Int", nullable: true, default: 100 },
    { name: "minRevenue", type: "Float", nullable: true },
  ],
  "Top customers by revenue"
);

// ============================================================================
// Export Schema
// ============================================================================

// Export schema to JSON when run as main module
if (require.main === module) {
  fraiseql.exportSchema("schema.json");

  console.log("\n✅ Analytics schema exported successfully!");
  console.log("   Fact Tables: 1 (tf_sales)");
  console.log("   Aggregate Queries: 3");
  console.log("   ");
  console.log("   Next steps:");
  console.log("   1. Compile schema: fraiseql-cli compile schema.json");
  console.log("   2. Query sales data with aggregations:");
  console.log("      query {");
  console.log("        salesAggregate(");
  console.log("          groupBy: { category: true }");
  console.log("          aggregates: { count: true, revenue_sum: true }");
  console.log("        ) { ... }");
  console.log("      }");
}

// Also export for use as a module
export { Sale };
