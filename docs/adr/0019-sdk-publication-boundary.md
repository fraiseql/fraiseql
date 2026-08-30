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
| Ever published to a registry | **3** — Rust (crates.io), Python (PyPI), TypeScript (npm) |
| Frozen at 2.1.6 while the engine was at 2.14.1 | 8 |
| Of those eight, carrying a real tag-triggered publish step | 6 |
| Registry probes for the eight (NuGet, Hex, RubyGems, Packagist, pub.dev) | all **404** |

So two SDKs were advertised as Supported that a user could not obtain at all, and six
publish jobs stood ready to push a version six minors stale to registries that do not
allow a version number to be re-used. Nothing had fired only because no SDK-scoped tag
has ever been pushed.

The root cause is that a *tier* is a promise about future effort. Nothing in a repository
can contradict a promise, so it drifts silently and is discovered years later by reading.

## Decision

**Replace the tier vocabulary with a property the repository can check: is this SDK
published to a registry, or is it source-only?**

- **Published (3):** Rust, Python, TypeScript. Versioned in lockstep with the engine by
  `tools/release.sh`, each with a publish job, each asserting its manifest matches the
  tag before pushing.
- **Source-only (8):** C#, Dart, Elixir, F#, Go, Java, PHP, Ruby. Tested on every push,
  scored by the conformance suite, and used by vendoring the directory. **No publish
  job.** A deleted publisher cannot push a stale version; a disabled one can be
  re-enabled by anyone who does not know why it was disabled.

Source-only is a statement about **distribution**, not quality. Most source-only SDKs
score 18/19 or 19/19 — better than published Rust, which scores 5/19 because it is
deliberately field-level-RBAC focused.

Moving an SDK into the published set means: add its manifest to `RELEASE_FILES`, add a
publish job that calls `assert_sdk_version_matches`, and add its registry to the README
table. The gate requires all three together, so an SDK cannot be half-published.

## Consequences

**Positive**

- The claim is checkable. `tools/check-sdk-publication-claims.py` compares three
  artifacts that exist for their own reasons — what `release.sh` bumps, what the
  workflows can push, and what `README.md` says — and fails when any one drifts.
- The irreversible failure is gone. Six publish jobs that would have pushed 2.1.6 no
  longer exist, and every remaining publisher asserts its version. Two of the three
  survivors (`rust-sdk.yml`, `python-sdk.yml`) had no such assertion; a direct SDK tag
  bypassed the H30 gate that `release.yml` applies.
- A reader is told how to obtain each SDK, including the eight where the answer is
  "vendor the directory".

**Negative**

- Eight SDKs a reader might have expected on a registry are visibly not there. This is a
  change in what is *said*, not in what was *available* — none of the eight had ever been
  published.
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
