# Official FraiseQL SDKs

These are the **authoring-layer SDKs**. They let you define your GraphQL schema
in your language of choice and generate the `schema.json` consumed by
`fraiseql-cli compile`. No runtime Rust dependency — pure authoring tools.

## SDK inventory

**Distribution.** Three SDKs are published to a registry and versioned in lockstep with
the engine; the other eight are source-only and are used by vendoring the directory. That
is a statement about distribution, not quality — most source-only SDKs out-score published
Rust, which is deliberately field-level-RBAC focused. See
[ADR-0019](../../docs/adr/0019-sdk-publication-boundary.md), and
`make lint-sdk-publication-claims` for the gate that keeps this column honest.


Conformance is measured by [`conformance/`](conformance/README.md): the canonical schema
is authored through each SDK's **public API**, compiled by the real CLI, and the compiled
result is compared against a shared expectation. The score is constructs satisfied out of
constructs in the fixture; a gap is declared in `conformance/manifest.json` with the
reason shown here, and the suite fails if a declared gap is no longer true.

| Directory | Language | Distribution | Conformance | Declared gaps |
|-----------|----------|--------------|-------------|---------------|
| `fraiseql-python/` | Python 3.11+ | PyPI | 33/34 | fact_tables: the SDK has no fact-table authoring |
| `fraiseql-typescript/` | TypeScript / Node.js | npm | 34/34 | — |
| `fraiseql-go/` | Go 1.23+ | — (source-only) | 32/34 | type_crud: the SDK ships no CRUD generator; subscription_policy: no authoring surface (#1657) |
| `fraiseql-php/` | PHP 8.2+ | — (source-only) | 32/34 | fact_tables: the SDK has no fact-table authoring; subscription_policy: no authoring surface (#1657) |
| `fraiseql-java/` | Java 21+ | — (source-only) | 32/34 | fact_tables: `@GraphQLFactTable` registers an ordinary type and emits no fact table; subscription_policy: no authoring surface (#1657) |
| `fraiseql-csharp/` | C# / .NET 8+ | — (source-only) | 31/34 | subscriptions: the SDK ships no subscription authoring surface at all (#1024); fact_tables: no fact-table authoring; subscription_policy: no authoring surface (#1657) |
| `fraiseql-fsharp/` | F# / .NET 8+ | — (source-only) | 31/34 | subscriptions: the SDK ships no subscription authoring surface at all (#1024); fact_tables: no fact-table authoring; subscription_policy: no authoring surface (#1657) |
| `fraiseql-elixir/` | Elixir | — (source-only) | 31/34 | subscriptions: the SDK ships no subscription authoring surface at all (#1024); fact_tables: no fact-table authoring; subscription_policy: no authoring surface (#1657) |
| `fraiseql-ruby/` | Ruby 3.2+ | — (source-only) | 31/34 | subscriptions: the SDK ships no subscription authoring surface at all (#1024); fact_tables: no fact-table authoring; subscription_policy: no authoring surface (#1657) |
| `fraiseql-dart/` | Dart / Flutter | — (source-only) | 30/34 | subscriptions: the SDK ships no subscription authoring surface at all (#1024); fact_tables: no fact-table authoring; enum_values: a Dart enum member carries no stored value, so every member is stored as its name; subscription_policy: no authoring surface (#1657) |
| `fraiseql-rust/` | Rust | crates.io | 7/34 | queries, mutations, subscriptions, enums and their values, input types, CRUD, relationships, Relay and error-type flags, serverless functions, fact tables, localized mutation arguments, list arguments: the Rust SDK is field-level-RBAC focused and ships no builder for them; subscription_policy: no authoring surface (#1657) |

The scores and gap counts above are checked against `conformance/manifest.json` by
`selftest.py`, because they had already drifted: the table read `19/19` when the fixture
carried 23 constructs, and Go's `type_crud` gap was not listed at all. A support matrix
nobody checks is a published claim about eleven SDKs that decays every time a construct is
added.

## One rule for nullability

Every SDK reads the nullability its language already declares, and adds no knob of its own
(#1586, #1608). **Every position (a field, a list, a list's items, at any depth) is non-null
unless the language marks it nullable**, and it is published and enforced as declared:

| SDK | A nullable position is written | A list is |
|---|---|---|
| Python | `T \| None` | `list[T]`, `list[T \| None]`, nested |
| Go | a pointer: `*T`, `*[]T` | a slice: `[]T`, `[]*T`, nested |
| Java | `@GraphQLField(nullable = true)` for a field; an `Optional<T>` element, or one whose type carries a runtime-retained type-use annotation named `Nullable` (JSpecify's): `List<@Nullable Double>` | an array or a `Collection` declared with its element type; a raw `List` is refused |
| C# | `T?` (`double?`, and `string?` under nullable reference types) | an array or a one-argument generic collection: `List<double?>` |
| F# | `option` / `Nullable<>` (or `Nullable = true`) | an array or a one-argument generic collection: `float option list` |
| PHP | `?T` / `T\|null`; for an element, in the docblock: `/** @var list<?float> */` | an `array` whose docblock gives its element type; an `array` without one is refused |
| Elixir | the GraphQL type string: `"[Float]"` | Ecto's `{:array, :float}` (`[Float!]`), or a type string |
| TypeScript, Ruby, Dart, Rust | the GraphQL type string they are given, published as written: `"[Float]"`, `"[[Float]!]"` | |

`conformance/`'s `list_fields` and `list_arguments` constructs hold all eleven to it.

The same principle decides what an **enum member is stored as** (#1580, #1611): the value
the language gives it, a string or an integer, as the language's standard serializers write
it, and its name when the language gives it none; any other value is refused where the enum
is declared, naming the member, and a member whose value is its name is exported without
one. A request names the member; the database stores and compares the value; a stored
value is served as the member's name.

| SDK | An enum is declared | A member is stored as |
|---|---|---|
| Python | `@fraiseql.enum class Priority(Enum)` | `.value` (`LOW = 1` is `1`, `auto()` included) |
| TypeScript | `enum_("Priority", Priority)`, or a plain object | the enum's runtime value (a numeric enum's reverse mapping is skipped) |
| Go | `fraiseql.Enum("Priority", fraiseql.EnumMember{Name: "LOW", Value: PriorityLow}, …)` | the constant's value, of a string or integer kind; the name when `Value` is nil |
| PHP | `StaticAPI::enum(Priority::class)` | a backed enum's value; a pure enum's case name |
| Java | `registerEnum("Priority", Map.of("LOW", 1, …), …)` | the map's value, a `String` or an integer |
| C# | `RegisterEnum<Priority>()` | the member's integer, or its name (or `[JsonStringEnumMemberName]`) under `[JsonConverter(typeof(JsonStringEnumConverter))]`, as `System.Text.Json` writes it |
| F# | `registerEnum typeof<Priority> None` | a .NET enum's integer (or name, as C#); a fieldless union case's name |
| Ruby | `schema.enum "Priority", { LOW: 1, HIGH: 3 }` | the Hash's value, as Rails writes `enum`; an Array names members stored as their names |
| Elixir | `fraiseql_enum "Priority", values: [LOW: 1, HIGH: 3]` | the keyword list's value, as Ecto.Enum writes it; a list names members stored as their names |
| Dart | `enumType('Priority', ['LOW', 'HIGH'])` | its name (a Dart enum has no serializer value) |
| Rust | no enum builder | |

`conformance/`'s `enum_values` construct holds every SDK to it.

The TypeScript decorators `@Type`, `@Query`, `@Mutation` and `@Subscription` are **not**
an authoring path — TypeScript erases the types they would need, so they refuse rather
than register the placeholders they used to. Use `registerTypeFields` / `registerQuery` /
`registerMutation`.

## SDK layout convention

Each SDK follows the same structure:

```
fraiseql-<lang>/
├── src/          # Source code
├── tests/        # Test suite
├── conformance/  # Cross-SDK conformance exporter (see ../conformance/)
├── examples/     # Usage examples
├── README.md     # Language-specific usage guide
└── <manifest>    # pyproject.toml / package.json / pom.xml / go.mod / …
```

Build artifacts (`dist/`, `target/`, `node_modules/`, `.venv/`) are gitignored.

## Relationship to the Rust engine

```
SDK (authoring)          CLI (compilation)        Server (runtime)
fraiseql-python/   →  fraiseql-cli compile  →  fraiseql-server
fraiseql-typescript/     schema.json                schema.compiled.json
…                        + fraiseql.toml            loaded at startup
```

The SDKs produce `schema.json`. The CLI validates and compiles it to
`schema.compiled.json`. The server loads the compiled schema at startup —
no SDK dependency at runtime.

The format is specified in
[`docs/architecture/intermediate-schema.md`](../../docs/architecture/intermediate-schema.md),
whose worked examples are the conformance fixtures themselves, compiled on every CI run.

## CI

| Workflow | What it gates |
|---|---|
| `.github/workflows/sdk-conformance.yml` | The table above: every SDK authors the canonical schema, compiles it, and must preserve what was declared. Hard gate, no skips. |
| `.github/workflows/sdk-parity.yml` | Cross-SDK agreement on the emitted `schema.json`, against the Python reference. |
| `.github/workflows/<lang>-sdk.yml` | Each SDK's own unit tests and lints. |
