/**
 * Analytics decorators for FraiseQL fact tables and aggregate queries.
 *
 * This module provides decorators for defining fact tables and aggregate queries
 * that enable advanced analytics capabilities with GROUP BY and aggregation functions.
 */

import { SchemaRegistry, Measure, Dimension, DenormalizedFilter, Field } from "./registry";

/**
 * Configuration for the factTable decorator.
 */
export interface FactTableDecoratorConfig {
  tableName: string;
  measures: string[];
  dimensionColumn?: string;
  dimensionPaths?: Array<{
    name: string;
    json_path: string;
    data_type: string;
  }>;
}

/**
 * Decorator to mark a class as a fact table type.
 *
 * Fact tables are special analytics tables that follow FraiseQL's pattern:
 * - Table name starts with `tf_` (e.g., "tf_sales")
 * - Measures: SQL columns with numeric types (for aggregation)
 * - Dimensions: JSONB column (for GROUP BY)
 * - Denormalized filters: Indexed columns (for fast WHERE)
 *
 * @param config - Fact table configuration with:
 *   - tableName: SQL table name (must start with "tf_")
 *   - measures: List of field names that are measures (numeric columns)
 *   - dimensionColumn: JSONB column name (default: "data")
 *   - dimensionPaths: Optional list of dimension paths
 *
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @FactTable({
 *   tableName: "tf_sales",
 *   measures: ["revenue", "quantity", "cost"],
 *   dimensionPaths: [
 *     {
 *       name: "category",
 *       json_path: "data->>'category'",
 *       data_type: "text"
 *     },
 *     {
 *       name: "product_name",
 *       json_path: "data->>'product_name'",
 *       data_type: "text"
 *     }
 *   ]
 * })
 * @Type()
 * class Sale {
 *   id: number;
 *   revenue: number;
 *   quantity: number;
 *   cost: number;
 *   customer_id: string;
 *   occurred_at: string;
 * }
 * ```
 *
 * This generates metadata:
 * ```json
 * {
 *   "table_name": "tf_sales",
 *   "measures": [
 *     {"name": "revenue", "sql_type": "Float", "nullable": false},
 *     {"name": "quantity", "sql_type": "Int", "nullable": false},
 *     {"name": "cost", "sql_type": "Float", "nullable": false}
 *   ],
 *   "dimensions": {
 *     "name": "data",
 *     "paths": [
 *       {"name": "category", "json_path": "data->>'category'", "data_type": "text"},
 *       {"name": "product_name", "json_path": "data->>'product_name'", "data_type": "text"}
 *     ]
 *   },
 *   "denormalized_filters": [
 *     {"name": "customer_id", "sql_type": "Text", "indexed": true},
 *     {"name": "occurred_at", "sql_type": "Timestamp", "indexed": true}
 *   ]
 * }
 * ```
 *
 * Notes:
 * - Table name must start with "tf_" prefix
 * - Measures must be numeric types (Int, Float)
 * - Dimension paths are optional (can be introspected at runtime)
 * - This decorator should be combined with @Type()
 */
export function FactTable(config: FactTableDecoratorConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    // Validate table name starts with tf_
    if (!config.tableName.startsWith("tf_")) {
      throw new Error(`Fact table name must start with 'tf_', got: ${config.tableName}`);
    }

    const tableName = config.tableName;
    const dimensionColumn = config.dimensionColumn || "data";

    // Build measure definitions
    const measures: Measure[] = config.measures.map((name) => ({
      name,
      sql_type: "Float", // Default - ideally extracted from class metadata
      nullable: false,
    }));

    // Build dimension metadata
    const dimensions: Dimension = {
      name: dimensionColumn,
      paths: config.dimensionPaths || [],
    };

    // Build denormalized filters (non-measure, non-id fields)
    const denormalizedFilters: DenormalizedFilter[] = [
      {
        name: "id",
        sql_type: "Int",
        indexed: true,
      },
    ];

    // Register fact table with schema registry
    SchemaRegistry.registerFactTable(tableName, measures, dimensions, denormalizedFilters);

    // Return the original class unmodified
    return constructor;
  };
}

/**
 * Configuration for the aggregateQuery decorator.
 */
