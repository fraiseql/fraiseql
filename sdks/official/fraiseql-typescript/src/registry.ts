/**
 * Global schema registry for collecting types, queries, and mutations.
 *
 * This module maintains a singleton registry of all definitions made via decorators,
 * which is then exported to schema.json for compilation.
 */

/**
 * Field metadata for access control and deprecation.
 */
export interface FieldMetadata {
  requiresScope?: string | string[];
  deprecated?: boolean | string;
  description?: string;
}

/**
 * Field definition in a GraphQL type.
 */
export interface Field extends FieldMetadata {
  name: string;
  type: string;
  nullable: boolean;
  default?: unknown;
}

/**
 * GraphQL type definition.
 */
export interface TypeDefinition {
  name: string;
  fields: Field[];
  description?: string;
  relay?: boolean;
  sql_source?: string;
  jsonb_column?: string;
  is_error?: boolean;
  requires_role?: string;
  implements?: string[];
  tenant_scoped?: boolean;
  crud?: boolean | string[];
}

/**
 * Argument definition for a query or mutation.
 */
export interface ArgumentDefinition {
  name: string;
  type: string;
  nullable: boolean;
  default?: unknown;
}

/**
 * GraphQL query definition.
 */
export interface QueryDefinition {
  name: string;
  return_type: string;
  returns_list: boolean;
  nullable: boolean;
  arguments: ArgumentDefinition[];
  description?: string;
  [key: string]: unknown; // For additional config like sql_source
}

/**
 * GraphQL mutation definition.
 */
export interface MutationDefinition {
  name: string;
  return_type: string;
  returns_list: boolean;
  nullable: boolean;
  arguments: ArgumentDefinition[];
  description?: string;
  operation?: string;
  [key: string]: unknown; // For additional config like sql_source
}

/**
 * Measure definition in a fact table.
 */
export interface Measure {
  name: string;
  sql_type: string;
  nullable: boolean;
}

/**
 * Dimension path in a fact table.
 */
export interface DimensionPath {
  name: string;
  json_path: string;
  data_type: string;
}

/**
 * Dimension definition in a fact table.
 */
export interface Dimension {
  name: string;
  paths: DimensionPath[];
}

/**
 * Denormalized filter in a fact table.
 */
export interface DenormalizedFilter {
  name: string;
  sql_type: string;
  indexed: boolean;
}

/**
 * Fact table definition.
 */
export interface FactTableDefinition {
  table_name: string;
  measures: Measure[];
  dimensions: Dimension;
  denormalized_filters: DenormalizedFilter[];
}

/**
 * Aggregate query definition.
 */
export interface AggregateQueryDefinition {
  name: string;
  fact_table: string;
  auto_group_by: boolean;
  auto_aggregates: boolean;
  description?: string;
}

/**
 * GraphQL subscription definition.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 */
export interface SubscriptionDefinition {
  name: string;
  entity_type: string;
  nullable: boolean;
  arguments: ArgumentDefinition[];
  description?: string;
  topic?: string;
  operation?: string;
  [key: string]: unknown; // For additional config
}

/**
 * Observer action definition.
 */
export interface ObserverAction {
  type: "webhook" | "slack" | "email";
  [key: string]: unknown;
}

/**
 * Observer retry configuration.
 */
export interface ObserverRetryConfig {
  max_attempts: number;
  backoff_strategy: string;
  initial_delay_ms: number;
  max_delay_ms: number;
}

/**
 * Observer definition.
 */
export interface ObserverDefinition {
  name: string;
  entity: string;
  event: string;
  actions: ObserverAction[];
  condition?: string;
  retry: ObserverRetryConfig;
}

/**
 * Enum value definition.
 */
export interface EnumValue {
  name: string;
  deprecated?: { reason: string };
}

/**
 * GraphQL enum definition.
 */
export interface EnumDefinition {
  name: string;
  values: EnumValue[];
  description?: string;
}

