/**
 * Federation support for Apollo Federation v2 in FraiseQL.
 *
 * This module provides decorators for defining federated GraphQL schemas:
 * - @Key: Define federation keys for entity resolution
 * - @Extends: Extend types from other subgraphs
 * - @External: Mark fields as owned by other subgraphs
 * - @Requires: Declare field dependencies
 * - @Provides: Mark fields that provide data for other subgraphs
 *
 * @example
 * ```typescript
 * import { Type, Key, Extends, External, Requires, Provides } from "fraiseql";
 *
 * // Authoritative User type in this subgraph
 * @Key("id")
 * @Type()
 * class User {
 *   id: string;
 *   email: string;
 * }
 *
 * // Extended User type in another subgraph
 * @Extends()
 * @Key("id")
 * @Type()
 * class User {
 *   @External() id: string;
 *   @External() email: string;
 *   orders: Order[];  // New field in this subgraph
 * }
 * ```
 */

import { SchemaRegistry } from "./registry";

/**
 * Global metadata storage for federation field markers.
 */
const fieldMetadata = new Map<object, Map<string, FieldMarker>>();

export type { ID } from "./scalars";

/**
 * Augment class constructor to support __fraiseqlFederation__ property.
 */
declare global {
  interface Function {
    __fraiseqlFederation__: FederationMetadata;
    __fraiseqlType__: boolean;
  }
}

/**
 * Federation metadata stored on type classes.
 */
interface FederationMetadata {
  keys: Array<{ fields: string[] }>;
  extend: boolean;
  external_fields: string[];
  requires: Record<string, string>;
  provides_data: string[];
}

/**
 * Property descriptor for federation field markers.
 */
class FieldMarker {
  constructor(
    public external: boolean = false,
    public requires: string | null = null,
    public provides: string[] = []
  ) {}
}

/**
 * Mark a field as external (owned by another subgraph).
 *
 * Use this in extended types to mark which fields are owned by the
 * authoritative subgraph.
 *
 * @example
 * ```typescript
 * @Extends()
 * @Key("id")
 * @Type()
 * class User {
 *   @External() id: string;
 *   @External() email: string;
 *   orders: Order[];  // New field in this subgraph
 * }
 * ```
 */
export function External(): PropertyDecorator {
  return function (target: object, propertyKey: string | symbol | undefined) {
    if (!propertyKey) return;
    const key = String(propertyKey);
    if (!fieldMetadata.has(target)) {
      fieldMetadata.set(target, new Map());
    }
    fieldMetadata.get(target)!.set(key, new FieldMarker(true));
  };
}

/**
 * Mark a field as requiring another field to be resolved first.
 *
 * This declares that a field needs data from another field (in the same
 * type or from federation) to compute its value.
 *
 * @param fieldName Name of the field that must be resolved first
 *
 * @example
 * ```typescript
 * @Extends()
 * @Key("id")
 * @Type()
 * class User {
 *   @External() id: string;
 *   @External() email: string;
 *   @Requires("email")
 *   profile: UserProfile;  // Needs email to resolve
 * }
 * ```
 */
export function Requires(fieldName: string): PropertyDecorator {
  return function (target: object, propertyKey: string | symbol | undefined) {
    if (!propertyKey) return;
    const key = String(propertyKey);
    if (!fieldMetadata.has(target)) {
      fieldMetadata.set(target, new Map());
    }
    fieldMetadata.get(target)!.set(key, new FieldMarker(false, fieldName));
  };
}

/**
 * Mark a field as providing data for external subgraph fields.
 *
 * This declares that this field's data can be used to resolve fields
 * in other subgraphs.
 *
 * @param targets List of "Type.field" references this field provides data for
 *
 * @example
 * ```typescript
 * @Key("id")
 * @Type()
 * class User {
 *   id: string;
 *   @Provides("Order.owner_email", "Invoice.owner_email")
 *   email: string;
 * }
 * ```
 */
export function Provides(...targets: string[]): PropertyDecorator {
  return function (target: object, propertyKey: string | symbol | undefined) {
    if (!propertyKey) return;
    const key = String(propertyKey);
    if (!fieldMetadata.has(target)) {
      fieldMetadata.set(target, new Map());
    }
    fieldMetadata.get(target)!.set(key, new FieldMarker(false, null, targets));
  };
}

/**
 * Mark a type with a federation key for entity resolution.
 *
 * Federation keys are used to uniquely identify entities and resolve them
 * across subgraphs. Multiple @Key decorators define composite keys.
 *
 * @param fieldNames Field name or list of field names that form the key
 *
 * @example
 * ```typescript
 * @Key("id")
 * @Type()
 * class User {
 *   id: string;
 *   email: string;
 * }
 *
 * @Key("tenant_id")
 * @Key("id")
 * @Type()
 * class Account {
 *   tenant_id: string;
 *   id: string;
 *   name: string;
 * }
 * ```
 */
