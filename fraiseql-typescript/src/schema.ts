/**
 * Schema export functionality for FraiseQL.
 *
 * This module provides functions to export the schema registry to JSON files
 * that can be consumed by the fraiseql-cli compiler.
 */

import * as fs from "fs";
import { SchemaRegistry, Schema } from "./registry";

/**
 * Configuration holder for temporary config during function definition.
 *
 * This is used by the config() function to store configuration that will be
 * applied by decorators.
 */
class ConfigHolder {
  static pendingConfig: Record<string, unknown> | null = null;
}

/**
 * Configuration helper for queries and mutations.
 *
 * This function is called inside decorated functions to specify SQL mapping
 * and other configuration. The config is stored temporarily and applied by
 * the decorator.
 *
 * @param config - Configuration options:
 *   - sqlSource: SQL view name (queries) or function name (mutations)
 *   - operation: Mutation operation type (CREATE, UPDATE, DELETE, CUSTOM)
 *   - autoParams: Auto-parameter configuration (limit, offset, where, order_by)
 *   - jsonbColumn: JSONB column name for flexible schemas
 *
 * @example
 * ```ts
 * @Query()
 * function users(limit: number = 10) {
 *   return config({
 *     sqlSource: "v_user",
 *     autoParams: { limit: true, offset: true, where: true }
 *   });
 * }
 *
 * @Mutation()
 * function createUser(name: string, email: string) {
 *   return config({
 *     sqlSource: "fn_create_user",
 *     operation: "CREATE"
 *   });
 * }
 * ```
 */
export function config(configObj: Record<string, unknown>): void {
  ConfigHolder.pendingConfig = configObj;
}

/**
 * Get pending configuration (internal use).
 *
 * @returns The pending configuration or null if none
 * @internal
 */
export function getPendingConfig(): Record<string, unknown> | null {
  const pending = ConfigHolder.pendingConfig;
  ConfigHolder.pendingConfig = null;
  return pending;
}

/**
 * Export the schema registry to a JSON file.
 *
 * This function should be called after all decorators have been processed
 * (typically at the end of the schema definition file).
 *
 * @param outputPath - Path to output schema.json file
 * @param options - Export options
 *
 * @example
 * ```ts
 * // At end of schema.ts
 * if (require.main === module) {
 *   exportSchema("schema.json");
 * }
 * ```
 *
 * Notes:
 * - Call this after all decorators have been applied
 * - The output schema.json is consumed by fraiseql-cli
 * - Pretty formatting is recommended for version control
 */
export function exportSchema(outputPath: string, options: { pretty?: boolean } = {}): void {
  const { pretty = true } = options;

  const schema = SchemaRegistry.getSchema();

  // Write to file
  const content = pretty ? JSON.stringify(schema, null, 2) + "\n" : JSON.stringify(schema);

  fs.writeFileSync(outputPath, content, { encoding: "utf-8" });

  // Print summary
  console.log(`✅ Schema exported to ${outputPath}`);
  console.log(`   Types: ${schema.types.length}`);
  console.log(`   Queries: ${schema.queries.length}`);
  console.log(`   Mutations: ${schema.mutations.length}`);

  if (schema.fact_tables) {
    console.log(`   Fact Tables: ${schema.fact_tables.length}`);
  }
  if (schema.aggregate_queries) {
    console.log(`   Aggregate Queries: ${schema.aggregate_queries.length}`);
  }
  if (schema.observers) {
    console.log(`   Observers: ${schema.observers.length}`);
  }
}

/**
 * Get the current schema as a dictionary (without exporting to file).
 *
 * @returns Schema object with "types", "queries", and "mutations"
 *
 * @example
 * ```ts
 * const schema = getSchemaDict();
 * console.log(schema.types);
 * // [{ name: "User", fields: [...] }, ...]
 * ```
 */
export function getSchemaDict(): Schema {
  return SchemaRegistry.getSchema();
}

/**
 * Export schema to a JSON string instead of a file.
 *
 * @param options - Export options
 * @returns JSON string representation of the schema
 *
 * @example
 * ```ts
 * const jsonString = exportSchemaToString({ pretty: true });
 * console.log(jsonString);
 * ```
 */
export function exportSchemaToString(options: { pretty?: boolean } = {}): string {
  const { pretty = true } = options;
  const schema = SchemaRegistry.getSchema();

  return pretty ? JSON.stringify(schema, null, 2) : JSON.stringify(schema);
}
