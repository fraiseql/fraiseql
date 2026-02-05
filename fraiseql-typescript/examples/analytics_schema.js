"use strict";
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
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.Sale = void 0;
const fraiseql = __importStar(require("../src/index"));
// ============================================================================
// Type Definitions
// ============================================================================
/**
 * Sale event type with analytics dimensions.
 */
let Sale = class Sale {
    id;
    revenue;
    quantity;
    cost;
    customerId;
    occurredAt;
};
exports.Sale = Sale;
exports.Sale = Sale = __decorate([
    fraiseql.type()
], Sale);
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
fraiseql.registerFactTableManual("tf_sales", [
    { name: "revenue", sql_type: "Float", nullable: false },
    { name: "quantity", sql_type: "Int", nullable: false },
    { name: "cost", sql_type: "Float", nullable: false },
], {
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
}, [
    { name: "customer_id", sql_type: "Text", indexed: true },
    { name: "occurred_at", sql_type: "Timestamp", indexed: true },
    { name: "id", sql_type: "Int", indexed: false },
]);
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
fraiseql.registerAggregateQuery("salesAggregate", "tf_sales", true, // autoGroupBy
true // autoAggregates
);
fraiseql.registerQuery("salesAggregate", "Record", true, // returns list
false, // not nullable
[], "Aggregate sales data with flexible grouping and aggregation");
/**
 * Monthly sales trend query.
 *
 * Aggregates sales by month and category.
 */
fraiseql.registerAggregateQuery("monthlySalesTrend", "tf_sales", true, // autoGroupBy
true // autoAggregates
);
fraiseql.registerQuery("monthlySalesTrend", "Record", true, // returns list
false, // not nullable
[{ name: "category", type: "String", nullable: true }], "Monthly sales trend by category");
/**
 * Customer revenue summary.
 *
 * Shows total and average revenue per customer.
 */
fraiseql.registerAggregateQuery("customerRevenueSummary", "tf_sales", true, // autoGroupBy
true // autoAggregates
);
fraiseql.registerQuery("customerRevenueSummary", "Record", true, // returns list
false, // not nullable
[
    { name: "limit", type: "Int", nullable: true, default: 100 },
    { name: "minRevenue", type: "Float", nullable: true },
], "Top customers by revenue");
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
