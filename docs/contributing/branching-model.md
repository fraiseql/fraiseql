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

The **fast** Dagger legs — `preflight` (shell gates · fmt · clippy · rustdoc) and
`security` (`cargo deny`) — run on **push to every in-repo branch**
(`branches-ignore: [dependabot/**, …]`) on the self-hosted runner, with a warm sccache
cache. Because **forks cannot push to this repo**, a `push` trigger only ever runs
trusted in-repo code — so these legs are **fork-safe by construction**, with no
`pull_request` trigger and no self-hosted exposure to fork code. (A `pull_request` run
would execute the PR *merge-ref's* workflow definition, which a fork can edit — hence
its deliberate absence.)

Push-triggered check runs attach to the commit SHA, so they show on any open PR for
that branch, and the **`dev` ruleset requires `preflight` + `security`** — a PR cannot
merge until both are green on its head. That is what prevents "merged before CI was
verified."

The **heavy** legs (`test` / `feature-matrix` / `integration`, which spins up
PostgreSQL) run **post-merge on the `dev` push** to spare the single runner; dispatch
them manually (`gh workflow run dagger-<leg>.yml --ref <branch>`) when a change
warrants full validation before merge. Locally, `make preflight` mirrors the fast gate.

### Fork PRs and Dependabot PRs

Two PR categories don't produce an in-repo `push` under a full-token actor, so the
required checks are **absent** and the merge is blocked by design:

- **Fork PRs** — a fork cannot push here, and fork code must never run on the
  self-hosted runner.
- **Dependabot PRs** — `dependabot/**` branches are excluded from the trigger because
  Dependabot's degraded token cannot pull the private ghcr base image.

Unblock either the same way — **push the reviewed head commit to an in-repo `ci/…`
branch** under your own actor:

```bash
git fetch origin pull/<N>/head
git push origin FETCH_HEAD:refs/heads/ci/review-<N>
```

The SHA is identical, so the push fires the legs under a full token (the ghcr pull
works), the checks land on that SHA, and the PR's merge box goes green.

For a **`cargo` bump this is not overhead — it is the point**: `cargo deny` runs against
the actual bumped `Cargo.lock` before merge, which is exactly where it earns its keep
(e.g. the #515 AWS-SDK bump tripped `bans.multiple-versions`). For a pure SDK/npm dep
bump the Rust legs assert nothing, so an **admin bypass** once the SDK CI is green is
also fine.

Do **not** add a blanket Dependabot bypass to the ruleset — it would exempt the one PR
category (`cargo`) with the best track record of catching a real problem at the gate.

### In-flight branches

`push` runs the workflow file from the pushed ref, so branches created before this
change won't have the new trigger until they are **rebased on `dev`** — otherwise the
required checks never appear and the ruleset blocks the merge. Rebase once and push.

### Workflow token permissions

The repository's default `GITHUB_TOKEN` permission is **`read`** (least-privilege). A
workflow (or a single job) that needs to write — comment on a PR, push a commit, create
a release, publish a package — must **declare that scope explicitly** in a `permissions:`
block. A workflow that needs write and forgets fails **closed and loud** (a `403` on first
run), never silently holding a scope it didn't ask for. Declare permissions at the *job*
level when only some jobs need write.

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

- **Single trunk name.** `main` auto-mirrors `dev` (`.github/workflows/mirror-main.yml`),
  so divergence can no longer happen silently; the fully-tidy end state is one trunk
  (retire `main`, or rename the trunk to `main` and retire `dev`) — deferred as churn
  that isn't worth it right now.
- **Default workflow token `write` → `read`.** Recommended hardening, but blocked on
  first giving the ~30 workflows that currently rely on the default an explicit
  `permissions:` block, so the flip doesn't silently break the ones that need write.
