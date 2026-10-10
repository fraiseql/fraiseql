/**
 * Type mapping and introspection for GraphQL schema generation.
 *
 * This module converts TypeScript type annotations to GraphQL type strings,
 * enabling compile-time schema generation without runtime overhead.
 */

import { isScalarType } from "./scalars";

/**
 * Convert TypeScript type to GraphQL type string.
 *
 * @param type - TypeScript type annotation
 * @returns Tuple of [graphql_type, is_nullable]
 *
 * @example
 * typeToGraphQL(String) => ["String", false]
 * typeToGraphQL(String | null) => ["String", true]
 * typeToGraphQL(Array<User>) => ["[User!]", false]
 */
export function typeToGraphQL(type: unknown): [graphqlType: string, nullable: boolean] {
  // Handle null/undefined
  if (type === null || type === undefined) {
    throw new Error("Cannot convert null or undefined type");
  }

  // `Localized<string>` (#1513) is a GraphQL String; `isLocalizedType` carries the flag.
  if (typeof type === "string" && isLocalizedType(type)) {
    return ["String", type.includes(" | null")];
  }

  // Handle union types (T | null) using string representation
  const typeStr = String(type);

  // Basic scalar types
  if (type === String || typeStr === "String") {
    return ["String", false];
  }
  if (type === Number || typeStr === "Number") {
    return ["Float", false];
  }
  if (type === Boolean || typeStr === "Boolean") {
    return ["Boolean", false];
  }

  // A JavaScript `Date` is an instant: the engine's `DateTime` (#1530). Its constructor's
  // name, `Date`, is a calendar date, so the class-name rule below would mislabel it.
  if (type === Date) {
    return ["DateTime", false];
  }

  // For class types, return the class name
  if (typeof type === "function") {
    return [type.name || "Object", false];
  }

  // For string literals representing types (from decorators metadata)
  if (typeof type === "string") {
    // Check if it's a nullable type (T | null syntax in string)
    if (type.includes(" | null")) {
      const baseType = type.replace(" | null", "").trim();
      return [baseType, true];
    }

    // Check if it's a list type (T[])
    if (type.endsWith("[]")) {
      const elementType = type.slice(0, -2);
      return [`[${elementType}!]`, false];
    }

    // Recognize scalar types (ID, DateTime, Email, etc.)
    // These pass through directly to schema.json
    if (isScalarType(type)) {
      return [type, false];
    }

    return [type, false];
  }

  throw new Error(`Unsupported type: ${type}`);
}

/**
 * A localized string (#1513): stored as a map of locale to label, returned to each client as
 * the label of its request locale. Declare a field or argument as `"Localized<string>"` (or
 * `"Localized<string> | null"`) in the type-string API, or pass `localized: true` in its
 * field config. The project's `fraiseql.toml` must declare `[locale]`. Only strings can be
 * localized.
 */
export type Localized<T extends string> = T;

const LOCALIZED_TYPE = /^Localized<\s*(\w+)\s*>(\s*\|\s*null)?$/;

/**
 * Whether a type string declares `Localized<...>`. `Localized` of anything but `string` is
 * refused here, at the author's declaration, rather than by the compiler later.
 */
export function isLocalizedType(type: string): boolean {
  const match = LOCALIZED_TYPE.exec(type.trim());
  if (!match) {
    return false;
  }
  if (match[1] !== "string") {
    throw new Error(
      `Localized<${match[1]}> is not supported; only Localized<string> can be localized ` +
        "(a localized field is a String stored as a locale map)"
    );
  }
  return true;
}

/**
 * Field information extracted from a class with type metadata.
 */
export interface FieldInfo {
  type: string;
  nullable: boolean;
  /** `Localized<string>` (#1513). */
  localized?: boolean;
}

/**
 * Extract field information from class property metadata.
 *
 * @param fields - Dictionary mapping field names to type annotations
 * @returns Dictionary of field_name -> FieldInfo
 *
 * @example
 * const fields = {
 *   id: "number",
 *   name: "string",
 *   email: "string | null"
 * };
 * extractFieldInfo(fields) => {
 *   id: { type: "Int", nullable: false },
 *   name: { type: "String", nullable: false },
 *   email: { type: "String", nullable: true }
 * }
 */
export function extractFieldInfo(fields: Record<string, unknown>): Record<string, FieldInfo> {
  const result: Record<string, FieldInfo> = {};

  for (const [fieldName, fieldType] of Object.entries(fields)) {
    const [graphqlType, nullable] = typeToGraphQL(fieldType);
    result[fieldName] = {
      type: graphqlType,
      nullable,
      ...(typeof fieldType === "string" && isLocalizedType(fieldType) ? { localized: true } : {}),
    };
  }

  return result;
}

/**
 * Argument information for a function parameter.
 */
export interface ArgumentInfo {
  name: string;
  type: string;
  nullable: boolean;
  default?: unknown;
  /** `Localized<string>` (#1513): the server coerces the value to a locale map. */
  localized?: boolean;
}

/**
 * Return type information for a function.
 */
export interface ReturnTypeInfo {
  type: string;
  nullable: boolean;
  isList: boolean;
}

/**
 * Function signature information extracted from a decorated function.
 */
export interface FunctionSignature {
  arguments: ArgumentInfo[];
  returnType: ReturnTypeInfo;
}

/**
 * Extract GraphQL-relevant information from function signature.
 *
 * @param name - Function name
 * @param params - Dictionary mapping parameter names to type annotations
 * @param returnType - Return type annotation
 * @returns FunctionSignature with arguments and return type info
 *
 * @example
 * extractFunctionSignature(
 *   "users",
 *   { limit: "number", offset: "number" },
 *   "User[]"
 * ) => {
 *   arguments: [
 *     { name: "limit", type: "Int", nullable: false },
 *     { name: "offset", type: "Int", nullable: false }
 *   ],
 *   returnType: { type: "[User!]", nullable: false, isList: true }
 * }
 */
export function extractFunctionSignature(
  _name: string,
  params: Record<string, unknown>,
  returnType: unknown
): FunctionSignature {
  // Extract arguments
  const args: ArgumentInfo[] = [];

  for (const [paramName, paramType] of Object.entries(params)) {
    // Skip special parameters
    if (paramName === "self" || paramName === "info") {
      continue;
    }

    const [graphqlType, nullable] = typeToGraphQL(paramType);
    args.push({
      name: paramName,
      type: graphqlType,
      nullable,
      ...(typeof paramType === "string" && isLocalizedType(paramType) ? { localized: true } : {}),
    });
  }

  // Extract return type
  const [returnTypeStr, returnNullable] = typeToGraphQL(returnType);

  // Check if return type is a list
  const isList = returnTypeStr.startsWith("[") && returnTypeStr.endsWith("]");

  return {
    arguments: args,
    returnType: {
      type: returnTypeStr,
      nullable: returnNullable,
      isList,
    },
  };
}
