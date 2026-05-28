# ADR-0014: Release-please automation spike

**Status:** Proposed (spike — not yet decided)
**Issue:** #319
**Branch:** `spike/release-please-evaluation`
**Date:** 2026-05-28

## Context

The v2.3.0, v2.3.1, and v2.3.2 releases each required a manual "align all
Cargo.toml versions" step. The drift specifically came from manifests that
`tools/release.sh` does **not** touch:

- `crates/*/fuzz/Cargo.toml` (8 files — server, auth, arrow, core, db,
  federation, secrets, wire)
- `crates/fraiseql-storage/Cargo.toml` (standalone-versioned, not
  workspace-inherit — see commit 06ccc8db8 for the back-fix)
- `sdks/official/fraiseql-rust/Cargo.toml` +
  `sdks/official/fraiseql-rust/fraiseql-client/Cargo.toml` (Rust SDK
  workspace)

That's 12 standalone-versioned Cargo.toml files including the workspace root.
`tools/release.sh` (line 60-ish, the `while IFS=` loop) updates only
`crates/*/Cargo.toml`, missing every nested manifest. The result: tagged
releases where some sub-crates are stale until somebody notices and pushes a
fixup commit.

#319 asked whether release-please can take this over.

## What this spike produces

Four artifacts live on this spike branch:

1. **`release-please-config.json`** — the static config. Declares all 12
   packages, groups them via the `linked-versions` plugin (so every release
   bumps all of them together), and maps Conventional Commits types to
   CHANGELOG sections matching the existing Keep-a-Changelog format.

2. **`.release-please-manifest.json`** — the version map. Seeded at 2.3.2
   to match the post-tag state of every package. release-please mutates
   this file on each release; it is the single source of truth for "what
   version are we at."

3. **`.github/workflows/release-please.yml`** — runs on push to `dev`.
   Either opens/updates a "Release PR" or no-ops based on commits since the
   previous tag.

4. **This document.**

## Dry-run result against current dev

I attempted `npx release-please release-pr --dry-run` locally to render what
the next Release PR would contain. **The CLI cannot do a fully local dry-run** —
it fetches config and manifest from the remote target branch via the GitHub
API, so until this spike branch is merged (or pushed and given a draft PR),
no remote `release-please-config.json` exists for it to read. Verified by
running:

```bash
TOKEN=$(gh auth token) npx --yes release-please release-pr \
  --token="$TOKEN" --repo-url=fraiseql/fraiseql \
  --target-branch=spike/release-please-evaluation \
  --config-file=release-please-config.json \
  --manifest-file=.release-please-manifest.json \
  --dry-run
# → ConfigurationError: Missing required manifest config (branch not pushed)
```

**Hand-computed preview** (what release-please *would* produce on a real
run against `dev` tip = `d0a4ed4ec`):

```bash
git log --no-merges v2.3.2..dev --pretty='%h %s'
# (empty — zero commits since v2.3.2 was tagged today)
```

→ release-please would correctly **no-op**: no Release PR opened, nothing
to release. The earliest meaningful preview happens after the next
`feat(...)` / `fix(...)` / `BREAKING CHANGE` commit lands on dev.

To make the dry-run actually executable as part of CI validation later,
either:
- Push the spike branch and let GitHub Actions run release-please in
  `workflow_dispatch` debug mode against the spike branch, or
