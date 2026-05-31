# fraiseql-codegen

Code generation for FraiseQL: typed **client** artefacts produced from a compiled
schema (`schema.compiled.json`).

Given a [`CompiledSchema`](https://docs.rs/fraiseql-core), this crate emits
source code that *consumers* of a FraiseQL API use to call it in a type-safe way —
TypeScript interfaces for every GraphQL type, typed query/mutation functions, a
`MutationResponse<T>` discriminated union, relationship metadata, and a tiny
`fetch`-based runtime client.

```rust
use fraiseql_codegen::client;
use fraiseql_core::schema::CompiledSchema;

let schema = CompiledSchema::default();
let files = client::typescript::generate(&schema)?;
for (path, contents) in &files {
    // write `contents` to `out_dir.join(path)`
}
# Ok::<(), fraiseql_codegen::FraiseQLError>(())
```

The crate is **filesystem-free**: generators return a `BTreeMap<PathBuf, String>`
(relative path → content) and the caller decides where to write. This makes the
API consumable by the `fraiseql` CLI, IDE extensions, scaffolders, and build
plugins alike.

## Not to be confused with `fraiseql generate`

The CLI's `fraiseql generate <language>` command emits **authoring** code —
FraiseQL type/query definitions in another language, fed *back into* the compiler.
This crate is the inverse: it consumes the compiler's *output* to build clients
*for callers of* the API. See the crate-level docs for the architectural rationale.

## Schema-hash stamping

Every generated file carries a `schema-hash` of the schema it was generated from.
Recompute the live schema's hash in CI and fail the build when the generated
client drifts out of date.

## License

Licensed under either of MIT or Apache-2.0 at your option.
