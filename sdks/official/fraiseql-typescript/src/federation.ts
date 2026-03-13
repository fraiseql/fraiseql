/**
 * Apollo Federation v2 decorators for FraiseQL.
 *
 * Provides @Key, @Extends, @External, @Requires, @Provides decorators for
 * marking types and fields with federation metadata. NO runtime behavior —
 * only used for schema compilation.
 *
 * @example
 * ```typescript
 * import { Type, Key, Extends, External, Requires } from "fraiseql/federation";
 *
 * @Extends()
 * @Key('id')
 * @Type()
 * class User {
 *   @External() id: string;
 *   @Requires('id') profile: string;
 * }
 * ```
 */

// Re-export shared infrastructure
export { Type } from "./decorators";
export { SchemaRegistry } from "./registry";
export type { ID } from "./scalars";

/**
 * Internal federation metadata stored on the class constructor.
 */
interface FederationMetadata {
  keys: Array<{ fields: string[] }>;
  extend: boolean;
  external_fields: string[];
  requires: Record<string, string>;
  provides: Record<string, string[]>;
  provides_data: string[];
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function getOrInitMeta(cls: any): FederationMetadata {
  if (!cls.__fraiseqlFederation__) {
    cls.__fraiseqlFederation__ = {
      keys: [],
      extend: false,
      external_fields: [],
      requires: {},
      provides: {},
      provides_data: [],
    };
  }
  return cls.__fraiseqlFederation__ as FederationMetadata;
}

/**
 * Get all declared field names from a class constructor.
 *
 * Uses ES2022 native class fields: `Object.keys(new cls())` discovers all
 * instance properties, including those with no initializer (value = undefined).
 * Falls back to federation metadata for any decorated fields that may not
 * appear in a plain instantiation.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function getClassFields(cls: any): Set<string> {
  const fields = new Set<string>();

  // ES2022 class field instantiation
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const instance = new (cls as new () => any)();
    for (const key of Object.keys(instance)) {
      fields.add(key);
    }
  } catch {
    // Instantiation may fail for classes with required constructor arguments
  }

  // Also collect from federation metadata (decorated properties)
  const meta: FederationMetadata | undefined = cls.__fraiseqlFederation__;
  if (meta) {
    for (const f of meta.external_fields) {
      fields.add(f);
    }
    for (const f of Object.keys(meta.requires)) {
      fields.add(f);
    }
    for (const f of Object.keys(meta.provides)) {
      fields.add(f);
    }
  }

  return fields;
}

/**
 * Mark a type as a federation entity with the given key field.
 *
 * Multiple @Key decorators can be stacked for composite keys.
 * Duplicate key fields are rejected immediately.
 *
 * @param field - The name of the key field
 * @throws If the same field is declared as a key more than once
 */
export function Key(field: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (cls: any): any {
    const meta = getOrInitMeta(cls);
    if (meta.keys.some((k) => k.fields.includes(field))) {
      throw new Error(`Duplicate key field '${field}' on type ${cls.name as string}`);
    }
    meta.keys.push({ fields: [field] });
    return cls;
  };
}

/**
 * Mark a type as extending a federated type from another subgraph.
 *
 * Must be combined with @Key. The extended type's key fields should be
 * marked @External.
 *
 * @throws If @Key decorator has not been applied to the class
 */
export function Extends() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (cls: any): any {
    const meta = getOrInitMeta(cls);
    if (meta.keys.length === 0) {
      throw new Error(`@Extends requires @Key decorator on type ${cls.name as string}`);
    }
    meta.extend = true;
    return cls;
  };
}

/**
 * Mark a class property as external — owned by another subgraph.
 *
 * Must be used on a class decorated with @Extends.
 *
 * @throws If applied to a method (not a property)
 */
export function External() {
  return function (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    target: any,
    key: string,
    descriptor?: PropertyDescriptor
  ): void {
    if (descriptor !== undefined && typeof descriptor.value === "function") {
      throw new Error(
        `@External can only be applied to class properties, not methods (got '${key}')`
      );
    }
    const meta = getOrInitMeta(target.constructor);
    if (!meta.external_fields.includes(key)) {
      meta.external_fields.push(key);
    }
  };
}

/**
 * Declare that a field requires another field to be fetched for resolution.
 *
 * @param field - Name of the required field
 */
export function Requires(field: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (target: any, key: string): void {
    const meta = getOrInitMeta(target.constructor);
    meta.requires[key] = field;
  };
}

/**
 * Declare that a field provides data to other subgraphs.
 *
 * @param targets - One or more "TypeName.fieldName" strings
 */
export function Provides(...targets: string[]) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return function (target: any, key: string): void {
    const meta = getOrInitMeta(target.constructor);
    meta.provides[key] = (meta.provides[key] ?? []).concat(targets);
    meta.provides_data.push(...targets);
  };
}