- Use [release-please-action's `output-only` mode](https://github.com/googleapis/release-please-action#outputs)
  on a temporary workflow.

## Sharp edges encountered

### 1. SDKs in other languages are not in the linked group

Python and TypeScript SDKs are at 2.1.6 (Ruby gem is computed from a
constant). Rust is at 2.3.2. They are on independent cadence — adding them
to `linked-versions` would force-bump them on every Rust release, which is
exactly wrong. They stay out of release-please for now.

If we later want a single "everything moves together" cadence, those SDKs
would need their own release-please configs (or get folded into this one
with `release-type: python` / `node` per-package). Out of scope for this
spike — flagged as a follow-up only if/when the cadence story changes.

### 2. The `rust` release-type and workspace inheritance

The `rust` release-type handles `Cargo.toml`'s `[package].version` field.
We have 16 crates with `version.workspace = true` — release-please leaves
those alone (no per-file edit needed; they pick up the workspace root
bump). The 12 standalone manifests get individually patched. I have *not*
verified end-to-end against a real Cargo workspace with this exact layout;
that's the main risk to test before going live.

### 3. The Cargo.lock regen

release-please does not run `cargo update -p fraiseql --precise X.Y.Z` to
keep `Cargo.lock` in sync. The Release PR will fail CI on `--locked` builds
unless one of:
- Add a `cargo-workspace` plugin step (the spike config includes
  `plugins: [{type: "cargo-workspace"}]` — this is supposed to regen the
  lockfile, but I have NOT verified it works as advertised for our
  workspace shape).
- A separate workflow auto-commits the lockfile update onto the Release
  PR branch.
- Manual intervention on each Release PR.

This is the highest-risk unknown. Plan a follow-up issue to nail down lockfile
handling before merging the spike, even if everything else looks clean.

### 4. Branch protection

`dev` currently allows direct pushes by the release-please bot? Not yet
verified. If `dev` has required-status-checks blocking, the bot's PRs may
fail to land. Pre-merge action: confirm the release-please bot (or a
service-account PAT in `RELEASE_PLEASE_TOKEN`) can author + merge PRs.

### 5. Existing release pipeline interaction

| Stage | Today | With release-please |
|---|---|---|
| Bump 12 Cargo.tomls | `tools/release.sh` (incomplete — misses 9 files) | release-please (covers all 12) |
| Promote `[Unreleased]` → versioned section in CHANGELOG | `tools/release.sh` (sed insert) | release-please (auto-generated from commits) |
| Create annotated tag `vX.Y.Z` | `tools/release.sh` | release-please (on Release PR merge) |
| Create GitHub Release | manual `gh release create` | release-please (on Release PR merge) |
| Validate prereqs (CARGO_TOKEN, etc.) | `release.yml::validate-release` | unchanged — keep release.yml |
| Run `cargo publish` per-crate in topological order | `release.yml::publish-*` jobs (15 crates, hand-ordered) | unchanged — keep release.yml |
| npm/PyPI publish | `release.yml` | unchanged — keep release.yml |
| release-smoke test on tag | `release-smoke.yml` (#317) | unchanged |

## Retirement plan

If we adopt this spike:

1. **`tools/release.sh` → retired.** It does the right thing for step 1-4
   of the existing flow but incompletely (misses the 9 non-workspace
   manifests). With release-please owning bump + CHANGELOG + tag, the
   script becomes dead code. Keep it on disk for one release as a
   fallback, then delete in the v2.4.0 cycle.

2. **`.github/workflows/release.yml` → kept, simplified.** The
   `validate-release` job (secrets check, dry-run packaging gate) stays
   — release-please does not run `cargo publish`. The `prepare-release`
   logic in release.yml can shrink because release-please now owns the
   tag/release creation, but the actual publish jobs (15 crates +
   PyPI + npm) keep their existing trigger of `push: tags/v*`.

3. **`.github/workflows/release-smoke.yml` → kept unchanged.** Triggers on
   the `v*` tag, which release-please still creates.

4. **CHANGELOG.md format.** release-please's default sectioning is close
   to but not identical to the current Keep-a-Changelog layout. The
   `changelog-sections` block in the config maps `feat→Added`,
   `fix→Fixed`, `security→Security`, `perf/refactor→Changed`,
   and hides `docs/test/ci/build/chore` from the published CHANGELOG.
   Section heading style (`### Added`) is preserved.

5. **Conventional Commits enforcement.** Recent commit history (last 30
   non-merge commits) already follows Conventional Commits closely
   (`fix(server):`, `feat(...)`, `refactor!:`). release-please will pick
   these up natively without a hard enforcement gate. *Optional follow-up:*
   wire `commitlint` into pre-commit hooks if drift becomes a problem.

## Recommendation

**Conditional adopt.** The config layout, linked-versions plugin, and
workflow look sound and would fix the v2.3.x drift class. Two unknowns
must be resolved on a follow-up before this lands on `dev`:

1. **Verify `cargo-workspace` plugin regenerates Cargo.lock correctly**
   for our 28-crate workspace shape. Do this by pushing this spike branch,
   landing a synthetic `feat(...)` commit on a throwaway branch, running
   release-please in workflow_dispatch debug mode against it, and confirming
   the generated Release PR includes a working `Cargo.lock` diff.

2. **Confirm release-please bot permissions** on `dev` — branch
   protection, required-status-checks, PR self-approval.

If (1) fails — i.e. release-please can bump Cargo.tomls but breaks the
lockfile — the fallback is to write a `scripts/bump-versions.sh` that
just covers the 9 missing manifests (essentially patching `tools/release.sh`
to find every `Cargo.toml` with a literal version, not just `crates/*`).
That's a lower-leverage but lower-risk path.

## Next steps if accepted

- [ ] Push this spike branch as a draft PR for review
- [ ] In the same PR (or a follow-up), run release-please in
      workflow_dispatch debug mode and attach the dry-run output
- [ ] Verify `Cargo.lock` regen works
- [ ] Verify release-please bot can open PRs against `dev`
- [ ] Open a follow-up issue tracking the `tools/release.sh` retirement
      (1-release grace period)

## Next steps if rejected

Open a follow-up issue: "scripts/bump-versions.sh — fix the 9 missing
manifests in `tools/release.sh`." The drift problem is fixable without
release-please by widening the find scope; the spike just shows the
larger-leverage option.
