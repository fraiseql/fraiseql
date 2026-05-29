# ADR-0011: Release tooling — `release-please` evaluated, **not adopted**

- **Status:** Accepted (decision recorded 2026-05-29)
- **Decision:** FraiseQL will **not** adopt `release-please`. The current
  `tools/release.sh` + tag-driven `release.yml` flow stays canonical, with two
  small mitigations shipped alongside this ADR so that one `make release
  VERSION=x.y.z` invocation aligns 100% of version strings.
- **See also:** `/tmp/fraiseql-319-fix/decision.md` (full spike report) and
  issue #319.

## Context

Every release pays a "manually align N `Cargo.toml` files" tax. The
canonical past examples (cited in #319):

- **v2.3.0** ([06ccc8db8]) — a dedicated 30-file alignment commit because
  8 fuzz crates were at `2.1.0` and 2 Rust SDK crates at `2.1.6 / 2.1.4`.
- **v2.2.1** ([1834a7b83]) — 4 SDK manifests bumped by hand.
- **v2.3.1** (#318) — same 12-file dance for a one-line patch fix.

Issue #319 proposed evaluating [`release-please`] as a way to collapse the
manual alignment surface via *linked-versions*, generated CHANGELOG, and a
single always-open Release PR. The issue scoped a spike (no commitments)
with three acceptance gates: linked-versions config, GitHub workflow,
locally-rendered preview of the next Release PR.

## What was proposed (release-please adoption)

A standard `release-please-config.json` + `.github/workflows/release-please.yml`
pair, targeting 12 standalone `Cargo.toml` files plus the root workspace
manifest, with a `linked-versions` group keeping all of them in lockstep. The
existing `release.yml` would keep owning crates.io publishing (release-please
only owns version bumps + the tag + the GitHub Release).

## What was found (spike, 2026-05-29)

The full spike report lives at `/tmp/fraiseql-319-fix/decision.md`. The
summary of the four cruxes:

### Crux 1 — Workspace inheritance is unsupported: **BLOCKER**

`release-please` cannot bump `[workspace.package].version` /
`version.workspace = true`. Its Cargo updater targets the literal
`['package', 'version']` JSON path and the `cargo-workspace` plugin reads
`manifest.package?.version` per crate — neither understands workspace
inheritance.

This is [release-please issue #2478] ("value at path `package.version` is not
tagged", reported 2025-02-13 against core `16.12.0`, **closed**). The
maintainer-acknowledged resolution is to *abandon workspace inheritance*:

> "It only works if I remove `workspace.package` and `workspace.dependency`
> from root Cargo.toml and add respective values to [each member's]
> cargo.toml."

FraiseQL is exactly that case — 15 of 16 crates inherit from
`[workspace.package].version` (single source of truth at
`Cargo.toml:343`). Adopting release-please natively would force
**reversing** the workspace-inheritance migration already completed
before v2.3.1. Net result: more literal version strings to manage than
today, with a bot instead of `sed`.

### Crux 2 — CHANGELOG ownership clash: **unresolvable cleanly**

FraiseQL's `CHANGELOG.md` is hand-authored prose with custom sections
(`Fixed`, `Changed (additive, non-breaking)`, `Security`, `Known follow-ups
(#329)`, `Added`, `Documentation`, `Migration`). Entries are deliberately
multi-paragraph essays referencing issue numbers, failure modes, and
migration paths.

`release-please` always owns and rewrites a changelog file as the core of
its Release PR. There is no first-class "manage version + tag, leave the
changelog alone" mode. Pointing it at the real `CHANGELOG.md` overwrites
prose with terse, commit-subject-derived bullets — unacceptable. Pointing
it at a throwaway file means maintaining two changelogs, the useless one
in the Release PR body.

### Crux 3 — Combined-release / version-freeze fit: **partial friction**

The "long-lived Release PR" model maps onto FraiseQL's "accumulate fixes
until ready" pattern (e.g. the `fix/329-…` branch stacking #329 + #300 +
#326 + #170 + #148 + #319 + #149 + #291). The friction: release-please
*computes* the next version from conventional-commit types (`feat` ⇒
minor, `fix` ⇒ patch). FraiseQL deliberately freezes the version on these
combined branches and ships everything under `[Unreleased]`. We would be
fighting the auto-computed version on most releases with `Release-As:`
footers.

### Crux 4 — Tag/publish composition: **the one clean fit**

`release-please` creates the tag + GitHub Release on Release-PR merge.
`release.yml` triggers on `v*` tag push (`release.yml:3-6`) and reads the
version from the tag. So a release-please-created `vX.Y.Z` tag would
trigger the existing publish flow unchanged. Downstream of the two
blockers above, however, this is academic.

### Premise re-examination: the original tax is mostly gone

The empirical drift table from the spike:

| Release | `Cargo.toml` files bumped | Notes |
|---|---|---|
| v2.2.1 → v2.3.0 | **30** | Pre-migration — the painful release the issue cites. |
| v2.3.0 → v2.3.1 | 12 | Post-migration: root + 8 fuzz + storage + 2 SDK only. |
| v2.3.1 → v2.3.2 | 12 | Same shape. |

`version.workspace = true` (landed before v2.3.1) already collapsed the
15 member-crate bumps into the single `[workspace.package].version` line.
The 30-file v2.3.0 tax is gone. `tools/release.sh` already auto-bumps
10 of the remaining 12 manifests (root + 8 fuzz + storage). The only
manual hand-bumps were the 2 Rust SDK manifests under
`sdks/official/fraiseql-rust/` — which this ADR's companion commit
(`3eaa7bf45`) now closes.

## Decision

**Do not adopt `release-please`.** Ship the Option C mitigations from the
spike instead:

1. **`fraiseql-storage` migrated to `version.workspace = true`**
   (`crates/fraiseql-storage/Cargo.toml:3-7`). Removes the lone
   publishable outlier carrying a literal `version`, and incidentally
   fixes a stale `richer-xyz` `repository` URL.
2. **`tools/release.sh` extended** to also bump and stage the 2 Rust
   SDK manifests under `sdks/official/fraiseql-rust/`. The 8 fuzz crates
   were already bumped by the existing `find crates -name Cargo.toml`
   loop but were left unstaged, requiring a manual `git add` each
   release; they are now in the `RELEASE_FILES` stage list.

After (1) + (2), `make release VERSION=x.y.z` bumps **100%** of version
strings automatically; the only manual work per release is the prose
CHANGELOG entries — which is the entire point. That work is
human-authored value, not a tax to be automated away.

## Consequences

- **Positive:** Zero new moving parts. Existing `release.yml` /
  `release-smoke.yml` / `semver.yml` / `changelog-check.yml` /
  `cargo-deny` machinery is untouched. Manual flow remains cheap (`make
  release VERSION=x.y.z` then `git push` + tag push). Prose CHANGELOG
  stays first-class.
- **Negative:** No bot-mediated Release PR previewing the next bump.
  The 8 fuzz crates remain individually-versioned (still bumped by
  release.sh; cosmetic since they are `publish = false` dev tooling).
- **Neutral:** SDKs in other languages (Python, TypeScript, PHP, Go,
  Dart, Elixir, F#, Ruby, Java, C#) are versioned by their own
  manifests / workflows and stay out of scope for any workspace-level
  version tool. This was already true.

## Re-evaluate when

Both conditions must hold before this ADR is worth reopening:

1. `release-please` ships first-class support for
   `[workspace.package].version` / `version.workspace = true` (track
   [release-please issue #2478] and the `cargo-workspace` plugin), **or**
   FraiseQL deliberately moves off workspace inheritance for unrelated
   reasons.
2. **AND** the prose CHANGELOG is replaced by a generated changelog
   workflow (the spike's Crux 2 dissolves on its own if/when entries
   become bullet-summarised rather than essay-length).

Until then, manual flow + the mitigations in (1) and (2) above is
cheaper, safer, and preserves the CHANGELOG voice.

## What was NOT done (spike hygiene)

- No live `npx release-please --dry-run` (needs a GitHub token + network
  to the real repo; the decision is dispositive from issue #2478 + source
  reading).
- No prototype config committed anywhere (the decision is NO-GO; nothing
  to prove).
- No repo edits other than the two `Option C` mitigations
  (`3eaa7bf45`) and this ADR.

[06ccc8db8]: https://github.com/fraiseql/fraiseql/commit/06ccc8db8
[1834a7b83]: https://github.com/fraiseql/fraiseql/commit/1834a7b83
[`release-please`]: https://github.com/googleapis/release-please
[release-please issue #2478]: https://github.com/googleapis/release-please/issues/2478