/**
 * GraphQL interface definition.
 */
export interface InterfaceDefinition {
  name: string;
  fields: Field[];
  description?: string;
}

/**
 * GraphQL input type definition.
 */
export interface InputTypeDefinition {
  name: string;
  fields: Array<Field & { default?: unknown }>;
  description?: string;
}

/**
 * GraphQL union definition.
 */
export interface UnionDefinition {
  name: string;
  member_types: string[];
  description?: string;
}

/**
 * Complete schema definition.
 */
export interface Schema {
  types: TypeDefinition[];
  queries: QueryDefinition[];
  mutations: MutationDefinition[];
  subscriptions: SubscriptionDefinition[];
  enums?: EnumDefinition[];
  interfaces?: InterfaceDefinition[];
  input_types?: InputTypeDefinition[];
  unions?: UnionDefinition[];
  fact_tables?: FactTableDefinition[];
  aggregate_queries?: AggregateQueryDefinition[];
  observers?: ObserverDefinition[];
  /** Apollo Federation v2 metadata, included when generateSchemaJson() is used. */
  federation?: { enabled: boolean; version: string; [key: string]: unknown };
}

/**
 * Normalise camelCase config keys to snake_case so the Rust compiler receives
 * the expected field names.  Handles all known camelCase keys used in decorator
 * config objects and performs structural transformations for inject and deprecated.
 */
/**
 * Valid HTTP methods for REST annotations.
 */
const VALID_REST_METHODS = new Set(["GET", "POST", "PUT", "PATCH", "DELETE"]);

function normaliseConfig(
  config: Record<string, unknown>,
  defaultRestMethod: string = "GET"
): Record<string, unknown> {
  const keyMap: Record<string, string> = {
    sqlSource: "sql_source",
    autoParams: "auto_params",
    jsonbColumn: "jsonb_column",
    cacheTtlSeconds: "cache_ttl_seconds",
    invalidatesViews: "invalidates_views",
    invalidatesFactTables: "invalidates_fact_tables",
    relayCursorColumn: "relay_cursor_column",
    relayCursorType: "relay_cursor_type",
    requiresRole: "requires_role",
    additionalViews: "additional_views",
  };
  const result: Record<string, unknown> = {};
  let restPath: string | undefined;
  let restMethod: string | undefined;
  for (const [key, value] of Object.entries(config)) {
    if (key === "restPath") {
      restPath = value as string;
    } else if (key === "restMethod") {
      restMethod = value as string;
    } else if (key === "inject" && value !== null && typeof value === "object") {
      // Transform { param: "jwt:claim" } → inject_params: { param: { source: "jwt", claim: "claim" } }
      const injected: Record<string, { source: string; claim: string }> = {};
      for (const [param, spec] of Object.entries(value as Record<string, string>)) {
        const colonIdx = spec.indexOf(":");
        if (colonIdx > 0) {
          injected[param] = { source: spec.slice(0, colonIdx), claim: spec.slice(colonIdx + 1) };
        }
      }
      result["inject_params"] = injected;
    } else if (key === "deprecated" && typeof value === "string") {
      // Transform deprecated: "reason" → deprecation: { reason: "reason" }
      result["deprecation"] = { reason: value };
    } else {
      result[keyMap[key] ?? key] = value;
    }
  }

  // Handle REST annotations
  if (restMethod !== undefined && restPath === undefined) {
    throw new Error("restMethod requires restPath to be set");
  }
  if (restPath !== undefined) {
    const method = (restMethod ?? defaultRestMethod).toUpperCase();
    if (!VALID_REST_METHODS.has(method)) {
      throw new Error(
        `Invalid REST method '${method}'. Must be one of: ${[...VALID_REST_METHODS].join(", ")}`
      );
    }
    result["rest"] = { path: restPath, method };
  }

  return result;
}

