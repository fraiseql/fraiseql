# Cross-SDK conformance suite

Every official SDK's contract is one sentence:

> the `schema.json` it emits, compiled by the real `fraiseql compile`, must preserve what
> the author declared.

Nothing before this suite tested that sentence, and eleven SDKs drifted.

## Why the previous gate could not have caught anything

`.github/workflows/sdk-parity.yml` compared each SDK's `schema.json` against the Python
SDK's `schema.json`. Two structural problems made it incapable of failing:

1. **It never ran the compiler.** `#849` (C# silently drops `relay`/`is_error`) and `#852`
   (PHP writes `invalidates` where the compiler reads `invalidates_views`) both produce
   documents that compile cleanly and are wrong afterwards. Only comparing the *compiled*
   artifact shows the loss — the C# case changes the compiled type count from 3 to 6.
2. **Six of the eleven "parity generators" hand-wrote their JSON.** Java, C#, F#, Dart,
   Elixir and Ruby each built the expected bytes with literals and never called the SDK
   they claimed to test. A generator that constructs the answer passes whatever the SDK
   does — which is how Ruby came to have a README documenting a schema exporter that did
   not exist, and Dart to have no export path at all.

## What runs

```
run.py            the harness: export → compile → observe → diff
project.py        compiled schema → canonical observations, keyed by construct
manifest.json     per-SDK: how to drive its exporter, and its declared gaps
reference/        the canonical fixture, in wire format
expected.*.json   the observations the reference fixture compiles to
```

Per SDK, twice — once for each fixture:

1. run that SDK's `conformance/export.*`, which authors the fixture **through the SDK's
   public API**;
2. check no top-level array section was emitted as `null` (`#850`, which made every
   shipped Go example uncompilable);
3. run the real `fraiseql compile`;
4. reduce the compiled schema to *observations* and diff against `expected.*.json`.

Two fixtures, because they catch different things. `full` exercises every construct;
`minimal` is one type and one query with **no** enums, inputs, mutations or subscriptions
— and that emptiness is the point, since a fixture that populates every section can never
see a producer that marshals an empty collection to `null`.

## Running it

```bash
cargo build -p fraiseql-cli
python3 sdks/official/conformance/run.py --cli target/debug/fraiseql-cli
```

Add `--sdk go --sdk php` to narrow. Add `--require-all` to turn "toolchain missing" into a
failure rather than a skip — **CI uses this**, because a skipped SDK reads exactly like a
passing one in a log.

You do not need eleven language runtimes: any SDK whose toolchain is absent is run in the
container named in `manifest.json`, cached under `~/.cache/fraiseql-conformance`. Set
`FRAISEQL_CONFORMANCE_FORCE_CONTAINER=java` (or `=1` for all) when a toolchain is present
but unusable — a box with a JRE has `mvn` on `$PATH` and still cannot compile, and the
resulting error reads like an SDK defect rather than a local one.

## Observations, not bytes

A compiled schema carries SQL templates, filter-operator metadata, content hashes and
synthesized helper types. Diffing all of it would make every compiler change break eleven
SDK gates at once. Instead each compiled schema is reduced to the set of facts an author
declared that survived the pipeline, grouped by the **construct** that produces them —
`types`, `field_scope`, `type_relay`, `mutation_invalidates_views`, and so on.

`type_relay` is the shape to imitate when adding one. It does not merely assert the flag
survived; it asserts the compiler *acted* on it — the `Node` interface exists, the type
implements it, and `UserEdge`/`UserConnection`/`PageInfo` were synthesized. A flag that
arrives and does nothing is the defect one layer down.

## Declared gaps

Every construct is required by default. An SDK that genuinely cannot author one declares
it in `manifest.json` with a reason:

```json
"rust": {
  "unsupported": {
    "queries": "the Rust SDK is field-level-RBAC focused: it registers types and their
                field scopes and ships no builder for this construct"
  }
}
```

Three properties follow, and all three matter:

- **Adding a construct to `project.CONSTRUCTS` fails every SDK that has not implemented
  it** — the gate is opt-out, not opt-in, so a new authorable feature cannot quietly ship
  in one SDK and silently not exist in ten.
- **Declaring a gap is a deliberate, reviewed act with a published reason.** The reasons
  are the SDK support matrix in `../README.md`.
- **A declaration that is no longer true also fails.** A construct declared unsupported
  that the export in fact satisfies is reported, so the matrix cannot go stale into a
  published falsehood.

## Adding a construct

1. Declare it in `reference/full.json`.
2. Emit an observation for it in `project.project()` and list it in `project.CONSTRUCTS`.
3. `run.py --update` to re-record `expected.*.json`, and **read the diff** — it is the
   compiler contract, and a surprise there is a compiler finding, not a fixture chore.
4. Every SDK now fails. Implement it in each, or declare the gap with a reason.

## Adding an SDK

Add a `conformance/export.*` that authors both fixtures through the SDK's public API and
writes to `$FRAISEQL_CONFORMANCE_OUT`, then an entry in `manifest.json`. The exporter must
go through the same call the SDK's README tells a user to make — `ExportSchema`,
`SchemaExporter::exportToFile`, `export_json`. Assembling the expected JSON by hand is the
one thing it must not do; that is what the six broken generators did.
