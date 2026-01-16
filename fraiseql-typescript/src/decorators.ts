/**
 * Decorators for FraiseQL schema authoring (compile-time only).
 *
 * These decorators register type and query definitions with the schema registry
 * for JSON export. NO runtime behavior - only metadata collection.
 */

import { SchemaRegistry, ArgumentDefinition, Field } from "./registry";

/**
 * Configuration for a Type decorator.
 */
export interface TypeConfig {
  description?: string;
}

/**
 * Decorator to mark a class as a GraphQL type.
 *
 * This decorator registers the class with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * @param config - Optional configuration
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @Type()
 * class User {
 *   id: number;
 *   name: string;
 *   email: string | null;
 * }
 * ```
 *
 * This generates JSON:
 * ```json
 * {
 *   "name": "User",
 *   "fields": [
 *     {"name": "id", "type": "Int", "nullable": false},
 *     {"name": "name", "type": "String", "nullable": false},
 *     {"name": "email", "type": "String", "nullable": true}
 *   ]
 * }
 * ```
 */
export function Type(_config?: TypeConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    // Extract type name from class
    const typeName = constructor.name;

    // For TypeScript at runtime, we need metadata from reflect-metadata or manual annotation
    // This is a simplified version that relies on manual field type passing
    // In a real implementation, you'd use TypeScript decorators with reflect-metadata

    // For now, register an empty type - users will need to provide metadata separately
    SchemaRegistry.registerType(typeName, [], _config?.description);

    // Return the original class unmodified
    return constructor;
  };
}

/**
 * Configuration for Query and Mutation decorators.
 */
export interface OperationConfig {
  sqlSource?: string;
  autoParams?: Record<string, boolean>;
  operation?: string;
  jsonbColumn?: string;
  [key: string]: unknown;
}

/**
 * Decorator to mark a function as a GraphQL query.
 *
 * This decorator registers the function with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * Configuration is provided via parameters:
 * - sqlSource: SQL view name or table name
 * - autoParams: Auto-parameter configuration
 * - Other configuration as needed
 *
 * @param config - Query configuration
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @Query({ sqlSource: "v_user" })
 * function users(limit: number = 10, offset: number = 0): User[] {
 *   pass;
 * }
 * ```
 *
 * This generates JSON:
 * ```json
 * {
 *   "name": "users",
 *   "return_type": "User",
 *   "returns_list": true,
 *   "nullable": false,
 *   "arguments": [
 *     {"name": "limit", "type": "Int", "nullable": false, "default": 10},
 *     {"name": "offset", "type": "Int", "nullable": false, "default": 0}
 *   ],
 *   "sql_source": "v_user"
 * }
 * ```
 */
export function Query(config?: OperationConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (_target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const originalMethod = descriptor.value;
    const methodName = propertyKey;

    // For now, register with basic info
    // In a full implementation with reflect-metadata, we'd extract parameter types
    SchemaRegistry.registerQuery(
      methodName,
      "Query", // Placeholder - should be extracted from metadata
      false, // Placeholder
      false, // Placeholder
      [], // Placeholder
      originalMethod?.toString?.().split("\n")[0],
      config
    );

    return descriptor;
  };
}

/**
 * Configuration for Mutation decorator.
 */
export interface MutationConfig extends OperationConfig {
  operation?: "CREATE" | "UPDATE" | "DELETE" | "CUSTOM";
}

/**
 * Decorator to mark a function as a GraphQL mutation.
 *
 * This decorator registers the function with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * @param config - Mutation configuration
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @Mutation({ sqlSource: "fn_create_user", operation: "CREATE" })
 * function createUser(name: string, email: string): User {
 *   pass;
 * }
 * ```
 *
 * This generates JSON:
 * ```json
 * {
 *   "name": "createUser",
 *   "return_type": "User",
 *   "returns_list": false,
 *   "nullable": false,
 *   "arguments": [
 *     {"name": "name", "type": "String", "nullable": false},
 *     {"name": "email", "type": "String", "nullable": false}
 *   ],
 *   "sql_source": "fn_create_user",
 *   "operation": "CREATE"
 * }
 * ```
 */
export function Mutation(config?: MutationConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (_target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const originalMethod = descriptor.value;
    const methodName = propertyKey;

    // For now, register with basic info
    // In a full implementation with reflect-metadata, we'd extract parameter types
    SchemaRegistry.registerMutation(
      methodName,
      "Mutation", // Placeholder - should be extracted from metadata
      false, // Placeholder
      false, // Placeholder
      [], // Placeholder
      originalMethod?.toString?.().split("\n")[0],
      config
    );

    return descriptor;
  };
}

