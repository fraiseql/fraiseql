# Branching Model — Trunk-Based Development

FraiseQL uses **trunk-based development**: a single long-lived branch that everyone
integrates into continuously, with short-lived topic branches and feature flags
instead of long-lived parallel branches. This document is the authoritative statement
of the model — the [Development Workflow](../../CONTRIBUTING.md#development-workflow)
and [Release Process](../../CONTRIBUTING.md#release-process) sections are the
step-by-step how-to; this is the *why* and the rules.

## The one rule that matters most

**Trunk first.** Every change — feature or fix — lands on the trunk first. A release
line only ever receives **cherry-picked backports from the trunk**, never original
commits. This is the rule whose absence caused real feature loss (see
[Why we do this](#why-we-do-this)).

## The trunk

- **`dev` is the trunk** and the repository's **default branch**. All development
  integrates here.
- It is **always releasable**: every commit on `dev` should compile, pass the gates,
  and be safe to tag.
- **`main` is a mirror of `dev`**, kept in sync. It carries no independent work.
  (Historically `main` was a parallel line; it drifted and stranded features — it was
  consolidated onto `dev` on 2026-07-04. Do not commit original work to `main`.)

## Topic branches

- Branch off `dev`, keep it **short-lived** (hours to a few days), and open a PR back
  to `dev`.
- Naming: `feat/<slug>`, `fix/<slug>`, `chore/<slug>`, `docs/<slug>`,
  `refactor/<slug>`.
- PRs are **squash-merged** into `dev`, so the trunk history stays linear and each
  merge is one reviewable, revertable unit.
- Delete the branch after merge.

## Incomplete work rides behind feature flags, not long-lived branches

Work that isn't ready to be on by default lands on the trunk **behind a Cargo
feature** (or a config gate), disabled by default, and is promoted to stable later —
rather than living on a branch that drifts from the trunk.

- Examples: `functions-runtime-deno`, `inbound`, `inbound-email`, `saga` all landed
  opt-in and were (or will be) promoted once hardened.
- This keeps every contributor integrating against the same trunk daily, which is the
  whole point of trunk-based development.

## Releases

- Releases are **semver tags cut from the trunk** (`vX.Y.Z`). Bump the workspace
  version and `CHANGELOG.md` via a `chore/release-prep` PR to `dev`, then tag `dev`.
  Pushing the tag triggers the release workflow. See
  [Release Process](../../CONTRIBUTING.md#release-process).
- **Patching an older released line** is the *only* sanctioned use of a longer-lived
  branch: cut a short-lived `release/X.Y` branch from the tag, and land the fix **on
  the trunk first**, then cherry-pick it onto `release/X.Y`. Never the reverse.
- `release/*` branches also drive the release-smoke workflow; keep them short and
  delete them once the patch ships.

## Keeping the trunk green

- Every push runs the strict gates locally (`make preflight`) and the Dagger CI legs
  (preflight / test / security / feature-matrix / integration).
- **Recommended hardening (not yet adopted):** a **GitHub merge queue** with the
  Dagger legs as **required checks on the PR**, so changes are validated against
  trunk-HEAD and merged only when green. Today the Dagger legs run *post-merge* on the
  `dev` push (and must be dispatched manually to validate a PR — see
  [`.github/workflows/dagger-*.yml`](../../.github/workflows/)), which leaves a window
  where the trunk can go red between merges. A merge queue closes that window.

## Why we do this

Trunk-based development is not an aesthetic preference here — it is the direct fix for
a failure this project actually hit.

FraiseQL previously kept `main` and `dev` as two long-lived branches. `dev` raced
ahead (v2.2 → v2.10 in about three months) while `main` was left at v2.2.1. Because
they diverged and fixes were not consistently trunk-first:

- **Claims enrichment (#242)** shipped on `main`/`v2.2.1` and was **never
  forward-ported to `dev`** — so the live line silently lost the feature, and its
  config parsed but did nothing. It has to be rebuilt (tracked as a fresh issue)
  rather than merged.
- Several **camelCase → snake_case casing fixes** landed on `main` and had to be
  independently re-derived on `dev`.

The common cause was a long-lived parallel branch plus non-trunk-first commits. A
single trunk with trunk-first backports makes that class of loss structurally
impossible: there is one place a change can live, and release lines can only *pull
from* it.

## Anti-patterns

| Don't | Do instead |
|-------|------------|
| Commit original work to `main` or any release branch | Land it on `dev`, then cherry-pick to the release branch |
| Keep a long-lived feature branch | Land behind a default-off Cargo feature on `dev` |
| Let `main` and `dev` diverge | Keep `main` a mirror of `dev` (or retire `main` and use `dev` alone) |
| Cherry-pick trunk ← release | Always cherry-pick trunk → release |

## Open adoption items

These formalize the model further; tracked for follow-up:

- **Single trunk name.** `main`/`dev` are currently kept in sync; the cleanest end
  state is one trunk (either retire `main` and keep `dev`, or rename the trunk to
  `main` and retire `dev`). Two branches held in sync is the divergence risk this
  model exists to remove.
- **Merge queue + Dagger-on-PR** (see [Keeping the trunk green](#keeping-the-trunk-green)).