export interface AggregateQueryDecoratorConfig {
  factTable: string;
  autoGroupBy?: boolean;
  autoAggregates?: boolean;
}

/**
 * Decorator to mark a function as an aggregate query.
 *
 * Aggregate queries run GROUP BY operations on fact tables with:
 * - GROUP BY: Dimensions and temporal buckets
 * - SELECT: Aggregate functions (COUNT, SUM, AVG, etc.)
 * - WHERE: Pre-aggregation filters
 * - HAVING: Post-aggregation filters
 *
 * @param config - Configuration with:
 *   - factTable: Fact table name (e.g., "tf_sales")
 *   - autoGroupBy: Automatically generate groupBy fields (default: true)
 *   - autoAggregates: Automatically generate aggregate fields (default: true)
 *
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @AggregateQuery({
 *   factTable: "tf_sales",
 *   autoGroupBy: true,
 *   autoAggregates: true
 * })
 * @Query()
 * async function salesAggregate(): Promise<Record<string, unknown>[]> {
 *   // Function body not executed - only for type/metadata
 * }
 * ```
 *
 * This generates a query that accepts:
 * - groupBy: { category: true, occurred_at_day: true }
 * - aggregates: { count: true, revenue_sum: true, revenue_avg: true }
 * - where: { customer_id: { _eq: "uuid-123" } }
 * - having: { revenue_sum_gt: 1000 }
 * - orderBy: [{ field: "revenue_sum", direction: "DESC" }]
 * - limit: 100
 * - offset: 0
 *
 * Notes:
 * - Must be used with @Query() decorator
 * - Fact table must be registered with @FactTable()
 * - Return type should be `Record<string, unknown>[]` for flexibility
 * - NO runtime behavior - only used for schema compilation
 */
export function AggregateQuery(config: AggregateQueryDecoratorConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (_target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const methodName = propertyKey;

    // Register aggregate query with schema registry
    SchemaRegistry.registerAggregateQuery(
      methodName,
      config.factTable,
      config.autoGroupBy ?? true,
      config.autoAggregates ?? true
    );

    return descriptor;
  };
}

/**
 * Helper function to manually register a fact table with full metadata.
 *
 * This is useful when you can't use decorators or need programmatic registration.
 *
 * @param tableName - Fact table name (must start with "tf_")
 * @param measures - List of measure definitions
 * @param dimensions - Dimension configuration
 * @param denormalizedFilters - List of denormalized filter definitions
 *
 * @example
 * ```ts
 * registerFactTableManual(
 *   "tf_sales",
 *   [
 *     { name: "revenue", sql_type: "Float", nullable: false },
 *     { name: "quantity", sql_type: "Int", nullable: false }
 *   ],
 *   {
 *     name: "data",
 *     paths: [
 *       { name: "category", json_path: "data->>'category'", data_type: "text" }
 *     ]
 *   },
 *   [
 *     { name: "customer_id", sql_type: "Text", indexed: true },
 *     { name: "occurred_at", sql_type: "Timestamp", indexed: true }
 *   ]
 * );
 * ```
 */
export function registerFactTableManual(
  tableName: string,
  measures: Measure[],
  dimensions: Dimension,
  denormalizedFilters: DenormalizedFilter[]
): void {
  // Validate table name
  if (!tableName.startsWith("tf_")) {
    throw new Error(`Fact table name must start with 'tf_', got: ${tableName}`);
  }

  SchemaRegistry.registerFactTable(tableName, measures, dimensions, denormalizedFilters);
}

/**
 * Helper function to manually register type fields for use with fact tables.
 *
 * This is useful when combining @FactTable with manual field registration.
 *
 * @param typeName - Type name (should match fact table entity name)
 * @param fields - List of field definitions
 * @param description - Optional type description
 *
 * @example
 * ```ts
 * registerTypeFieldsManual(
 *   "Sale",
 *   [
 *     { name: "id", type: "Int", nullable: false },
 *     { name: "revenue", type: "Float", nullable: false },
 *     { name: "quantity", type: "Int", nullable: false },
 *     { name: "customer_id", type: "String", nullable: false }
 *   ]
 * );
 * ```
 */
export function registerTypeFieldsManual(
  typeName: string,
  fields: Field[],
  description?: string
): void {
  SchemaRegistry.registerType(typeName, fields, description);
}