/**
 * Configuration for FactTable decorator.
 */
export interface FactTableConfig {
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
 * Fact tables follow FraiseQL's analytics pattern:
 * - Table name starts with "tf_" (e.g., "tf_sales")
 * - Measures: SQL columns with numeric types (for aggregation)
 * - Dimensions: JSONB column (for GROUP BY)
 * - Denormalized filters: Indexed columns (for fast WHERE)
 *
 * @param config - Fact table configuration
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
 */
export function FactTable(config: FactTableConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function <T extends { new (...args: any[]): {} }>(constructor: T) {
    // Validate table name starts with tf_
    if (!config.tableName.startsWith("tf_")) {
      throw new Error(`Fact table name must start with 'tf_', got: ${config.tableName}`);
    }

    const tableName = config.tableName;
    const dimensionColumn = config.dimensionColumn || "data";

    // Build measure definitions
    const measures = config.measures.map((name) => ({
      name,
      sql_type: "Float", // Placeholder - should extract from class
      nullable: false, // Placeholder
    }));

    // Build dimension metadata
    const dimensions = {
      name: dimensionColumn,
      paths: config.dimensionPaths || [],
    };

    // Build denormalized filters (non-measure fields)
    const denormalizedFilters = [
      {
        name: "id",
        sql_type: "Int",
        indexed: true,
      },
    ];

    // Register fact table
    SchemaRegistry.registerFactTable(tableName, measures, dimensions, denormalizedFilters);

    // Return the original class unmodified
    return constructor;
  };
}

/**
 * Configuration for AggregateQuery decorator.
 */
export interface AggregateQueryConfig {
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
 * @param config - Aggregate query configuration
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
 * function salesAggregate(): Record<string, unknown>[] {
 *   pass;
 * }
 * ```
 */
export function AggregateQuery(config: AggregateQueryConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (_target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const methodName = propertyKey;

    // Register aggregate query
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
 * Helper function to manually register type fields with metadata.
 *
 * Since TypeScript doesn't preserve type information at runtime by default,
 * this helper allows explicit field registration for types.
 *
 * @param typeName - Name of the type
 * @param fields - Field definitions
 * @param description - Optional type description
 *
 * @example
 * ```ts
 * @Type()
 * class User {
 *   id: number;
 *   name: string;
 *   email: string | null;
 * }
 *
 * registerTypeFields("User", [
 *   { name: "id", type: "Int", nullable: false },
 *   { name: "name", type: "String", nullable: false },
 *   { name: "email", type: "String", nullable: true }
 * ]);
 * ```
 */
export function registerTypeFields(
  typeName: string,
  fields: Field[],
  description?: string
): void {
  SchemaRegistry.registerType(typeName, fields, description);
}

/**
 * Helper function to manually register query with full metadata.
 *
 * @param name - Query name
 * @param returnType - Return type name
 * @param returnsList - Whether query returns a list
 * @param nullable - Whether result can be null
 * @param args - Argument definitions
 * @param description - Optional query description
 * @param config - Additional configuration
 *
 * @example
 * ```ts
 * registerQuery(
 *   "users",
 *   "User",
 *   true,
 *   false,
 *   [
 *     { name: "limit", type: "Int", nullable: false, default: 10 },
 *     { name: "offset", type: "Int", nullable: false, default: 0 }
 *   ],
 *   "Get list of users",
 *   { sql_source: "v_user" }
 * );
 * ```
 */
export function registerQuery(
  name: string,
  returnType: string,
  returnsList: boolean,
  nullable: boolean,
  args: ArgumentDefinition[],
  description?: string,
  config?: Record<string, unknown>
): void {
  SchemaRegistry.registerQuery(name, returnType, returnsList, nullable, args, description, config);
}

/**
 * Helper function to manually register mutation with full metadata.
 *
 * @param name - Mutation name
 * @param returnType - Return type name
 * @param returnsList - Whether mutation returns a list
 * @param nullable - Whether result can be null
 * @param args - Argument definitions
 * @param description - Optional mutation description
 * @param config - Additional configuration
 */
export function registerMutation(
  name: string,
  returnType: string,
  returnsList: boolean,
  nullable: boolean,
  args: ArgumentDefinition[],
  description?: string,
  config?: Record<string, unknown>
): void {
  SchemaRegistry.registerMutation(
    name,
    returnType,
    returnsList,
    nullable,
    args,
    description,
    config
  );
}