/**
 * Convert PascalCase to snake_case.
 *
 * @param name - PascalCase name (e.g., "OrderItem")
 * @returns snake_case name (e.g., "order_item")
 */
export function pascalToSnake(name: string): string {
  return name
    .replace(/([A-Z])/g, "_$1")
    .toLowerCase()
    .replace(/^_/, "");
}

/**
 * Pluralize a snake_case name using basic English rules.
 *
 * Rules (ordered):
 * 1. Already ends in 's' (but not 'ss') -> no change (e.g. 'statistics')
 * 2. Ends in 'ss', 'sh', 'ch', 'x', 'z' -> append 'es'
 * 3. Ends in consonant + 'y' -> replace 'y' with 'ies'
 * 4. Default -> append 's'
 */
export function pluralize(name: string): string {
  if (name.endsWith("s") && !name.endsWith("ss")) return name;
  if (/(?:ss|sh|ch|x|z)$/.test(name)) return name + "es";
  if (name.length >= 2 && name.endsWith("y") && !"aeiou".includes(name[name.length - 2])) {
    return name.slice(0, -1) + "ies";
  }
  return name + "s";
}

/**
 * Parse an inject shorthand string into a structured inject param.
 *
 * @param spec - Shorthand like "jwt:claim_name"
 * @returns Structured inject param or undefined if invalid
 */
function parseInjectSpec(spec: string): { source: string; claim: string } | undefined {
  const colonIdx = spec.indexOf(":");
  if (colonIdx > 0) {
    return { source: spec.slice(0, colonIdx), claim: spec.slice(colonIdx + 1) };
  }
  return undefined;
}

/**
 * Auto-generate CRUD queries and mutations for a type following the Trinity pattern.
 *
 * @param typeName - PascalCase type name (e.g., "OrderItem")
 * @param fields - Field definitions for the type
 * @param crud - true for all operations, or array of operation names
 */
function generateCrudOperations(
  typeName: string,
  fields: Field[],
  crud: boolean | string[],
  sqlSource?: string
): void {
  const allOps = ["read", "create", "update", "delete"];
  const ops: string[] = crud === true ? allOps : (Array.isArray(crud) ? crud : []);
  const snake = pascalToSnake(typeName);
  const view = sqlSource ?? `v_${snake}`;
  const pk = fields[0]; // First field is the PK by convention

  if (!pk) return;

  if (ops.includes("read")) {
    // Single get by ID (nullable)
    SchemaRegistry.registerQuery(
      snake,
      typeName,
      false,
      true,
      [{ name: pk.name, type: pk.type, nullable: false }],
      `Get ${typeName} by ID`,
      { sqlSource: view }
    );
    // List query
    SchemaRegistry.registerQuery(
      pluralize(snake),
      typeName,
      true,
      false,
      [],
      `List ${typeName} records`,
      { sqlSource: view, autoParams: { limit: true, offset: true, where: true, order_by: true } }
    );
  }

  if (ops.includes("create")) {
    const args: ArgumentDefinition[] = fields.map((f) => ({
      name: f.name,
      type: f.type,
      nullable: f.nullable,
    }));
    SchemaRegistry.registerMutation(
      `create_${snake}`,
      typeName,
      false,
      false,
      args,
      `Create a new ${typeName}`,
      { sqlSource: `fn_create_${snake}`, operation: "CREATE" }
    );
  }

  if (ops.includes("update")) {
    const args: ArgumentDefinition[] = fields.map((f, idx) => ({
      name: f.name,
      type: f.type,
      nullable: idx === 0 ? false : true, // PK required, others nullable
    }));
    SchemaRegistry.registerMutation(
      `update_${snake}`,
      typeName,
      false,
      false,
      args,
      `Update an existing ${typeName}`,
      { sqlSource: `fn_update_${snake}`, operation: "UPDATE" }
    );
  }

  if (ops.includes("delete")) {
    SchemaRegistry.registerMutation(
      `delete_${snake}`,
      typeName,
      false,
      false,
      [{ name: pk.name, type: pk.type, nullable: false }],
      `Delete a ${typeName}`,
      { sqlSource: `fn_delete_${snake}`, operation: "DELETE" }
    );
  }
}

