/**
 * FraiseQL v2 - TypeScript Schema Authoring.
 *
 * This module provides decorators for defining GraphQL schemas that are compiled
 * by the FraiseQL Rust engine. NO runtime FFI - decorators output JSON only.
 *
 * Architecture:
 *   TypeScript @decorators → schema.json → fraiseql-cli → schema.compiled.json → Rust runtime
 *
 * @example
 * ```typescript
 * import * as fraiseql from "fraiseql";
 *
 * @fraiseql.Type()
 * class User {
 *   id: number;
 *   name: string;
 *   email: string;
 * }
 *
 * @fraiseql.Query({ sqlSource: "v_user" })
 * function users(limit: number = 10): User[] {
 *   // Function body not executed - only for type/metadata
 * }
 *
 * // Export to JSON
 * if (require.main === module) {
 *   fraiseql.exportSchema("schema.json");
 * }
 * ```
 *
 * @packageDocumentation
 */

export const version = "2.0.0-alpha.1";

// Export type system
export { typeToGraphQL, extractFieldInfo, extractFunctionSignature } from "./types";
export type { FieldInfo, ArgumentInfo, ReturnTypeInfo, FunctionSignature } from "./types";

// Export registry
export { SchemaRegistry } from "./registry";
export type {
  Field,
  TypeDefinition,
  QueryDefinition,
  MutationDefinition,
  ArgumentDefinition,
  Schema,
  Measure,
  Dimension,
  DimensionPath,
  DenormalizedFilter,
  FactTableDefinition,
  AggregateQueryDefinition,
} from "./registry";

// Export decorators
export {
  Type,
  Query,
  Mutation,
  FactTable as FactTableDecorator,
  AggregateQuery as AggregateQueryDecorator,
  registerTypeFields,
  registerQuery,
  registerMutation,
} from "./decorators";
export type { TypeConfig, OperationConfig, MutationConfig, FactTableConfig, AggregateQueryConfig } from "./decorators";

// Export schema functions
export { config, exportSchema, getSchemaDict, exportSchemaToString } from "./schema";

// Export analytics
export {
  FactTable,
  AggregateQuery,
  registerFactTableManual,
  registerTypeFieldsManual,
} from "./analytics";
export type { FactTableDecoratorConfig, AggregateQueryDecoratorConfig } from "./analytics";

