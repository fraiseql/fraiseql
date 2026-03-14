/**
 * Decorators for FraiseQL schema authoring (compile-time only).
 *
 * These decorators register type and query definitions with the schema registry
 * for JSON export. NO runtime behavior - only metadata collection.
 */

import { SchemaRegistry, ArgumentDefinition, Field, EnumValue, FieldMetadata } from "./registry";
import { CustomScalar } from "./scalars";

/**
 * Create field-level metadata for access control and deprecation.
 *
 * This function creates metadata for use with field definitions to add:
 * - `requiresScope`: JWT scope required to access this field
 * - `deprecated`: Deprecation marker with optional reason
 * - `description`: Field description for GraphQL schema
 *
 * @param options - Field metadata options
 * @returns Field metadata object
 *
 * @example
 * ```typescript
 * fraiseql.registerTypeFields("User", [
 *   { name: "id", type: "ID", nullable: false },
 *   {
 *     name: "salary",
 *     type: "Decimal",
 *     nullable: false,
 *     requiresScope: "read:User.salary"  // Requires JWT scope
 *   },
 *   {
 *     name: "oldEmail",
 *     type: "String",
 *     nullable: true,
 *     deprecated: "Use email instead"  // Deprecation marker
 *   }
 * ]);
 * ```
 */
export function field(options: FieldMetadata): FieldMetadata {
  return options;
}

/**
 * Configuration for a Type decorator.
 */
export interface TypeConfig {
  description?: string;
  relay?: boolean;
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
  relay?: boolean;
  /** REST path pattern, e.g. "/users/{id}". Path params must match declared arguments. */
  restPath?: string;
  /** HTTP method for the REST endpoint. Defaults to "GET" for queries, "POST" for mutations. */
  restMethod?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
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
export function registerTypeFields(
  typeName: string,
  fields: Field[],
  description?: string,
  options?: {
    relay?: boolean;
    sqlSource?: string;
    jsonbColumn?: string;
    isError?: boolean;
    requiresRole?: string;
    implements?: string[];
  }
): void {
  SchemaRegistry.registerType(typeName, fields, description, options);
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

/**
 * Decorator to register a custom scalar with the schema.
 *
 * This decorator registers the scalar globally so it can be:
 * 1. Used in type annotations
 * 2. Exported to schema.json
 * 3. Validated at runtime
 *
 * @param target - CustomScalar subclass
 * @returns The original class (unmodified)
 * @throws If scalar name is not unique
 * @throws If class doesn't extend CustomScalar
 *
 * @example
 * ```typescript
 * @Scalar()
 * class Email extends CustomScalar {
 *   name = "Email"
 *
 *   serialize(value: unknown): string {
 *     return String(value)
 *   }
 *
 *   parseValue(value: unknown): string {
 *     const str = String(value)
 *     if (!str.includes("@")) {
 *       throw new Error("Invalid email")
 *     }
 *     return str
 *   }
 *
 *   parseLiteral(ast: unknown): string {
 *     if (ast && typeof ast === "object" && "value" in ast) {
 *       return this.parseValue((ast as any).value)
 *     }
 *     throw new Error("Invalid email literal")
 *   }
 * }
 *
 * // Use in type:
 * @Type()
 * class User {
 *   id: string
 *   email: Email  // Uses registered Email scalar
 *   name: string
 * }
 *
 * // Export schema
 * const schema = exportSchema("schema.json")
 * // schema contains: "customScalars": {"Email": {...}}
 * ```
 *
 * @remarks
 * - Decorator returns class unmodified (no runtime FFI)
 * - Registration is global (per-process)
 * - Name must be unique within schema
 * - Scalar must be defined before @Type that uses it
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function Scalar<T extends typeof CustomScalar>(target: T): T {
  // Verify that target extends CustomScalar
  if (!isCustomScalarSubclass(target)) {
    throw new TypeError(
      `@Scalar can only be applied to CustomScalar subclasses, got ${(target as any).name}`
    );
  }

  // Create instance to get the name
  const instance = new (target as any)();
  const scalarName = instance.name;

  // Validate name
  if (!scalarName || typeof scalarName !== "string") {
    throw new Error(
      `CustomScalar ${target.name} must have a 'name' property of type string`
    );
  }

  // Register with schema registry
  SchemaRegistry.registerScalar(scalarName, target, target.toString());

  return target;
}

/**
 * Check if a class extends CustomScalar.
 *
 * @internal
 */
function isCustomScalarSubclass(target: any): target is typeof CustomScalar {
  try {
    // Check prototype chain
    return target.prototype instanceof CustomScalar || target === CustomScalar;
  } catch {
    return false;
  }
}
