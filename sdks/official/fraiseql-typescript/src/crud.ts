/**
 * CRUD operation generation for FraiseQL types.
 *
 * When `crud` is enabled on a type, this module auto-generates standard
 * GraphQL queries (get-by-ID, list) and mutations (create, update, delete)
 * following FraiseQL naming conventions.
 */

import { SchemaRegistry, ArgumentDefinition, Field } from "./registry";

const ALL_OPS = new Set(["read", "create", "update", "delete"]);

function pascalToSnake(name: string): string {
  return name.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase();
}

function pluralize(name: string): string {
  if (name.endsWith("s") && !name.endsWith("ss")) return name;
  for (const suffix of ["ss", "sh", "ch", "x", "z"]) {
    if (name.endsWith(suffix)) return name + "es";
  }
  if (/[^aeiou]y$/.test(name)) return name.slice(0, -1) + "ies";
  return name + "s";
}

function parseCrudOps(crud: boolean | string[]): Set<string> {
  if (crud === true) return new Set(ALL_OPS);
  if (Array.isArray(crud)) {
    const unknown = crud.filter((op) => !ALL_OPS.has(op));
    if (unknown.length > 0) {
      throw new Error(
        `Unknown CRUD operations: ${unknown.join(", ")}. Valid: read, create, update, delete`
      );
    }
    return new Set(crud);
  }
  return new Set();
}

/**
 * Generate CRUD queries and mutations for a type.
 *
 * @param typeName - The GraphQL type name (PascalCase)
 * @param fields - The type's field definitions
 * @param crud - `true` for all operations, or an array of specific operations
 * @param sqlSource - Optional SQL source override (defaults to `v_{snake_name}`)
 * @param cascade - Whether generated mutations include `cascade: true`
 *
 * @throws If `crud` contains unknown operation names
 * @throws If the type has no fields
 */
export function generateCrudOperations(
  typeName: string,
  fields: Field[],
  crud: boolean | string[],
  sqlSource?: string,
  cascade?: boolean
): void {
  const ops = parseCrudOps(crud);
  if (ops.size === 0) return;
  if (fields.length === 0) {
    throw new Error(`Type '${typeName}' has no fields; cannot generate CRUD operations`);
  }

  const snake = pascalToSnake(typeName);
  const view = sqlSource ?? `v_${snake}`;
  // Safe: we checked fields.length > 0 above
  const pkField = fields[0]!;

  if (ops.has("read")) {
    // Get-by-ID query
    SchemaRegistry.registerQuery(
      snake,
      typeName,
      false,
      true,
      [{ name: pkField.name, type: pkField.type, nullable: false }],
      `Get ${typeName} by ID.`,
      { sql_source: view }
    );

    // List query with auto_params
    SchemaRegistry.registerQuery(
      pluralize(snake),
      typeName,
      true,
      false,
      [],
      `List ${typeName} records.`,
      { sql_source: view, auto_params: { where: true, order_by: true, limit: true, offset: true } }
    );
  }

  if (ops.has("create")) {
    const args: ArgumentDefinition[] = fields.map((f) => ({
      name: f.name,
      type: f.type,
      nullable: f.nullable,
    }));
    const config: Record<string, unknown> = {
      sql_source: `fn_create_${snake}`,
      operation: "INSERT",
    };
    if (cascade) config.cascade = true;
    SchemaRegistry.registerMutation(
      `create_${snake}`,
      typeName,
      false,
      false,
      args,
      `Create a new ${typeName}.`,
      config
    );
  }

  if (ops.has("update")) {
    const args: ArgumentDefinition[] = [
      { name: pkField.name, type: pkField.type, nullable: false },
      ...fields.slice(1).map((f) => ({ name: f.name, type: f.type, nullable: true })),
    ];
    const config: Record<string, unknown> = {
      sql_source: `fn_update_${snake}`,
      operation: "UPDATE",
    };
    if (cascade) config.cascade = true;
    SchemaRegistry.registerMutation(
      `update_${snake}`,
      typeName,
      false,
      true,
      args,
      `Update an existing ${typeName}.`,
      config
    );
  }

  if (ops.has("delete")) {
    const config: Record<string, unknown> = {
      sql_source: `fn_delete_${snake}`,
      operation: "DELETE",
    };
    if (cascade) config.cascade = true;
    SchemaRegistry.registerMutation(
      `delete_${snake}`,
      typeName,
      false,
      false,
      [{ name: pkField.name, type: pkField.type, nullable: false }],
      `Delete a ${typeName}.`,
      config
    );
  }
}
