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
/**
 * Sale event type with analytics dimensions.
 */
declare class Sale {
    id: number;
    revenue: number;
    quantity: number;
    cost: number;
    customerId: string;
    occurredAt: string;
}
export { Sale };
//# sourceMappingURL=analytics_schema.d.ts.map