/**
 * Global schema registry (singleton).
 *
 * Maintains maps of all registered types, queries, mutations, and analytics definitions.
 * These are collected during decorator evaluation and exported to schema.json.
 */
export class SchemaRegistry {
  private static types: Map<string, TypeDefinition> = new Map();
  private static queries: Map<string, QueryDefinition> = new Map();
  private static mutations: Map<string, MutationDefinition> = new Map();
  private static subscriptions: Map<string, SubscriptionDefinition> = new Map();
  private static enums: Map<string, EnumDefinition> = new Map();
  private static interfaces: Map<string, InterfaceDefinition> = new Map();
  private static inputTypes: Map<string, InputTypeDefinition> = new Map();
  private static unions: Map<string, UnionDefinition> = new Map();
  private static factTables: Map<string, FactTableDefinition> = new Map();
  private static aggregateQueries: Map<string, AggregateQueryDefinition> = new Map();
  private static observers: Map<string, ObserverDefinition> = new Map();
  private static customScalars: Map<string, { class: any; description?: string }> = new Map();
  private static injectDefaultsBase: Map<string, string> = new Map();
  private static injectDefaultsQueries: Map<string, string> = new Map();
  private static injectDefaultsMutations: Map<string, string> = new Map();

  /**
   * Set inject defaults loaded from TOML config.
   *
   * @param base - Base defaults applied to all operations
   * @param queries - Defaults applied only to queries
   * @param mutations - Defaults applied only to mutations
   */
  static setInjectDefaults(
    base: Map<string, string>,
    queries: Map<string, string>,
    mutations: Map<string, string>
  ): void {
    this.injectDefaultsBase = base;
    this.injectDefaultsQueries = queries;
    this.injectDefaultsMutations = mutations;
  }