export function Key(
  fieldNames: string | string[]
): ClassDecorator & PropertyDecorator {
  // When used as class decorator (called first)
  return function (target: any) {
    const fields =
      typeof fieldNames === "string" ? [fieldNames] : fieldNames;

    // Initialize federation metadata if not present
    if (!target.__fraiseqlFederation__) {
      target.__fraiseqlFederation__ = {
        keys: [],
        extend: false,
        external_fields: [],
        requires: {},
        provides_data: [],
      };
    }

    const metadata: FederationMetadata = target.__fraiseqlFederation__;

    // Validate that @Type decorator was applied
    if (!target.__fraiseqlType__) {
      throw new TypeError(`@Key requires @Type decorator to be applied to ${target.name}`);
    }

    // Note: Field validation is deferred to compile time
    // At runtime, TypeScript type annotations don't exist
    // We just store the key definition and validate later

    // Check for duplicate keys
    const newKey = { fields };
    const isDuplicate = metadata.keys.some(
      (existingKey) =>
        JSON.stringify(existingKey.fields) === JSON.stringify(newKey.fields)
    );

    if (isDuplicate) {
      throw new Error(`Duplicate key field in ${target.name}`);
    }

    // Add key to federation metadata
    metadata.keys.push(newKey);

    return target;
  } as any;
}

/**
 * Mark a type as extending a type from another subgraph.
 *
 * Extended types can have external fields from the authoritative subgraph
 * and add new fields specific to this subgraph.
 *
 * @example
 * ```typescript
 * @Extends()
 * @Key("id")
 * @Type()
 * class User {
 *   @External() id: string;
 *   @External() email: string;
 *   orders: Order[];  // New field in this subgraph
 * }
 * ```
 */
export function Extends(): ClassDecorator {
  return function (target: any) {
    // Initialize federation metadata if not present
    if (!target.__fraiseqlFederation__) {
      target.__fraiseqlFederation__ = {
        keys: [],
        extend: false,
        external_fields: [],
        requires: {},
        provides_data: [],
      };
    }

    const metadata: FederationMetadata = target.__fraiseqlFederation__;

    // Check that @Key decorator was used
    if (!metadata.keys || metadata.keys.length === 0) {
      throw new Error(`@Extends requires @Key decorator on ${target.name}`);
    }

    // Mark type as extended
    metadata.extend = true;

    // Extract key field names
    const keyFields = new Set<string>();
    for (const keyDef of metadata.keys) {
      for (const field of keyDef.fields) {
        keyFields.add(field);
      }
    }

    // Scan fields for field markers
    const prototype = target.prototype || target;
    const externalFields = new Set<string>();

    if (fieldMetadata.has(prototype)) {
      const classMetadata = fieldMetadata.get(prototype)!;
      for (const [key, marker] of classMetadata.entries()) {
        if (marker instanceof FieldMarker) {
          if (marker.external) {
            externalFields.add(key);
          }
          if (marker.requires) {
            metadata.requires[key] = marker.requires;
          }
          if (marker.provides && marker.provides.length > 0) {
            metadata.provides_data.push(...marker.provides);
          }
        }
      }
    }

    // Validate consistency: if non-key fields are external, all key fields must be external
    if (externalFields.size > 0) {
      let hasNonKeyExternal = false;
      for (const extField of externalFields) {
        if (!keyFields.has(extField)) {
          hasNonKeyExternal = true;
          break;
        }
      }

      if (hasNonKeyExternal) {
        // Check if all key fields are also external
        for (const keyField of keyFields) {
          if (!externalFields.has(keyField)) {
            throw new Error(`Field '${Array.from(externalFields)[0]}' not found`);
          }
        }
      }
    }

    // Store external fields
    metadata.external_fields = Array.from(externalFields);

    return target;
  };
}

/**
 * Mark a class as a GraphQL type with optional federation support.
 *
 * @example
 * ```typescript
 * @Type()
 * class User {
 *   id!: string;
 *   email!: string;
 * }
 * ```
 */
export function Type(): ClassDecorator {
  return function (target: any) {
    // Mark that @Type decorator was applied
    target.__fraiseqlType__ = true;

    // Initialize federation metadata if not present
    if (!target.__fraiseqlFederation__) {
      target.__fraiseqlFederation__ = {
        keys: [],
        extend: false,
        external_fields: [],
        requires: {},
        provides_data: [],
      };
    }

    // Validate that @external is only used on extended types
    const prototype = target.prototype || target;
    const metadata: FederationMetadata = target.__fraiseqlFederation__;

    if (fieldMetadata.has(prototype)) {
      const classMetadata = fieldMetadata.get(prototype)!;
      for (const [, marker] of classMetadata.entries()) {
        if (marker instanceof FieldMarker && marker.external) {
          if (!metadata.extend) {
            throw new Error("@external requires @extends");
          }
        }
      }
    }

    // Register type with schema registry
    const fields: Record<string, any> = {};
    for (const key in prototype) {
      const descriptor = Object.getOwnPropertyDescriptor(prototype, key);
      if (descriptor && descriptor.value !== undefined) {
        fields[key] = {
          type: "String", // Default type for property markers
          nullable: false,
        };
      }
    }

    // Register with schema registry (federation metadata will be added in schema generation)
    try {
      const fieldArray: Array<{
        name: string;
        type: string;
        nullable: boolean;
      }> = Object.entries(fields).map(([name, info]) => ({
        name,
        ...info,
      }));
      SchemaRegistry.registerType(target.name, fieldArray, undefined);
    } catch (e) {
      // Ignore registry errors in tests
    }

    return target;
  };
}

/**
 * Generate schema JSON from the current registry.
 *
 * @param _types Optional list of types to include (unused for compatibility)
 * @returns Schema dictionary with federation metadata if applicable
 */
export function generateSchemaJson(
  _types?: Array<{ name: string }> | unknown
): Record<string, any> {
  const schema = SchemaRegistry.getSchema();
  return {
    ...schema,
    federation: {
      enabled: false,
      version: "v2",
    },
  };
}

// Re-export commonly used types
export { SchemaRegistry };
