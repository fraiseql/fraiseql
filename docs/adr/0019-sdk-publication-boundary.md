# ADR-0019: SDK publication is the boundary, not a support tier

## Status: Accepted (2026-08-30)

Supersedes [ADR-0005](0005-sdk-tier-strategy.md).

## Context

ADR-0005 sorted the SDKs into Tier 1 (Supported) / Tier 2 (Maintained) / Community
(Deprecated). Five years of drift later, that document and the repository disagreed on
almost every point:

- It called .NET, Ruby, Elixir and Dart "Community (Deprecated)". All four now live in
  `sdks/official/` and score 18/19 on the conformance suite.
- It did not mention F#, which exists.
- `README.md` reproduced its table, advertising **Java and Go as Tier 1 (Supported)**.

Measured on `dev` at `8a9295f3e` (2026-08-30), the state behind those claims was:

| | |
|---|---|
| Official SDKs | 11 |
| Ever published to a registry | **2** — Python (PyPI), TypeScript (npm) |
| Frozen at 2.1.6 while the engine was at 2.14.1 | 8 |
| Of those eight, carrying a real tag-triggered publish step | 5 (C#, Dart, Elixir, F#, Java) |
| Plus the community Ruby gem, publishing a gemspec declaring 1.0.0 | 1 (#1237) |
| Registry probes for the eight (NuGet, Hex, RubyGems, Packagist, pub.dev) | all **404** |

So two SDKs were advertised as Supported that a user could not obtain at all, and six
publish jobs stood ready to push a stale version to registries that do not allow a
version number to be re-used. Nothing had fired only because no SDK-scoped tag has ever
been pushed.

Two corrections to that count, both measured rather than read:

- **Rust had never been published either.** It sits in `tools/release.sh`'s bump set and
  carries a publish job, which is why it reads as published — but `fraiseql-rust` answers
  404 on crates.io at every version, because its publisher fires on a `rust-sdk/v*` tag
  that `release.sh` does not create. Designated-published is not published; see the
  consequence below.
- **Four of the five, not five, were reachable by pushing their tag.** `csharp-sdk.yml`
  declares `push: branches: ['**']` with no `tags:` filter, and GitHub ANDs the ref filter
  with the path filter, so a tag push matched no ref pattern and never started the
  workflow (#1119). Its publisher could only have fired from a GitHub *Release* whose tag
  was named `csharp-sdk/v*`, which nothing in this repository creates. Dart, Elixir, F#,
  Java and the community Ruby gem were reachable: a `paths:` filter is not evaluated for
  tag pushes, so a paths-only workflow runs on every tag.

The root cause is that a *tier* is a promise about future effort. Nothing in a repository
can contradict a promise, so it drifts silently and is discovered years later by reading.

## Decision

**Replace the tier vocabulary with a property the repository can check: is this SDK
published to a registry, or is it source-only?**

- **Published (3):** Rust, Python, TypeScript. Versioned in lockstep with the engine by
  `tools/release.sh`, each with a publish job **that the `v*` release tag reaches**, each
  asserting its manifest matches the tag before pushing. Python publishes from
  `release.yml:publish-python`, TypeScript from `npm-publish.yml:publish-npm`, Rust from
  `release.yml:publish-rust-sdk`. v2.15.0 is the Rust SDK's first release: until it, the
  only Rust publisher was `rust-sdk.yml:publish` on a tag no release creates.
- **Source-only (8):** C#, Dart, Elixir, F#, Go, Java, PHP, Ruby. Tested on every push,
  scored by the conformance suite, and used by vendoring the directory. **No publish
  job.** A deleted publisher cannot push a stale version; a disabled one can be
  re-enabled by anyone who does not know why it was disabled.

Source-only is a statement about **distribution**, not quality. Most source-only SDKs
score 18/19 or 19/19 — better than published Rust, which scores 5/19 because it is
deliberately field-level-RBAC focused.

Moving an SDK into the published set means: add its manifest to `RELEASE_FILES`, add a
publish job that calls `assert_sdk_version_matches` **and that a `v*` tag runs**, and add
its registry to the README table. The gate requires all four together, so an SDK cannot be
half-published — and the fourth is not redundant: Rust satisfied the first three for as
long as this repository has existed and was never published once.

## Consequences

**Positive**

- The claim is checkable. `tools/check-sdk-publication-claims.py` compares three
  artifacts that exist for their own reasons — what `release.sh` bumps, what the
  workflows can push, and what `README.md` says — and fails when any one drifts.
- The irreversible failure is gone. Six publish jobs that would have pushed a stale
  version no longer exist, and every remaining publisher asserts its version. Two of the
  three survivors (`rust-sdk.yml`, `python-sdk.yml`) had no such assertion; a direct SDK
  tag bypassed the H30 gate that `release.yml` applies.
- The opposite failure is gone too, and it was the quieter one: an SDK can no longer be
  *claimed* published while no release publishes it. The gate requires each published SDK
  to have a publisher the release tag actually reaches, decided from GitHub's ref-filter
  rules rather than from the job existing.
- A reader is told how to obtain each SDK, including the eight where the answer is
  "vendor the directory".

**Negative**

- Eight SDKs a reader might have expected on a registry are visibly not there. This is a
  change in what is *said*, not in what was *available* — none of the eight had ever been
  published.
- v2.15.0 creates a new public crates.io package, `fraiseql-rust`. It is permanent: a
  crates.io name cannot be released once taken, and a published version cannot be
  withdrawn. The alternative was to leave the README naming a registry a reader would find
  empty.
- Vendoring is a worse consumption story than a package manager. It is the honest one
  until someone commits to maintaining a registry release for that language.

**Neutral**

- Nothing is deleted. All eleven SDKs keep their tests, their conformance fixture and
  their place in `sdks/official/`.

## Alternatives considered

1. **Publish all eleven at 2.15.0.** Makes the old Tier 1 claim true, at the cost of
   eight new public registry packages — each with an account, a secret, a deprecation
   cost, and a promise to keep bumping. Rejected: the maintenance burden ADR-0005 was
   written to avoid, re-incurred to satisfy a sentence in a README.
2. **Keep the tiers and correct the table.** Rejected: the failure mode is not that the
   table was wrong once, it is that a promise cannot be checked. It would drift again.
3. **Leave the publish jobs in place but gate them on a version match.** Safer than
   nothing, and it was the first design. Rejected in favour of deletion: a gated
   publisher for an SDK no release bumps is a job that can only ever fail, and a job that
   can only fail is one someone eventually "fixes" by bumping the manifest by hand.
