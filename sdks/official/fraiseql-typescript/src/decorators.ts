/**
 * Decorators for FraiseQL schema authoring (compile-time only).
 *
 * These decorators register type and query definitions with the schema registry
 * for JSON export. NO runtime behavior - only metadata collection.
 */

import {
  SchemaRegistry,
  ArgumentDefinition,
  Field,
  EnumValue,
  FieldMetadata,
  SubscriptionOptions,
} from "./registry";
import { CustomScalar } from "./scalars";
import { generateCrudOperations } from "./crud";

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
 * The one message every erased-metadata decorator raises.
 *
 * TypeScript erases types at runtime: a class decorator cannot read `id: number` off
 * `User`, and a method decorator cannot read `: User[]` off a query. `reflect-metadata`
 * does not rescue this either — it records `Array` for `User[]` with no element type,
 * and `Object` for any structural return.
 *
 * These four decorators used to paper over that by registering a guess: a type with
 * **zero fields**, and operations whose return type was the literal string
 * `"Query"`/`"Mutation"` with no arguments. No error, no warning. A developer following
 * the package's own documentation exported a plausible-looking `schema.json` describing
 * a schema they had not written (#733). For an authoring tool whose entire contract is
 * "the schema.json is the truth", a silent guess is the one unacceptable behaviour — so
 * they now refuse and name the function that does work.
 */
function erasedMetadata(decorator: string, replacement: string): never {
  throw new Error(
    `${decorator} cannot build a schema: TypeScript erases the type information it ` +
      `would need, so it can only guess — and it used to, registering placeholders that ` +
      `compiled into a schema you did not write (#733). Use ${replacement}() instead, ` +
      `which takes the same metadata explicitly. See the README's "Authoring" section ` +
      `for a worked example.`
  );
}

/**
 * Configuration for a Type decorator.
 */
export interface TypeConfig {
  description?: string;
  relay?: boolean;
  sqlSource?: string;
  crud?: boolean | string[];
  cascade?: boolean;
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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- required by legacy decorator API
  return function <T extends { new (...args: any[]): object }>(constructor: T) {
    // Registers the type's *name* and config only. TypeScript erases field types, so
    // this decorator cannot see `id: number` — the fields arrive later, either from
    // `registerTypeFields` or from `generateSchemaJson`'s class-instantiation pass on
    // the federation path, which is why this stays a marker rather than refusing
    // outright the way @Query and @Mutation now do.
    //
    // What used to be wrong is that a type whose fields never arrived was *exported*
    // anyway, as an object type with zero fields, silently (#733).
    // `validateSchemaBeforeExport` now refuses that at the moment it matters.
    SchemaRegistry.registerType(constructor.name, [], _config?.description, {
      sqlSource: _config?.sqlSource,
    });
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
export function Query(_config?: OperationConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy decorator target type
  return function (_target: any, _propertyKey: string, _descriptor: PropertyDescriptor): never {
    return erasedMetadata("@Query", "registerQuery");
  };
}

/**
 * Configuration for Mutation decorator.
 */
export interface MutationConfig extends OperationConfig {
  operation?: "CREATE" | "UPDATE" | "DELETE" | "CUSTOM";
  /**
   * Whether a successful run of this mutation writes a Change-Spine change-log row.
   *
   * Defaults to `true`. Set `changelog: false` to opt this single mutation out of the
   * in-transaction change-log outbox while the rest of the schema keeps logging.
   * Omitting it leaves the key out of the JSON, and the compiler defaults it to `true`
   * (a row is written only when the global `[changelog]` switch and this per-mutation
   * flag are both on).
   */
  changelog?: boolean;
  /**
   * Whether a successful, state-changing run of this mutation also records the
   * changed entity's pre-image (before-state) into the Change-Spine
   * `object_data_before` column, alongside the after-state in `object_data` —
   * sourced from an optional `entity_before` on the mutation's
   * `app.mutation_response`.
   *
   * Defaults to `false`. Set `changelogPreImage: true` for audit-sensitive
   * mutations that need an inline Debezium-style `{before, after}` on the single
   * change event. Omitting it leaves the key out of the JSON, and the compiler
   * defaults it to `false` (after-image only, today's behavior).
   */
  changelogPreImage?: boolean;
  /**
   * How the GraphQL `input` argument is passed to the SQL function:
   * `"flatten"` (positional columns, the default) or `"jsonb"` (the whole input
   * as one `jsonb` argument).
   *
   * Orthogonal to `operation`: set `inputStyle: "jsonb"` so a backend using the
   * single-`jsonb`-wrapper convention (`fn(input_payload jsonb, …)`) can register
   * the real DML verb (`CREATE`/`DELETE`/`CUSTOM`) instead of being forced to
   * `UPDATE` purely to opt into single-JSONB input passing — the Change Spine then
   * records the true `modification_type`. Omitting it leaves the key out of the
   * JSON, and the compiler defaults it to `"flatten"`.
   */
  inputStyle?: "flatten" | "jsonb";
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
export function Mutation(_config?: MutationConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy decorator target type
  return function (_target: any, _propertyKey: string, _descriptor: PropertyDescriptor): never {
    return erasedMetadata("@Mutation", "registerMutation");
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
    crud?: boolean | string[];
    cascade?: boolean;
  }
): void {
  SchemaRegistry.registerType(typeName, fields, description, options);
  if (options?.crud) {
    generateCrudOperations(typeName, fields, options.crud, options.sqlSource, options.cascade);
  }
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
 *
 * `entityType` is the authoring spelling of the compiler's `return_type`. There is no
 * `operation`: the runtime subscription model filters on argument-to-JSON-path
 * conditions, not on a DML verb (#1024).
 */
export interface SubscriptionConfig extends SubscriptionOptions {
  entityType?: string;
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
 *   "return_type": "Order",
 *   "arguments": [
 *     {"name": "userId", "type": "String", "nullable": true}
 *   ],
 *   "topic": "order_events"
 * }
 * ```
 */
export function Subscription(_config?: SubscriptionConfig) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy decorator target type
  return function (_target: any, _propertyKey: string, _descriptor: PropertyDescriptor): never {
    return erasedMetadata("@Subscription", "registerSubscription");
  };
}

/**
 * Helper function to manually register subscription with full metadata.
 *
 * @param name - Subscription name
 * @param entityType - Entity type being subscribed to (the return type)
 * @param args - Argument definitions (filters)
 * @param description - Optional subscription description
 * @param options - topic, filter, projected fields, deprecation
 *
 * @example
 * ```ts
 * registerSubscription(
 *   "orderCreated",
 *   "Order",
 *   [
 *     { name: "userId", type: "String", nullable: true }
 *   ],
 *   "Subscribe to new orders",
 *   {
 *     topic: "order_events",
 *     filter: { conditions: [{ argument: "userId", path: "$.user_id" }] },
 *   }
 * );
 * ```
 */
export function registerSubscription(
  name: string,
  entityType: string,
  args: ArgumentDefinition[],
  description?: string,
  options?: SubscriptionOptions
): void {
  SchemaRegistry.registerSubscription(name, entityType, args, description, options);
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
export function Scalar<T extends typeof CustomScalar>(target: T): T {
  // Verify that target extends CustomScalar
  if (!isCustomScalarSubclass(target)) {
    // Use (target as object) to avoid narrowing to never before the throw
    const name = (target as { name?: string }).name ?? "(unknown)";
    throw new TypeError(
      `@Scalar can only be applied to CustomScalar subclasses, got ${name}`
    );
  }

  // Create instance to get the name; double-cast through unknown to satisfy abstract→concrete
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- instantiating abstract class subclass
  const instance = new (target as unknown as new () => any)();
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
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- required for prototype chain check
function isCustomScalarSubclass(target: any): target is typeof CustomScalar {
  try {
    // Check prototype chain
    return target.prototype instanceof CustomScalar || target === CustomScalar;
  } catch {
    return false;
  }
}