  /**
   * Register a GraphQL type.
   *
   * @param name - Type name (e.g., "User")
   * @param fields - List of field definitions
   * @param description - Optional type description
   * @param options - Additional type options
   */
  static registerType(
    name: string,
    fields: Field[],
    description?: string,
    options?: {
      relay?: boolean;
      sqlSource?: string;
      jsonbColumn?: string;
      isError?: boolean;
      requiresRole?: string;
      implements?: string[];
      tenantScoped?: boolean;
      crud?: boolean | string[];
    }
  ): void {
    if (this.types.has(name)) {
      throw new Error(
        `Type '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    const typeDef: TypeDefinition = { name, fields, description };
    if (options?.relay) typeDef.relay = true;
    if (options?.sqlSource) typeDef.sql_source = options.sqlSource;
    if (options?.jsonbColumn) typeDef.jsonb_column = options.jsonbColumn;
    if (options?.isError) typeDef.is_error = true;
    if (options?.requiresRole) typeDef.requires_role = options.requiresRole;
    if (options?.implements) typeDef.implements = options.implements;
    if (options?.tenantScoped) typeDef.tenant_scoped = true;
    this.types.set(name, typeDef);

    // Auto-generate CRUD operations if requested
    if (options?.crud) {
      generateCrudOperations(name, fields, options.crud, options.sqlSource);
    }
  }

  /**
   * Register a GraphQL query.
   *
   * @param name - Query name
   * @param returnType - Return type name
   * @param returnsList - Whether query returns a list
   * @param nullable - Whether result can be null
   * @param args - List of argument definitions
   * @param description - Optional query description
   * @param config - Additional configuration (sql_source, etc.)
   */
  static registerQuery(
    name: string,
    returnType: string,
    returnsList: boolean,
    nullable: boolean,
    args: ArgumentDefinition[],
    description?: string,
    config?: Record<string, unknown>
  ): void {
    if (this.queries.has(name)) {
      throw new Error(
        `Query '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    const cleanType = returnsList ? returnType.replace(/[[\]!]/g, "") : returnType;

    // Relay validation — fail fast at authoring time
    if (config?.relay) {
      if (!returnsList) {
        throw new Error(
          `registerQuery('${name}'): relay: true requires returns_list to be true. ` +
          "Relay connections only apply to list queries."
        );
      }
      if (!config.sqlSource) {
        throw new Error(
          `registerQuery('${name}'): relay: true requires sqlSource to be set. ` +
          "The compiler needs the view name to derive the cursor column."
        );
      }
      // Strip limit/offset from autoParams — relay uses first/after/last/before instead
      if (config.autoParams) {
        const ap = { ...(config.autoParams as Record<string, boolean>) };
        delete ap["limit"];
        delete ap["offset"];
        config = { ...config, autoParams: ap };
      }
    }

    // Normalise camelCase config keys to snake_case for the compiler
    const normalisedConfig = config ? normaliseConfig(config, "GET") : undefined;

    this.queries.set(name, {
      name,
      return_type: cleanType,
      returns_list: returnsList,
      nullable,
      arguments: args,
      description,
      ...normalisedConfig,
    });
  }

  /**
   * Register a GraphQL mutation.
   *
   * @param name - Mutation name
   * @param returnType - Return type name
   * @param returnsList - Whether mutation returns a list
   * @param nullable - Whether result can be null
   * @param args - List of argument definitions
   * @param description - Optional mutation description
   * @param config - Additional configuration (sql_source, operation, etc.)
   */
  static registerMutation(
    name: string,
    returnType: string,
    returnsList: boolean,
    nullable: boolean,
    args: ArgumentDefinition[],
    description?: string,
    config?: Record<string, unknown>
  ): void {
    if (this.mutations.has(name)) {
      throw new Error(
        `Mutation '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    const cleanType = returnsList ? returnType.replace(/[[\]!]/g, "") : returnType;

    // Normalise camelCase config keys to snake_case for the compiler
    const normalisedConfig = config ? normaliseConfig(config, "POST") : undefined;

    this.mutations.set(name, {
      name,
      return_type: cleanType,
      returns_list: returnsList,
      nullable,
      arguments: args,
      description,
      ...normalisedConfig,
    });
  }

  /**
   * Register a GraphQL subscription.
   *
   * Subscriptions in FraiseQL are compiled projections of database events.
   * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
   *
   * @param name - Subscription name
   * @param entityType - Entity type name being subscribed to
   * @param nullable - Whether result can be null
   * @param args - List of argument definitions (filters)
   * @param description - Optional subscription description
   * @param config - Additional configuration (topic, operation, etc.)
   */
  static registerSubscription(
    name: string,
    entityType: string,
    nullable: boolean,
    args: ArgumentDefinition[],
    description?: string,
    config?: Record<string, unknown>
  ): void {
    if (this.subscriptions.has(name)) {
      throw new Error(
        `Subscription '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.subscriptions.set(name, {
      name,
      entity_type: entityType,
      nullable,
      arguments: args,
      description,
      ...config,
    });
  }

  /**
   * Register a fact table definition.
   *
   * @param tableName - Fact table name
   * @param measures - List of measure definitions
   * @param dimensions - Dimension metadata
   * @param denormalizedFilters - List of denormalized filter definitions
   */
  static registerFactTable(
    tableName: string,
    measures: Measure[],
    dimensions: Dimension,
    denormalizedFilters: DenormalizedFilter[]
  ): void {
    this.factTables.set(tableName, {
      table_name: tableName,
      measures,
      dimensions,
      denormalized_filters: denormalizedFilters,
    });
  }

  /**
   * Register an aggregate query definition.
   *
   * @param name - Query name
   * @param factTable - Fact table name
   * @param autoGroupBy - Auto-generate groupBy fields
   * @param autoAggregates - Auto-generate aggregate fields
   * @param description - Optional query description
   */
  static registerAggregateQuery(
    name: string,
    factTable: string,
    autoGroupBy: boolean,
    autoAggregates: boolean,
    description?: string
  ): void {
    this.aggregateQueries.set(name, {
      name,
      fact_table: factTable,
      auto_group_by: autoGroupBy,
      auto_aggregates: autoAggregates,
      description,
    });
  }

  /**
   * Register an observer.
   *
   * @param name - Observer function name
   * @param entity - Entity type to observe
   * @param event - Event type (INSERT, UPDATE, or DELETE)
   * @param actions - List of action configurations
   * @param condition - Optional condition expression
   * @param retry - Retry configuration
   */
  static registerObserver(
    name: string,
    entity: string,
    event: string,
    actions: ObserverAction[],
    condition?: string,
    retry?: ObserverRetryConfig
  ): void {
    this.observers.set(name, {
      name,
      entity,
      event: event.toUpperCase(),
      actions,
      condition,
      retry: retry || {
        max_attempts: 3,
        backoff_strategy: "exponential",
        initial_delay_ms: 100,
        max_delay_ms: 60000,
      },
    });
  }

  /**
   * Register a GraphQL enum type.
   *
   * @param name - Enum name (e.g., "OrderStatus")
   * @param values - List of enum value definitions
   * @param description - Optional enum description
   */
  static registerEnum(name: string, values: EnumValue[], description?: string): void {
    if (this.enums.has(name)) {
      throw new Error(
        `Enum '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.enums.set(name, {
      name,
      values,
      description,
    });
  }

  /**
   * Register a GraphQL interface type.
   *
   * @param name - Interface name (e.g., "Node")
   * @param fields - List of field definitions
   * @param description - Optional interface description
   */
  static registerInterface(name: string, fields: Field[], description?: string): void {
    if (this.interfaces.has(name)) {
      throw new Error(
        `Interface '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.interfaces.set(name, {
      name,
      fields,
      description,
    });
  }

  /**
   * Register a GraphQL input type.
   *
   * @param name - Input type name (e.g., "CreateUserInput")
   * @param fields - List of field definitions with optional defaults
   * @param description - Optional input type description
   */
  static registerInputType(
    name: string,
    fields: Array<Field & { default?: unknown }>,
    description?: string
  ): void {
    if (this.inputTypes.has(name)) {
      throw new Error(
        `Input type '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.inputTypes.set(name, {
      name,
      fields,
      description,
    });
  }

  /**
   * Register a GraphQL union type.
   *
   * @param name - Union name (e.g., "SearchResult")
   * @param memberTypes - List of member type names
   * @param description - Optional union description
   */
  static registerUnion(name: string, memberTypes: string[], description?: string): void {
    if (this.unions.has(name)) {
      throw new Error(
        `Union '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.unions.set(name, {
      name,
      member_types: memberTypes,
      description,
    });
  }

  /**
   * Register a custom scalar.
   *
   * @param name - Scalar name (e.g., "Email")
   * @param scalarClass - The CustomScalar subclass
   * @param description - Optional scalar description
   *
   * @throws If scalar name is not unique
   */
  static registerScalar(name: string, scalarClass: any, description?: string): void {
    if (this.customScalars.has(name)) {
      throw new Error(
        `Scalar '${name}' is already registered. Each name must be unique within a schema.`
      );
    }
    this.customScalars.set(name, { class: scalarClass, description });
  }

  /**
   * Get all registered custom scalars.
   *
   * @returns Map of scalar names to CustomScalar classes
   */
  static getCustomScalars(): Map<string, any> {
    const result = new Map<string, any>();
    for (const [name, { class: scalarClass }] of this.customScalars) {
      result.set(name, scalarClass);
    }
    return result;
  }

  /**
   * Get the complete schema as an object.
   *
   * @returns Schema object with types, queries, mutations, subscriptions, and analytics sections
   */
  static getSchema(): Schema {
    const queries = Array.from(this.queries.values());
    const mutations = Array.from(this.mutations.values());

    // Merge inject defaults into queries
    if (this.injectDefaultsBase.size > 0 || this.injectDefaultsQueries.size > 0) {
      for (const query of queries) {
        const merged: Record<string, string> = {};
        for (const [k, v] of this.injectDefaultsBase) merged[k] = v;
        for (const [k, v] of this.injectDefaultsQueries) merged[k] = v;
        const existing = (query.inject_params ?? {}) as Record<string, unknown>;
        const result: Record<string, unknown> = {};
        for (const [param, spec] of Object.entries(merged)) {
          if (!(param in existing)) {
            const parsed = parseInjectSpec(spec);
            if (parsed) result[param] = parsed;
          }
        }
        for (const [param, val] of Object.entries(existing)) {
          result[param] = val;
        }
        if (Object.keys(result).length > 0) {
          query.inject_params = result;
        }
      }
    }

    // Merge inject defaults into mutations
    if (this.injectDefaultsBase.size > 0 || this.injectDefaultsMutations.size > 0) {
      for (const mutation of mutations) {
        const merged: Record<string, string> = {};
        for (const [k, v] of this.injectDefaultsBase) merged[k] = v;
        for (const [k, v] of this.injectDefaultsMutations) merged[k] = v;
        const existing = (mutation.inject_params ?? {}) as Record<string, unknown>;
        const result: Record<string, unknown> = {};
        for (const [param, spec] of Object.entries(merged)) {
          if (!(param in existing)) {
            const parsed = parseInjectSpec(spec);
            if (parsed) result[param] = parsed;
          }
        }
        for (const [param, val] of Object.entries(existing)) {
          result[param] = val;
        }
        if (Object.keys(result).length > 0) {
          mutation.inject_params = result;
        }
      }
    }

    const schema: Schema = {
      types: Array.from(this.types.values()),
      queries,
      mutations,
      subscriptions: Array.from(this.subscriptions.values()),
    };

    if (this.enums.size > 0) {
      schema.enums = Array.from(this.enums.values());
    }

    if (this.interfaces.size > 0) {
      schema.interfaces = Array.from(this.interfaces.values());
    }

    if (this.inputTypes.size > 0) {
      schema.input_types = Array.from(this.inputTypes.values());
    }

    if (this.unions.size > 0) {
      schema.unions = Array.from(this.unions.values());
    }

    if (this.factTables.size > 0) {
      schema.fact_tables = Array.from(this.factTables.values());
    }

    if (this.aggregateQueries.size > 0) {
      schema.aggregate_queries = Array.from(this.aggregateQueries.values());
    }

    if (this.observers.size > 0) {
      schema.observers = Array.from(this.observers.values());
    }

    if (this.customScalars.size > 0) {
      const customScalars: Record<string, any> = {};
      for (const [name, { class: scalarClass, description }] of this.customScalars) {
        customScalars[name] = {
          name,
          description: description || scalarClass.__doc__ || "Custom scalar",
          validate: true,
        };
      }
      (schema as any).customScalars = customScalars;
    }

    return schema;
  }

  /**
   * Clear the registry (useful for testing).
   */
  static clear(): void {
    this.types.clear();
    this.queries.clear();
    this.mutations.clear();
    this.subscriptions.clear();
    this.enums.clear();
    this.interfaces.clear();
    this.inputTypes.clear();
    this.unions.clear();
    this.factTables.clear();
    this.aggregateQueries.clear();
    this.observers.clear();
    this.customScalars.clear();
    this.injectDefaultsBase.clear();
    this.injectDefaultsQueries.clear();
    this.injectDefaultsMutations.clear();
  }
}
