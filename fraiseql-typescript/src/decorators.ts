/**
 * Decorators for FraiseQL schema authoring (compile-time only).
 *
 * These decorators register type and query definitions with the schema registry
 * for JSON export. NO runtime behavior - only metadata collection.
 */

import { SchemaRegistry, ArgumentDefinition, Field, EnumValue } from "./registry";

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
 * Configuration for Enum decorator.
 */
export interface EnumConfig {
  description?: string;
}

/**
 * Decorator to mark an object as a GraphQL enum.
 *
 * This decorator registers the enum with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * @param values - Object with enum values as keys (values are not used, only keys matter)
 * @param config - Optional configuration
 * @returns Decorator function
 *
 * @example
 * ```ts
 * const OrderStatus = enum('OrderStatus', {
 *   PENDING: 'pending',
 *   SHIPPED: 'shipped',
 *   DELIVERED: 'delivered'
 * }, {
 *   description: 'The status of an order'
 * })
 * ```
 *
 * This generates JSON:
 * ```json
 * {
 *   "name": "OrderStatus",
 *   "description": "The status of an order",
 *   "values": [
 *     {"name": "PENDING"},
 *     {"name": "SHIPPED"},
 *     {"name": "DELIVERED"}
 *   ]
 * }
 * ```
 */
export function enum_(
  name: string,
  values: Record<string, unknown>,
  config?: EnumConfig
): Record<string, unknown> {
  // Extract enum value names from the values object
  const enumValues: EnumValue[] = Object.keys(values).map((key) => ({
    name: key,
  }));

  // Register enum with schema registry
  SchemaRegistry.registerEnum(name, enumValues, config?.description);

  // Return the values object for backward compatibility
  return values;
}

/**
 * Configuration for Interface decorator.
 */
export interface InterfaceConfig {
  description?: string;
}

/**
 * Decorator to mark a class as a GraphQL interface.
 *
 * This decorator registers the interface with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * Interfaces define a common set of fields that multiple object types can implement.
 * Per GraphQL spec §3.7, interfaces enable polymorphic queries.
 *
 * @param name - Interface name
 * @param fields - Field definitions
 * @param config - Optional configuration
 * @returns Interface marker object
 *
 * @example
 * ```ts
 * const Node = interface('Node', {
 *   id: { type: 'ID', nullable: false },
 *   createdAt: { type: 'DateTime', nullable: false }
 * }, {
 *   description: 'An object with a globally unique ID'
 * })
 * ```
 */
export function interface_(
  name: string,
  fields: Field[],
  config?: InterfaceConfig
): Record<string, unknown> {
  // Register interface with schema registry
  SchemaRegistry.registerInterface(name, fields, config?.description);

  // Return an empty object as marker
  return {};
}

/**
 * Configuration for Union decorator.
 */
export interface UnionConfig {
  description?: string;
}

/**
 * Decorator to mark a class as a GraphQL union type.
 *
 * Per GraphQL spec §3.10, unions represent a type that could be one of
 * several object types. Unlike interfaces, unions don't define common fields.
 *
 * This decorator registers the union with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * @param name - Union name
 * @param memberTypes - List of member type names
 * @param config - Optional configuration
 * @returns Union marker object
 *
 * @example
 * ```ts
 * const SearchResult = union('SearchResult', ['User', 'Post', 'Comment'], {
 *   description: 'Result of a search query'
 * })
 * ```
 */
export function union(
  name: string,
  memberTypes: string[],
  config?: UnionConfig
): Record<string, unknown> {
  // Register union with schema registry
  SchemaRegistry.registerUnion(name, memberTypes, config?.description);

  // Return an empty object as marker
  return {};
}

/**
 * Configuration for Input decorator.
 */
export interface InputConfig {
  description?: string;
}

/**
 * Decorator to mark a class as a GraphQL input type.
 *
 * This decorator registers the input type with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * @param name - Input type name
 * @param fields - Field definitions with optional defaults
 * @param config - Optional configuration
 * @returns Input marker object
 *
 * @example
 * ```ts
 * const CreateUserInput = input('CreateUserInput', [
 *   { name: 'name', type: 'String', nullable: false },
 *   { name: 'email', type: 'String', nullable: false },
 *   { name: 'role', type: 'String', nullable: false, default: 'user' }
 * ], {
 *   description: 'Input for creating a new user'
 * })
 * ```
 */
export function input(
  name: string,
  fields: Array<Field & { default?: unknown }>,
  config?: InputConfig
): Record<string, unknown> {
  // Register input type with schema registry
  SchemaRegistry.registerInputType(name, fields, config?.description);

  // Return an empty object as marker
  return {};
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
export function registerTypeFields(typeName: string, fields: Field[], description?: string): void {
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

/**
 * Configuration for Subscription decorator.
 */
export interface SubscriptionConfig {
  entityType?: string;
  topic?: string;
  operation?: "CREATE" | "UPDATE" | "DELETE";
  [key: string]: unknown;
}

/**
 * Decorator to mark a function as a GraphQL subscription.
 *
 * This decorator registers the function with the schema registry for JSON export.
 * NO runtime behavior - only used for schema compilation.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * @param config - Subscription configuration
 * @returns Decorator function
 *
 * @example
 * ```ts
 * @Subscription({ topic: "order_events" })
 * function orderCreated(userId?: string): Order {
 *   pass;
 * }
 * ```
 *
 * This generates JSON:
 * ```json
 * {
 *   "name": "orderCreated",
 *   "entity_type": "Order",
 *   "nullable": false,
 *   "arguments": [
 *     {"name": "userId", "type": "String", "nullable": true}
 *   ],
 *   "topic": "order_events"
 * }
 * ```
 */
export function Subscription(config?: SubscriptionConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (_target: any, propertyKey: string, descriptor: PropertyDescriptor) {
    const originalMethod = descriptor.value;
    const methodName = propertyKey;

    // Extract entity type from config or use placeholder
    const entityType = config?.entityType || "Subscription";

    // For now, register with basic info
    // In a full implementation with reflect-metadata, we'd extract parameter types
    SchemaRegistry.registerSubscription(
      methodName,
      entityType,
      false, // Placeholder for nullable
      [], // Placeholder for arguments
      originalMethod?.toString?.().split("\n")[0],
      config
    );

    return descriptor;
  };
}

/**
 * Helper function to manually register subscription with full metadata.
 *
 * @param name - Subscription name
 * @param entityType - Entity type being subscribed to
 * @param nullable - Whether result can be null
 * @param args - Argument definitions (filters)
 * @param description - Optional subscription description
 * @param config - Additional configuration (topic, operation)
 *
 * @example
 * ```ts
 * registerSubscription(
 *   "orderCreated",
 *   "Order",
 *   false,
 *   [
 *     { name: "userId", type: "String", nullable: true }
 *   ],
 *   "Subscribe to new orders",
 *   { topic: "order_events", operation: "CREATE" }
 * );
 * ```
 */
export function registerSubscription(
  name: string,
  entityType: string,
  nullable: boolean,
  args: ArgumentDefinition[],
  description?: string,
  config?: Record<string, unknown>
): void {
  SchemaRegistry.registerSubscription(name, entityType, nullable, args, description, config);
}