/**
 * Generate the schema JSON for a set of federated types.
 *
 * Uses SchemaRegistry.getSchema() as the single source of truth for all base
 * type and field information, then overlays federation metadata from the
 * federation decorators. This ensures federation metadata flows through the
 * same pipeline as every other schema declaration and ends up in schema.json.
 *
 * @param types - Array of class constructors decorated with @Type and federation decorators
 * @returns Schema object compatible with schema.json, augmented with federation metadata
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function generateSchemaJson(types: any[]): any {
  const { SchemaRegistry: Registry } = require("./registry") as {
    SchemaRegistry: typeof import("./registry").SchemaRegistry;
  };
  const base = Registry.getSchema();

  // Build a lookup: class name → federation metadata
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const fedByName = new Map<string, FederationMetadata>();
  for (const cls of types) {
    const meta: FederationMetadata = cls.__fraiseqlFederation__ ?? {
      keys: [],
      extend: false,
      external_fields: [],
      requires: {},
      provides: {},
      provides_data: [],
    };
    fedByName.set(cls.name as string, meta);
  }

  // Augment each type from the registry with federation metadata.
  // Fields come from SchemaRegistry (which has explicit type/nullable info from
  // registerTypeFields). For any field names only discoverable via class
  // instantiation (no explicit registration), we append them with federation info.
  const augmentedTypes = base.types.map((typeDef) => {
    const meta = fedByName.get(typeDef.name);
    if (!meta) return typeDef;

    // All field names known to this type (registered + class-instantiated)
    const cls = types.find((c) => (c.name as string) === typeDef.name);
    const allFieldNames = getClassFields(cls);

    // Start from registered fields (have full type info)
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const registeredNames = new Set(typeDef.fields.map((f) => f.name));
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const mergedFields: any[] = typeDef.fields.map((f) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const fieldFed: Record<string, any> = {};
      if (meta.external_fields.includes(f.name)) fieldFed.external = true;
      if (meta.requires[f.name] !== undefined) fieldFed.requires = meta.requires[f.name];
      if (meta.provides[f.name] !== undefined) fieldFed.provides = meta.provides[f.name];
      if (Object.keys(fieldFed).length === 0) return f;
      return { ...f, federation: fieldFed };
    });

    // Append fields found via class instantiation that weren't explicitly registered
    for (const fieldName of allFieldNames) {
      if (registeredNames.has(fieldName)) continue;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const fieldFed: Record<string, any> = {};
      if (meta.external_fields.includes(fieldName)) fieldFed.external = true;
      if (meta.requires[fieldName] !== undefined) fieldFed.requires = meta.requires[fieldName];
      if (meta.provides[fieldName] !== undefined) fieldFed.provides = meta.provides[fieldName];
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const entry: Record<string, any> = { name: fieldName };
      if (Object.keys(fieldFed).length > 0) entry.federation = fieldFed;
      mergedFields.push(entry);
    }

    return {
      ...typeDef,
      fields: mergedFields,
      federation: {
        keys: meta.keys,
        extend: meta.extend,
        external_fields: meta.external_fields,
      },
    };
  });

  return {
    ...base,
    federation: { enabled: true, version: "v2" },
    types: augmentedTypes,
  };
}

/**
 * Validate federation constraints across a set of types.
 *
 * Checks:
 * - @Key fields must exist on the class
 * - @Key requires @Type decorator
 * - @External requires @Extends
 * - @Requires target fields must exist on the class
 *
 * @param types - Array of class constructors to validate
 * @throws If any federation constraint is violated
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function validateFederation(types: any[]): void {
  // Import here to avoid circular dependency
  const { SchemaRegistry: Registry } = require("./registry") as {
    SchemaRegistry: typeof import("./registry").SchemaRegistry;
  };

  for (const cls of types) {
    const meta: FederationMetadata | undefined = cls.__fraiseqlFederation__;
    const allFields = getClassFields(cls);

    const keys: Array<{ fields: string[] }> = meta?.keys ?? [];

    if (keys.length > 0) {
      // @Key requires @Type
      const schema = Registry.getSchema();
      if (!schema.types.some((t) => t.name === (cls.name as string))) {
        throw new Error(`@Key requires @Type decorator on class ${cls.name as string}`);
      }

      // Key fields must exist on the class
      for (const key of keys) {
        for (const field of key.fields) {
          if (!allFields.has(field)) {
            throw new Error(
              `Field '${field}' not found on type ${cls.name as string}`
            );
          }
        }
      }
    }

    // @External requires @Extends
    if ((meta?.external_fields?.length ?? 0) > 0 && !(meta?.extend)) {
      throw new Error(`@external requires @extends on type ${cls.name as string}`);
    }

    // @Requires target fields must exist
    for (const requiredField of Object.values(meta?.requires ?? {})) {
      if (!allFields.has(requiredField)) {
        throw new Error(
          `Field '${requiredField}' not found on type ${cls.name as string}`
        );
      }
    }
  }
}
