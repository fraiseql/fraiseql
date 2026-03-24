/**
 * Schema export functionality for FraiseQL.
 *
 * This module provides functions to export the schema registry to JSON files
 * that can be consumed by the fraiseql-cli compiler.
 */

import * as fs from "fs";
import * as path from "path";
import { SchemaRegistry, Schema } from "./registry";

const BUILTIN_SCALARS = new Set(["String", "Int", "Float", "Boolean", "ID"]);

function validateSchemaBeforeExport(schema: Schema): void {
  const registeredTypeNames = new Set<string>([
    ...schema.types.map((t) => t.name),
    ...(schema.enums ?? []).map((e) => e.name),
    ...BUILTIN_SCALARS,
  ]);

  const errors: string[] = [];

  for (const query of schema.queries) {
    const ret = query.return_type;
    if (ret && !registeredTypeNames.has(ret)) {
      errors.push(
        `Query '${query.name}' has return type '${ret}' which is not a registered type.`
      );
    }
  }

  for (const mutation of schema.mutations) {
    const ret = mutation.return_type;
    if (ret && !registeredTypeNames.has(ret)) {
      errors.push(
        `Mutation '${mutation.name}' has return type '${ret}' which is not a registered type.`
      );
    }
  }

  if (errors.length > 0) {
    throw new Error(
      `Schema validation failed before export. Fix the following errors:\n  - ${errors.join("\n  - ")}`
    );
  }
}

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

  validateSchemaBeforeExport(schema);

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

/**
 * Export ONLY types to a minimal types.json file (TOML-based workflow).
 *
 * This is the new minimal export function for the TOML-based configuration approach.
 * It exports only the type definitions (types, enums, input_types, interfaces) without
 * queries, mutations, federation, security, observers, or analytics metadata.
 *
 * All configuration moves to fraiseql.toml, which is merged with this types.json
 * by the fraiseql-cli compile command.
 *
 * @param outputPath - Path to output types.json file
 * @param options - Export options
 *
 * @example
 * ```ts
 * // At end of schema.ts
 * if (require.main === module) {
 *   exportTypes("user_types.json");
 * }
 * ```
 *
 * Notes:
 * - Call this after all decorators have been applied
 * - The output types.json contains only type definitions
 * - Queries, mutations, and all configuration moves to fraiseql.toml
 * - Use with: fraiseql compile fraiseql.toml --types user_types.json
 */
export function exportTypes(outputPath: string, options: { pretty?: boolean } = {}): void {
  const { pretty = true } = options;

  const fullSchema = SchemaRegistry.getSchema();

  // Extract only types, enums, input_types, interfaces
  // (no queries/mutations/federation/security/observers/analytics)
  const minimalSchema = {
    types: fullSchema.types || [],
    enums: fullSchema.enums || [],
    input_types: fullSchema.input_types || [],
    interfaces: fullSchema.interfaces || [],
  };

  // Write to file
  const content = pretty
    ? JSON.stringify(minimalSchema, null, 2) + "\n"
    : JSON.stringify(minimalSchema);

  fs.writeFileSync(outputPath, content, { encoding: "utf-8" });

  // Print summary
  console.log(`✅ Types exported to ${outputPath}`);
  console.log(`   Types: ${minimalSchema.types.length}`);
  if (minimalSchema.enums.length > 0) {
    console.log(`   Enums: ${minimalSchema.enums.length}`);
  }
  if (minimalSchema.input_types.length > 0) {
    console.log(`   Input types: ${minimalSchema.input_types.length}`);
  }
  if (minimalSchema.interfaces.length > 0) {
    console.log(`   Interfaces: ${minimalSchema.interfaces.length}`);
  }
  console.log(`   → Use with: fraiseql compile fraiseql.toml --types ${outputPath}`);
}

/**
 * Parse a minimal subset of TOML (simple key=value pairs and [section] headers).
 *
 * Supports the structure needed for inject_defaults:
 *   [inject_defaults]
 *   key = "value"
 *   [inject_defaults.queries]
 *   key = "value"
 *   [inject_defaults.mutations]
 *   key = "value"
 *
 * @param content - TOML file content
 * @returns Parsed sections as nested maps
 * @internal
 */
function parseMinimalToml(content: string): Record<string, Record<string, string>> {
  const sections: Record<string, Record<string, string>> = {};
  let currentSection = "";

  for (const rawLine of content.split("\n")) {
    const line = rawLine.trim();
    // Skip comments and blank lines
    if (line === "" || line.startsWith("#")) continue;

    // Section header
    const sectionMatch = line.match(/^\[([^\]]+)\]$/);
    if (sectionMatch) {
      currentSection = sectionMatch[1].trim();
      if (!sections[currentSection]) {
        sections[currentSection] = {};
      }
      continue;
    }

    // Key = value (string values only)
    const kvMatch = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*"([^"]*)"$/);
    if (kvMatch && currentSection) {
      sections[currentSection][kvMatch[1]] = kvMatch[2];
    }
  }

  return sections;
}

/**
 * Load FraiseQL configuration from a TOML file and apply inject_defaults.
 *
 * Reads the `[inject_defaults]`, `[inject_defaults.queries]`, and
 * `[inject_defaults.mutations]` sections from the given TOML file and
 * registers them with the SchemaRegistry so they are merged into
 * queries/mutations at export time.
 *
 * @param tomlPath - Path to fraiseql.toml file. Defaults to "fraiseql.toml"
 *   in the current working directory.
 *
 * @example
 * ```ts
 * // fraiseql.toml:
 * // [inject_defaults]
 * // tenant_id = "jwt:tenant_id"
 * //
 * // [inject_defaults.queries]
 * // user_id = "jwt:sub"
 *
 * import { loadConfig } from "fraiseql";
 * loadConfig("fraiseql.toml");
 * ```
 */
export function loadConfig(tomlPath?: string): void {
  const resolvedPath = tomlPath
    ? path.resolve(tomlPath)
    : path.resolve("fraiseql.toml");

  const content = fs.readFileSync(resolvedPath, { encoding: "utf-8" });
  const sections = parseMinimalToml(content);

  const base = new Map<string, string>();
  const queries = new Map<string, string>();
  const mutations = new Map<string, string>();

  const baseSection = sections["inject_defaults"];
  if (baseSection) {
    for (const [k, v] of Object.entries(baseSection)) {
      base.set(k, v);
    }
  }

  const queriesSection = sections["inject_defaults.queries"];
  if (queriesSection) {
    for (const [k, v] of Object.entries(queriesSection)) {
      queries.set(k, v);
    }
  }

  const mutationsSection = sections["inject_defaults.mutations"];
  if (mutationsSection) {
    for (const [k, v] of Object.entries(mutationsSection)) {
      mutations.set(k, v);
    }
  }

  SchemaRegistry.setInjectDefaults(base, queries, mutations);
}
