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

  /**
   * Register a GraphQL type.
   *
   * @param name - Type name (e.g., "User")
   * @param fields - List of field definitions
   * @param description - Optional type description
   */
  static registerType(name: string, fields: Field[], description?: string): void {
    this.types.set(name, {
      name,
      fields,
      description,
    });
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
    const cleanType = returnsList ? returnType.replace(/[[\]!]/g, "") : returnType;

    this.queries.set(name, {
      name,
      return_type: cleanType,
      returns_list: returnsList,
      nullable,
      arguments: args,
      description,
      ...config,
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
    const cleanType = returnsList ? returnType.replace(/[[\]!]/g, "") : returnType;

    this.mutations.set(name, {
      name,
      return_type: cleanType,
      returns_list: returnsList,
      nullable,
      arguments: args,
      description,
      ...config,
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
    this.unions.set(name, {
      name,
      member_types: memberTypes,
      description,
    });
  }

  /**
   * Get the complete schema as an object.
   *
   * @returns Schema object with types, queries, mutations, subscriptions, and analytics sections
   */
  static getSchema(): Schema {
    const schema: Schema = {
      types: Array.from(this.types.values()),
      queries: Array.from(this.queries.values()),
      mutations: Array.from(this.mutations.values()),
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
  }
}
