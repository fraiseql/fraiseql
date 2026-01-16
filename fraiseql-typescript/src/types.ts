/**
 * Type mapping and introspection for GraphQL schema generation.
 *
 * This module converts TypeScript type annotations to GraphQL type strings,
 * enabling compile-time schema generation without runtime overhead.
 */

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

    return [type, false];
  }

  throw new Error(`Unsupported type: ${type}`);
}

/**
 * Field information extracted from a class with type metadata.
 */
export interface FieldInfo {
  type: string;
  nullable: boolean;
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
