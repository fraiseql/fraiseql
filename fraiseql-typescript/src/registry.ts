/**
 * Global schema registry for collecting types, queries, and mutations.
 *
 * This module maintains a singleton registry of all definitions made via decorators,
 * which is then exported to schema.json for compilation.
 */

/**
 * Field definition in a GraphQL type.
 */
export interface Field {
  name: string;
  type: string;
  nullable: boolean;
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
 * Complete schema definition.
 */
export interface Schema {
  types: TypeDefinition[];
  queries: QueryDefinition[];
  mutations: MutationDefinition[];
  fact_tables?: FactTableDefinition[];
  aggregate_queries?: AggregateQueryDefinition[];
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
  private static factTables: Map<string, FactTableDefinition> = new Map();
  private static aggregateQueries: Map<string, AggregateQueryDefinition> =
    new Map();

  /**
   * Register a GraphQL type.
   *
   * @param name - Type name (e.g., "User")
   * @param fields - List of field definitions
   * @param description - Optional type description
   */
  static registerType(
    name: string,
    fields: Field[],
    description?: string
  ): void {
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
    const cleanType = returnsList ? returnType.replace(/[\[\]!]/g, "") : returnType;

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
    const cleanType = returnsList ? returnType.replace(/[\[\]!]/g, "") : returnType;

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
   * Get the complete schema as an object.
   *
   * @returns Schema object with types, queries, mutations, and analytics sections
   */
  static getSchema(): Schema {
    const schema: Schema = {
      types: Array.from(this.types.values()),
      queries: Array.from(this.queries.values()),
      mutations: Array.from(this.mutations.values()),
    };

    if (this.factTables.size > 0) {
      schema.fact_tables = Array.from(this.factTables.values());
    }

    if (this.aggregateQueries.size > 0) {
      schema.aggregate_queries = Array.from(this.aggregateQueries.values());
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
    this.factTables.clear();
    this.aggregateQueries.clear();
  }
}
