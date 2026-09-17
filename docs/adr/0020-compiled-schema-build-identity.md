# ADR-0020: The compiled schema is a build artifact of the release that produced it

## Status: Accepted

Supersedes the implicit policy encoded in `crates/fraiseql-core/tests/schema_migration_test.rs`,
which asserted the opposite guarantee: that artifacts compiled by earlier `fraiseql-cli`
versions keep loading.

Related: #1304 (this decision), #1303 (`pagination_order`, the change that forced it),
`docs/operations/compiled-schema-lifecycle.md`, `docs/operations/zero-downtime-deploys.md`.

---

## Context

`schema.compiled.json` was guarded by an integer `schema_format_version`, compared at
boot against a `CURRENT_SCHEMA_FORMAT_VERSION` constant. The constant was `1` and had
never moved. Nothing said what would move it.

The question stopped being theoretical when #1303 added `QueryDefinition.pagination_order`.
The runtime reads a missing `pagination_order` as *the author declared no page order* —
which is a real, supported declaration, the escape hatch for a view carrying its own
`ORDER BY`. A schema compiled by 2.14 has no `pagination_order` on any query, because the
field did not exist. So on a 2.15 runtime, a 2.14 artifact turns every paginated read back
into the overlapping, row-skipping pages that 2.15 shipped to remove — under a `200`, with
nothing in a log.

#1303 did not bump the constant, because a bump refuses artifacts for reasons unrelated to
the one field, and that is a decision about compiled artifacts in general rather than about
`pagination_order`. This is that decision.

Three facts measured while making it:

1. **An absent field is a value.** The runtime does not distinguish "this compiler never
   wrote this key" from "the author chose the behaviour that key's absence selects".
   Nothing in the artifact format can make that distinction, because the artifact is the
   only evidence.
2. **The guard's hole was inverted.** `None` — no version field at all, i.e. pre-v2.1 —
   was accepted unconditionally. A bump would therefore have refused a 2.14 artifact while
   still admitting a 2.0 one.
3. **Both producers of a `CompiledSchema` looked identical.** `CompiledSchema::default()`
   and an artifact deserialized from JSON with no version key both yielded `None`, so the
   value could not be used to refuse the artifact without also refusing every schema the
   binary constructed in process.

## Decision

**A compiled schema is a build artifact of the fraiseql release that produced it. A runtime
runs its own build's artifacts and refuses every other.**

The compiler stamps `fraiseql_version` — its own `CARGO_PKG_VERSION`, which every crate in
the workspace shares — into each artifact it writes. Every seam that turns an artifact into
a running executor compares that stamp against its own build and refuses anything else,
including an artifact that names no build at all.

The comparison is an **equality**, not a range or a compatibility rule. A runtime asked
whether some other release's artifact is *close enough* would be making, silently and on
the operator's behalf, exactly the judgement #1303 got wrong.

### Why not a format version that bumps when meaning changes

That was the obvious alternative, and it fails on the same step: it requires a human to
notice, while implementing an unrelated feature, that adding a field changed what an
existing artifact means. #1303's author did notice — and filed #1304 rather than bumping,
correctly, because the bump is a policy decision. The next such change will be made by
someone who does not notice, and the mechanism's correctness should not depend on that.

A build stamp needs nobody to notice anything.

### Why over-refusing is the right bias

A patch release that changes nothing about the compiled format still invalidates every
artifact. That is accepted deliberately:

- a **false refusal** costs one recompile, and says exactly what to do;
- a **false acceptance** costs silent, correct-looking wrong answers — #1303's overlapping
  pages are the worked example, and an operator has no way to see them.

The costs are not comparable, so the mechanism is biased toward the cheap failure.

### What it costs, honestly

Anyone who ships `schema.compiled.json` separately from the binary must recompile it in
lockstep with every fraiseql upgrade, patch releases included. `compiled-schema-lifecycle.md`
already told operators to keep the CLI and server in lockstep; this makes the advice
enforced rather than advisory. Hot reload is unaffected in normal use — the artifact a
running binary reloads is produced by the same pipeline — but reloading an artifact
compiled by a *different* build is refused, which is the point.

## Consequences

- `schema_format_version`, `CURRENT_SCHEMA_FORMAT_VERSION` and `validate_format_version()`
  are removed. `fraiseql_version`, `CURRENT_FRAISEQL_VERSION`, `ProducerVersion` and
  `validate_producer_version()` replace them.
- `ProducerVersion::default()` is **this build**, and a missing JSON key deserializes to
  `ProducerVersion::unstamped()` — a third value. Keeping those apart is what lets an
  unstamped artifact be refused without also refusing every schema built in process.
  Collapsing them (by making the serde default this build) silently readmits every
  artifact ever produced; `a_missing_key_does_not_deserialize_to_this_build` pins it.
- The producing build is part of the content hash, so a cache keyed on it cannot carry a
  previous build's entries into this one.
- The backward-compatibility guarantee in `schema_migration_test.rs` is withdrawn. That
  file now asserts the refusal is a *good* one: the artifact still parses, so an operator
  meets a message naming both builds and the recompile, not a serde error about a key.
- **No escape hatch** — no environment variable, no `--allow-foreign-schema` flag. One
  would be reached for in exactly the incident it exists to prevent.

## Alternatives rejected

**Never version; treat the artifact as a build output and rely on the pipeline.** This is
what the project already believes — compiled schemas are regenerated on every build, and
none are in the wild. But nothing enforced it, and its failure mode is silent, which the
project's standing rule ("breaking is fine; *silent* breaking is not") rules out. This ADR
is that belief, enforced.

**Bump the format integer on any meaning-changing field, and refuse an absent version
too.** Strictly better than the status quo, and it was the near-miss option: it keeps the
human judgement that had already failed once, in exchange for not invalidating artifacts on
releases that change nothing.

**A fingerprint derived from the compiled-schema struct shape.** Fires only on real shape
changes, needs no judgement — but it is more machinery for a narrower guarantee, and it
misses a change in how the runtime *interprets* an unchanged shape, which is a meaning
change the version string catches for free.
