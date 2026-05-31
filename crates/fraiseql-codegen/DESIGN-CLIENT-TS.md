# Design: TypeScript client generation (`client::typescript`)

This document is the spec the generator honours. It is derived from the actual
`CompiledSchema` shape (verified in `fraiseql-core`) and from a 2025–2026 review
of schema-first GraphQL codegen practice (see *Type honesty* below).

## 0. What we generate, and from what

Input: a `fraiseql_core::schema::CompiledSchema` (parsed from `schema.compiled.json`).
Output: a `Generated` map (relative path → file content). The CLI writes it; this
crate never touches the filesystem.

This is **consumer-side** codegen — clients that *call* a FraiseQL API. It is the
inverse of `fraiseql generate <lang>` (authoring code fed back into the compiler),
which stays in `fraiseql-cli` and operates on a different IR.

## 1. File layout

```
types.ts          # object/interface result types, union aliases, relay helpers
enums.ts          # GraphQL enums as string-union types
inputs.ts         # GraphQL input objects as TS interfaces      (only if any exist)
queries.ts        # typed query functions + embedded documents
mutations.ts      # typed mutation functions + isErrorResult     (only if any exist)
subscriptions.ts  # (omitted in v1 — see §8)
relationships.ts  # relationship metadata map                    (only if any exist)
client.ts         # FraiseqlClient — fetch-based, zero runtime deps
index.ts          # re-exports
```

Conditional files are omitted entirely (not emitted empty); `index.ts` only
re-exports what exists.

## 2. The central decision: type honesty (scalar-default projection)

Our default document for every operation selects **leaf fields only**
(scalars, enums, lists-of-leaf) plus `__typename`. Composite fields
(object/interface/union references and lists thereof) require a sub-selection and
are **not** auto-selected in v1.

The generated result types therefore reflect **exactly** what the default document
fetches. The `User` interface contains its leaf fields and omits `tenant`/`posts`
entirely — it is not a mirror of the full GraphQL type, it is a mirror of *the data
a caller actually receives*.

