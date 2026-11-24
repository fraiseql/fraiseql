/**
 * Apollo Federation v2 decorators for FraiseQL.
 *
 * Provides @Key, @Extends, @External, @Shareable and @Inaccessible for marking types and
 * fields with federation metadata. NO runtime behavior — only used for schema compilation.
 * `generateSchemaJson` writes them in the shapes the compiler reads: entities in the
 * top-level `federation.entities`, a type-level @Shareable in `federation.shareable_types`,
 * @Inaccessible as a field `directives` entry.
 *
 * @Requires, @Provides and @Override are refused where they are declared: the compiled
 * schema has no carrier for them, so they would not reach the subgraph's SDL.
 *
 * @example
 * ```typescript
 * import { Type, Key, Extends, External } from "fraiseql";
 *
 * @Extends()
 * @Key("id")
 * @Type()
 * class User {
 *   @External() id: string;
 *   email: string;
 * }
 * ```
 */

// Re-export shared infrastructure
export { Type } from "./decorators";
export { SchemaRegistry } from "./registry";
export type { ID } from "./scalars";

// Local import for use within this module
import { SchemaRegistry } from "./registry";
import { decoratedMemberName } from "./decorator-name";

/**
 * Internal federation metadata stored on the class constructor.
 */
interface FederationMetadata {
  /** The entity's key: one GraphQL field set, such as `"id"` or `"tenant_id id"`. */
  key?: string;
  extend: boolean;
  /** A type-level @Shareable(): a keyless value type shared across subgraphs. */
  shareable_type: boolean;
  external_fields: string[];
  shareable_fields: string[];
  inaccessible_fields: string[];
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- dynamic class property access required for legacy decorator metadata storage
function getOrInitMeta(cls: any): FederationMetadata {
  // An own property: a subclass inherits its parent's statics, and reading that record
  // would put the subclass's key and marks on the parent.
  if (!Object.prototype.hasOwnProperty.call(cls, "__fraiseqlFederation__")) {
    cls.__fraiseqlFederation__ = {
      extend: false,
      shareable_type: false,
      external_fields: [],
      shareable_fields: [],
      inaccessible_fields: [],
    };
  }
  return cls.__fraiseqlFederation__ as FederationMetadata;
}

/** The field marks a TC39 field decorator records, in its class's decorator metadata. */
type FieldMarks = Pick<
  FederationMetadata,
  "external_fields" | "shareable_fields" | "inaccessible_fields"
>;

/** The key of the field marks in a class's TC39 decorator metadata. */
const FIELD_MARKS = "__fraiseqlFederationFields";

/**
 * `Symbol.metadata`, or the `Symbol.for("Symbol.metadata")` stand-in esbuild (and so
 * `tsx`) stores a class's decorator metadata under where the runtime has none.
 */
const METADATA_SYMBOL: symbol =
  (Symbol as { metadata?: symbol }).metadata ?? Symbol.for("Symbol.metadata");

/**
 * Where a field decorator records its mark, and the field's name, under either decorator
 * protocol (see `decoratedMemberName`).
 *
 * A legacy field decorator is handed the class prototype, so the mark goes on the class.
 * A TC39 one is handed no class at all, only its context: the mark goes in the context's
 * `metadata`, the object the runtime stores on the class, where `federationOf` reads it.
 */
function fieldMarks(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy property decorator target
  target: any,
  keyOrContext: unknown,
  decorator: string
): { marks: FieldMarks; name: string } {
  const name = decoratedMemberName(keyOrContext, decorator);
  if (typeof keyOrContext === "string") {
    return { marks: getOrInitMeta(target.constructor), name };
  }
  const context = keyOrContext as { kind?: string; metadata?: Record<string, unknown> };
  if (context.kind !== "field") {
    throw new Error(
      `@${decorator} can only be applied to class fields (got a ${context.kind} '${name}')`
    );
  }
  if (!context.metadata) {
    throw new Error(
      `@${decorator} on '${name}': the decorator context carries no \`metadata\` object, ` +
        `so the field's class cannot be found. Use a transpiler that implements decorator ` +
        `metadata, or legacy decorators (\`experimentalDecorators\`).`
    );
  }
  // The metadata object inherits from the parent class's: a subclass's marks go in an
  // entry of its own, never in the one it would read through the prototype.
  if (!Object.prototype.hasOwnProperty.call(context.metadata, FIELD_MARKS)) {
    context.metadata[FIELD_MARKS] = {
      external_fields: [],
      shareable_fields: [],
      inaccessible_fields: [],
    } satisfies FieldMarks;
  }
  return { marks: context.metadata[FIELD_MARKS] as FieldMarks, name };
}

/** `cls`'s federation metadata, recorded under either decorator protocol. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- dynamic class property access
function federationOf(cls: any): FederationMetadata | undefined {
  const recorded =
    cls && Object.prototype.hasOwnProperty.call(cls, "__fraiseqlFederation__")
      ? (cls.__fraiseqlFederation__ as FederationMetadata)
      : undefined;
  const metadata = cls?.[METADATA_SYMBOL] as Record<string, unknown> | undefined;
  const marks =
    metadata && Object.prototype.hasOwnProperty.call(metadata, FIELD_MARKS)
      ? (metadata[FIELD_MARKS] as FieldMarks)
      : undefined;
  if (!marks) return recorded;
  const base = recorded ?? getOrInitMeta(cls);
  const merged = (a: string[], b: string[]) => [...a, ...b.filter((f) => !a.includes(f))];
  return {
    ...base,
    external_fields: merged(base.external_fields, marks.external_fields),
    shareable_fields: merged(base.shareable_fields, marks.shareable_fields),
    inaccessible_fields: merged(base.inaccessible_fields, marks.inaccessible_fields),
  };
}

/**
 * Get all declared field names from a class constructor.
 *
 * Uses ES2022 native class fields: `Object.keys(new cls())` discovers all
 * instance properties, including those with no initializer (value = undefined).
 * Falls back to federation metadata for any decorated fields that may not
 * appear in a plain instantiation.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- dynamic class instantiation for field discovery
function getClassFields(cls: any): Set<string> {
  const fields = new Set<string>();

  // ES2022 class field instantiation
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- instantiating unknown class constructor
    const instance = new (cls as new () => any)();
    for (const key of Object.keys(instance)) {
      fields.add(key);
    }
  } catch {
    // Instantiation may fail for classes with required constructor arguments
  }

  // Also collect from federation metadata (decorated properties)
  const meta = federationOf(cls);
  if (meta) {
    for (const f of meta.external_fields) fields.add(f);
    for (const f of meta.shareable_fields) fields.add(f);
    for (const f of meta.inaccessible_fields) fields.add(f);
  }

  return fields;
}

/** The names a field set selects at its top level: `"id org { id }"` → `id`, `org`. */
function topLevelFields(fieldSet: string): string[] {
  const names: string[] = [];
  let depth = 0;
  for (const token of fieldSet.replace(/[{}]/g, (b) => ` ${b} `).split(/[\s,]+/)) {
    if (token === "{") depth += 1;
    else if (token === "}") depth -= 1;
    else if (token !== "" && depth === 0) names.push(token);
  }
  return names;
}

/**
 * Mark a type as a federation entity, keyed on one GraphQL field set.
 *
 * A composite key is one field set: `@Key("tenant_id id")`. A type takes one @Key: the
 * compiled schema carries one key per entity, so a second @Key is refused rather than
 * merged into a key the author did not write.
 *
 * @param fieldSet - The key's field set, such as `"id"` or `"tenant_id id"`
 * @throws If the type already carries a @Key
 */
export function Key(fieldSet: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy class decorator signature
  return function (cls: any): any {
    const meta = getOrInitMeta(cls);
    if (meta.key !== undefined) {
      throw new Error(
        `@Key on type ${cls.name}: the type already has @Key("${meta.key}"), and the ` +
          `compiled schema carries one key per entity. For a composite key, pass one ` +
          `field set: @Key("${meta.key} ${fieldSet}").`
      );
    }
    meta.key = fieldSet;
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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy class decorator signature
  return function (cls: any): any {
    const meta = getOrInitMeta(cls);
    if (meta.key === undefined) {
      throw new Error(`@Extends requires @Key decorator on type ${cls.name}`);
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
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy property decorator target
    target: any,
    keyOrContext: unknown,
    descriptor?: PropertyDescriptor
  ): void {
    if (descriptor !== undefined && typeof descriptor.value === "function") {
      throw new Error(
        `@External can only be applied to class properties, not methods (got '${String(keyOrContext)}')`
      );
    }
    const { marks, name } = fieldMarks(target, keyOrContext, "External");
    if (!marks.external_fields.includes(name)) {
      marks.external_fields.push(name);
    }
  };
}

/** The refusal of a federation marker the compiled schema cannot carry. */
function noCarrier(marker: string): Error {
  return new Error(
    `${marker}: this federation directive has no carrier in the compiled schema, so ` +
      `it would not reach the subgraph's SDL. Remove it; the supported markers are ` +
      `@Key, @Extends, @External, @Shareable and @Inaccessible.`
  );
}

/**
 * Refused: federation `@requires` has no carrier in the compiled schema.
 *
 * @throws Always
 */
export function Requires(_field: string): never {
  throw noCarrier("@Requires");
}

/**
 * Refused: federation `@provides` has no carrier in the compiled schema.
 *
 * @throws Always
 */
export function Provides(..._targets: string[]): never {
  throw noCarrier("@Provides");
}

/**
 * Mark a field, or a whole keyless value type, as shareable across subgraphs.
 *
 * On a field of an entity, the field can be resolved by more than one subgraph. On a
 * class, the type is a keyless value type that several subgraphs define identically; it
 * cannot also carry a @Key.
 */
export function Shareable() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy class or property decorator target
  return function (target: any, keyOrContext?: unknown): any {
    const onClass =
      keyOrContext === undefined || (keyOrContext as { kind?: string }).kind === "class";
    if (onClass) {
      getOrInitMeta(target).shareable_type = true;
      return target;
    }
    const { marks, name } = fieldMarks(target, keyOrContext, "Shareable");
    if (!marks.shareable_fields.includes(name)) {
      marks.shareable_fields.push(name);
    }
    return undefined;
  };
}

/**
 * Mark a field as inaccessible — hidden from the public API.
 *
 * The field is still resolved internally for entity resolution but does
 * not appear in the composed supergraph schema exposed to clients.
 */
export function Inaccessible() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- legacy property decorator target
  return function (target: any, keyOrContext: unknown): void {
    const { marks, name } = fieldMarks(target, keyOrContext, "Inaccessible");
    if (!marks.inaccessible_fields.includes(name)) {
      marks.inaccessible_fields.push(name);
    }
  };
}

/**
 * Refused: federation `@override` has no carrier in the compiled schema.
 *
 * @throws Always
 */
export function Override(_from: string): never {
  throw noCarrier("@Override");
}

/** Opaque type for class constructors passed to federation functions. */
export type FederatedClass = { name: string; __fraiseqlFederation__?: FederationMetadata };

/** Options of {@link generateSchemaJson}. */
export interface FederationOptions {
  /** The subgraph's service name, written as `federation.service_name`. */
  serviceName?: string;
}

/** One entity of the compiled `federation.entities`. */
interface FederationEntity {
  name: string;
  key_fields: string[];
  extends?: true;
  external_fields?: string[];
  shareable_fields?: string[];
}

/**
 * Generate the schema JSON for a set of federated types.
 *
 * Uses SchemaRegistry.getSchema() as the single source of truth for all base type and
 * field information, and adds the federation decorators in the shapes the compiler
 * reads: `federation.entities` for each @Key type, `federation.shareable_types` for each
 * type-level @Shareable(), and an `inaccessible` field directive for each @Inaccessible().
 *
 * @param types - Array of class constructors decorated with @Type and federation decorators
 * @param options - The subgraph's service name
 * @returns Schema object compatible with schema.json, with its `federation` block
 * @throws If a type is both @Shareable() and @Key, or a field marker sits on a type that
 *   cannot carry it
 */
export function generateSchemaJson(
  types: FederatedClass[],
  options: FederationOptions = {}
): Record<string, unknown> {
  const base = SchemaRegistry.getSchema();

  type MergedField = import("./registry").Field | { name: string; directives?: unknown[] };

  const fedByName = new Map<string, FederationMetadata>();
  for (const cls of types) {
    const meta = federationOf(cls);
    if (meta) fedByName.set(cls.name, meta);
  }

  const entities: FederationEntity[] = [];
  const shareableTypes: string[] = [];
  for (const cls of types) {
    const meta = fedByName.get(cls.name);
    if (!meta) continue;
    if (meta.shareable_type) {
      if (meta.key !== undefined) {
        throw new Error(
          `@Shareable() on type ${cls.name} marks a keyless value type and cannot be ` +
            `combined with @Key. For a shareable field on an entity, put @Shareable() on ` +
            `the field.`
        );
      }
      shareableTypes.push(cls.name);
      continue;
    }
    if (meta.key === undefined) {
      const entityOnly = [...meta.external_fields, ...meta.shareable_fields];
      if (entityOnly.length > 0) {
        throw new Error(
          `@External/@Shareable on ${cls.name}.${entityOnly[0]}: the compiled schema ` +
            `carries these markers on an entity's fields, and ${cls.name} has no @Key.`
        );
      }
      continue;
    }
    const entity: FederationEntity = { name: cls.name, key_fields: [meta.key] };
    if (meta.extend) entity.extends = true;
    if (meta.external_fields.length > 0) entity.external_fields = [...meta.external_fields];
    if (meta.shareable_fields.length > 0) entity.shareable_fields = [...meta.shareable_fields];
    entities.push(entity);
  }

  /** `field`, with an `inaccessible` directive when the decorators hide it. */
  function withDirectives(field: MergedField, meta: FederationMetadata): MergedField {
    if (!meta.inaccessible_fields.includes(field.name)) return field;
    const existing = (field as { directives?: unknown[] }).directives ?? [];
    return { ...field, directives: [...existing, { name: "inaccessible" }] };
  }

  // Fields come from SchemaRegistry (which has explicit type/nullable info from
  // registerTypeFields). Field names only discoverable via class instantiation are
  // appended by name.
  const augmentedTypes = base.types.map((typeDef: import("./registry").TypeDefinition) => {
    const meta = fedByName.get(typeDef.name);
    if (!meta) return typeDef;

    const cls = types.find((c) => c.name === typeDef.name);
    const registeredNames = new Set(typeDef.fields.map((f) => f.name));
    const mergedFields: MergedField[] = typeDef.fields.map((f) => withDirectives(f, meta));
    for (const fieldName of getClassFields(cls)) {
      if (registeredNames.has(fieldName)) continue;
      mergedFields.push(withDirectives({ name: fieldName }, meta));
    }

    return { ...typeDef, fields: mergedFields };
  });

  const federation: Record<string, unknown> = { enabled: true, version: "v2" };
  if (options.serviceName !== undefined) federation.service_name = options.serviceName;
  federation.entities = entities;
  if (shareableTypes.length > 0) federation.shareable_types = shareableTypes;

  return {
    ...base,
    federation,
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
 *
 * @param types - Array of class constructors to validate
 * @throws If any federation constraint is violated
 */
export function validateFederation(types: FederatedClass[]): void {
  for (const cls of types) {
    const meta = federationOf(cls);
    const allFields = getClassFields(cls);

    if (meta?.key !== undefined) {
      // @Key requires @Type
      const schema = SchemaRegistry.getSchema();
      if (!schema.types.some((t) => t.name === cls.name)) {
        throw new Error(`@Key requires @Type decorator on class ${cls.name}`);
      }

      // Key fields must exist on the class
      for (const field of topLevelFields(meta.key)) {
        if (!allFields.has(field)) {
          throw new Error(`Field '${field}' not found on type ${cls.name}`);
        }
      }
    }

    // @External requires @Extends
    if ((meta?.external_fields?.length ?? 0) > 0 && !meta?.extend) {
      throw new Error(`@external requires @extends on type ${cls.name}`);
    }
  }
}