**Why not full interfaces + a "nested fields aren't fetched" caveat?** Because that
type lies. The GraphQL Foundation's own codegen guidance
(<https://graphql.org/blog/2024-09-19-codegen/>) states schema types are unsafe for
partial selections. Every modern schema-first generator (genql's `__scalar: true`,
GraphQL Zeus selectors, gql.tada inference, GraphQL Code Generator's client-preset)
returns selection-scoped types, never full interfaces. And a prose caveat is
invisible to the type-checker — and to AI coding agents, whose first-try
correctness improves measurably when declared types match returned data
(type-constrained generation cut compiler errors 56–75%, PLDI 2025
arXiv:2504.09246). A full-interface-with-caveat actively leads an agent to write
`user.tenant.name` against a value that is `undefined` at runtime.

**Leaf classification** (drives both the document and the result type):

| `FieldType` | leaf? | in default doc & result type |
|---|---|---|
| `String`/`Int`/`Float`/`Boolean`/`ID`/`DateTime`/`Date`/`Time`/`Json`/`UUID`/`Decimal`/`Vector` | yes | included |
| `Scalar(name)` (custom/rich scalar) | yes | included |
| `Enum(name)` | yes (GraphQL leaf) | included |
| `List(inner)` where `inner` is a leaf | yes | included |
| `Object`/`Interface`/`Union` | no | **omitted** |
| `List(inner)` where `inner` is composite | no | **omitted** |

Forward path (documented follow-up, not v1): a bounded-depth expansion or an
opt-in selection builder (genql/Zeus style) that lets callers fetch relationships
and regenerates exact types. The v1 contract leaves room for it without lying now.

## 3. Scalar map (v1, plain)

| GraphQL / `FieldType` | TS |
|---|---|
| `String`, `ID`, `UUID`, `Decimal` | `string` |
| `Int`, `Float` | `number` |
| `Boolean` | `boolean` |
| `DateTime`, `Date`, `Time` | `string` (ISO 8601) |
| `JSON` | `unknown` |
| `Vector` | `number[]` |
| `Scalar(name)` (custom) | `string` (with `// TODO: brand <name>` note) |
| `Enum(name)` | the generated enum union type |
| `Object`/`Interface`/`Union(name)` | the generated type name (inputs/unions only) |

Branded/refined scalars are a downstream concern (zod/io-ts on top).

## 4. Nullability

Two encodings exist in the compiled schema, so there are two paths sharing one
scalar map:

- **Object/interface fields** (`FieldDefinition`) and **operation arguments**
  (`ArgumentDefinition`) use the structured `FieldType` enum with a separate
  `nullable: bool`. This carries **outer** nullability only — it cannot express
  inner-list nullability, so `[Post!]!` and `[Post]!` both render as `T[]`.
  Documented v1 simplification.
  - non-null field → `name: T;`  · nullable field → `name: T | null;`
  - list (outer non-null) → `T[]`  · list (outer nullable) → `T[] | null`
- **Input object fields** (`InputFieldDefinition`) use a GraphQL type **string**
  (`"String!"`, `"[Int]"`, `"UserRole"`). These are parsed preserving the full
  `!`/`[]` grammar, so input nullability is precise:
  - `T!` → `name: T`  · `T` → `name?: T | null`
  - `[T!]!` → `T[]` · `[T]!` → `(T | null)[]` · `[T!]` → `T[] | null` · `[T]` → `(T | null)[] | null`

## 5. `__typename` and discriminated unions

The mutation runtime **always injects `__typename`**, and our default document
**always selects `__typename`**, so it is reliably present. Every object/interface
result type carries a **required** discriminant:

```ts
export interface User {
  __typename: "User";
  id: string;
  email: string;
  displayName: string | null;
  role: UserRole;
  createdAt: string;
}
```

Interfaces use `__typename: string` (it is the union of implementors). Object types
`extends` the interfaces they implement (`implements: ["Node"]` → `extends Node`).

## 6. Mutations: result unions, not `MutationResponse<T>`

The plan's `MutationResponse<T>` (`{ succeeded, state_changed, data }`) does **not**
exist on the wire. Verified against `fraiseql-core` runtime: a mutation returns its
`return_type` directly — typically a **union** of the entity type and error types
(`is_error: true`), discriminated by `__typename`. On the error branch the runtime
injects a `status` field (the `error_class`). So:

- Error types (`is_error: true`) get an extra `status: string;` field.
- Mutations return their `return_type` (a union alias or an object type), unwrapped.
- A generated `isErrorResult` narrows a union result to its error members:

```ts
export type ErrorTypename = "EmailTakenError";
const ERROR_TYPENAMES: ReadonlySet<string> = new Set<string>(["EmailTakenError"]);

export function isErrorResult<T extends { __typename: string }>(
  value: T,
): value is Extract<T, { __typename: ErrorTypename }> {
  return ERROR_TYPENAMES.has(value.__typename);
}
```

Union return types emit inline fragments in the document:
`createUser(...) { __typename ... on User { … } ... on EmailTakenError { … } }`.

**Cascade** (`mutation_response.cascade`) can appear on success but has no
representation in the compiled schema (no field on `MutationDefinition`), so it is
not typed in v1. Documented follow-up.

## 7. Queries

- `single, nullable`  → `Promise<T | null>`
- `single, non-null`  → `Promise<T>`
- `returns_list`      → `Promise<T[]>`
- `relay`             → `Promise<Connection<T>>`

`Connection<T>`/`Edge<T>`/`PageInfo` are emitted in `types.ts` only when at least
one relay query exists. Argument object:

- built from explicit `arguments` (typed via the `FieldType` path).
- relay queries add `first?: number` / `after?: string` (spec-standard forward
  pagination, which FraiseQL's keyset relay implements). Backward pagination
  (`last`/`before`) is a follow-up, gated on confirming the server accepts them.
- **`auto_params` are not rendered in v1.** The auto-wired arg names
  (`where`/`orderBy`/`limit`/`offset`) depend on the schema's `naming_convention`
  and are not reliably derivable from the compiled schema, so emitting them would
  risk invalid documents (and GraphQL rejects unused/unknown variables). Documented
  follow-up. Only declared `arguments` (and relay `first`/`after`) become variables.
- if every argument is optional the arg object is optional with a `= {}` default;
  if any is required it is required; if there are no arguments the param is omitted.

Each operation embeds its document and unwraps the root field:

```ts
const GET_USER = `query getUser($id: ID!) { getUser(id: $id) { __typename id email displayName role createdAt } }`;

export async function getUser(client: FraiseqlClient, variables: { id: string }): Promise<User | null> {
  const data = await client.request<{ getUser: User | null }>(GET_USER, variables);
  return data.getUser;
}
```

## 8. Subscriptions

Server transport is **WebSocket-only** (`graphql-transport-ws` + legacy
`graphql-ws`), no SSE (`fraiseql-server/src/subscriptions/`). v1 does not emit a
subscriptions client (the fixture has none); when present, a WebSocket helper is
the shape to generate. Tracked as a follow-up.

## 9. Relationships

`relationships.ts` mirrors `TypeDefinition.relationships` (REST-embedding metadata)
as a `const` map for generic UI/tooling. `Cardinality` has **three** variants;
they are emitted faithfully in camelCase:

```ts
export type RelationshipCardinality = "oneToMany" | "manyToOne" | "oneToOne";
export interface RelationshipMeta {
  targetType: string;
  cardinality: RelationshipCardinality;
  foreignKey: string;
  referencedKey: string;
}
export const relationships = {
  User: {
    tenant: { targetType: "Tenant", cardinality: "manyToOne", foreignKey: "fk_tenant", referencedKey: "id" },
    posts:  { targetType: "Post",   cardinality: "oneToMany", foreignKey: "fk_author", referencedKey: "id" },
  },
} as const;
export type EntityRelationships = typeof relationships;
```

## 10. `client.ts` — runtime

A single hand-written template, identical for every generated client. Zero
dependencies beyond `fetch`. Exposes `FraiseqlClient` with a generic
`request<TData>(document, variables?): Promise<TData>` that POSTs
`{ query, variables }`, throws a `FraiseqlError` (carrying `errors[]`) on a GraphQL
error response, and returns `data`. A custom `fetch` and dynamic `headers`
(for auth) are injectable.

## 11. Schema-hash stamping

Every file begins with:

```ts
// AUTO-GENERATED by fraiseql-codegen. DO NOT EDIT.
// schema-hash: <sha256>
// fraiseql-codegen: <version>
```

The hash is `sha256` over a canonical (recursively key-sorted) JSON encoding of the
schema (`client::schema_hash`, landed in Phase 01). It is a stable consumer-facing
CI contract — recompute the live schema's hash and fail the build on drift.

## 12. Determinism

All iteration is over `Vec`/`BTreeMap` in declaration order; the output `Generated`
map is a `BTreeMap`. Identical input → byte-identical output (snapshot-stable).
