# FraiseQL `.phases/`

Phase plans for this repository, in the phased-TDD format described in
`~/.claude/CLAUDE.md`.

**This file is the only tracked one in `.phases/`** — the directory is gitignored, so the
plans live on the machine that wrote them and only the code they produce reaches git history.
That is why this index is worth keeping accurate: it is the sole in-repo record of what is
running.

Programs run in their own worktree, so a program's plan is on disk **only in that worktree**.
The path is given with each entry below; a copy of this index in another worktree will not
have those files next to it.

---

## ⭐ Active — Make the delivery artifact a gated object (2026-08-26)

`2026-08-26-delivery-artifact-gating/`, in the program worktree —
**`/home/lionel/code/fraiseql-phase06`** (branch `fix/phase01-workflow-reachability`).

Every gate in this repository checks a property of the **source**; almost nothing checks the
**thing we ship**. Phase 10 of the backlog program found the two worst consequences within a
week of each other — the release image could not be built at all (#1205) and
`cargo deny check licenses` was red (#1204) — both invisible to all eleven legs, because **no
workflow builds the release Dockerfile before the tag**. Eight phases: gate trust, then a
Dagger image build, then boot-and-query, artifact properties, the chart, one Compose stack,
post-publish crate installability, and a delivery manifest.

It adds **no product surface**. Every phase must cover something already shipped, or be cut.

**Status: Phase 01 complete** (`1e241c721`, not pushed). **Next: Phase 02 — the substrate
builds the image, before the tag.** The live handoff is
`2026-08-26-delivery-artifact-gating/NEXT-AGENT-PROMPT.md` in the program worktree.

⚠ Phase 01 opened **#1208 — no secret scanner runs anywhere in CI.** The TruffleHog job that
`.dagger/security.go` and `dagger-security.yml` both defer to as authoritative was gated on a
`pull_request` its workflow has not received since 2026-05-31. The unreachable job is deleted;
the hole is a founder decision. Also open from that phase: **#1207** (17 step-level conditions
of the same class), **#1209**, **#1210**.

## Deferred behind the above — Fix the whole backlog before v2.15.0 (2026-08-22)

`2026-08-22-pre-2.15.0-backlog/`, in the same worktree.

**Decision of record (2026-08-22):** the founder chose to burn the **entire open backlog down
to zero before the v2.15.0 tag**, having been shown that it contains multi-week epics and that
the release record is already honest about its gaps. **The tag is deferred until this program
closes.** Do not relitigate.

110 open issues at `4388c7130`, in 14 phases, ordered by blast radius of being wrong: gate
integrity first (a gate that cannot fail turns every later "green" into an unverified claim),
then runtime correctness, surfaces, examples, docs; epics last.

**Status: phases 01–10 complete**, plus #1169 out-of-phase. **Next: Phase 11 — storage**,
deferred behind the delivery-artifact program above.

- **The live handoff is
  `2026-08-22-pre-2.15.0-backlog/NEXT-AGENT-PROMPT.md`** in the program worktree.
- Per-issue detail, founder decisions and each phase's verification record are in that
  directory's `README.md` (newest-first).
- ⚠ "Closed" in that plan means fixed-and-committed on the program branch, **not** closed on
  GitHub. `Closes #N` fires only on merge to the default branch, so fixed issues still reading
  OPEN are correct — do not close them by hand.

## Paused — v2.15.0 Release Program (2026-08-16)

[2026-08-16-v2.15.0-release/](2026-08-16-v2.15.0-release/) — take `dev` to a tagged, published
release. P00–P08 are merged; only [P09, the cut](2026-08-16-v2.15.0-release/phase-09-release-execution.md)
remains, and it is **held** by the 2026-08-22 decision above until the backlog program closes,
which is in turn behind the 2026-08-26 program.

Its two human decisions are answered and recorded in
[P00](2026-08-16-v2.15.0-release/phase-00-baseline-decisions-and-filings.md): **V — 2.15.0**,
with the changelog's SemVer-adherence sentence reworded in P08 so the release ships one claim
rather than two against 158 `### Breaking` bullets; **H — all three HIGHs hold**, so P05
(#1080), P06 (#1079) and P07 (#1068) all ran and the release notes carry no HIGH as a known
issue. P00 also filed the eight findings the readiness pass had surfaced but deliberately not
fixed, as **#1127–#1134**.

⚠ **`NEXT-AGENT-PROMPT.md` at this level is that program's handoff, written 2026-08-17, and is
STALE** — it says the only remaining work is the cut. It was superseded on 2026-08-22. Use the
active program's handoff above.

Supporting documents at this level:

- [RELEASE-READINESS-2.15.0.md](RELEASE-READINESS-2.15.0.md) — the 2026-08-16 verification
  pass on `c1608e6cd`: what is green, what the blockers are, what was deliberately not run.
  **Plan from it; do not redo it.**
- [BACKLOG-2026-08-16.md](BACKLOG-2026-08-16.md) — the 101 open issues as they stood on
  2026-08-16, with the traps, the gate list and the rig. Superseded as a *plan* by the
  2026-08-22 program (which counts 110 at `4388c7130`); still current as background.

## Not in either program — decided separately

- **[2026-08-20-document-validation-gaps/](2026-08-20-document-validation-gaps/) — the #1154
  family.** Nine phases. #1154 closed one door (an undeclared *argument* is now refused);
  re-probing found the same silent-discard failure mode through six more: unvalidated variable
  definitions and references (§5.8.2/§5.8.3/§5.8.4), unvalidated `where` field names,
  unvalidated `orderBy` field names on both the list and window paths (#1014), case-sensitive
  `ID` equality, and introspection ignoring the selection set. Written against `0fe785b07`;
  its README §2 records **nine places the source brief disagreed with the tree**. Five human
  decisions gate it, including whether any of it delays the v2.15.0 cut — start at
  [P00](2026-08-20-document-validation-gaps/phase-00-decisions-and-sequencing.md). Lives in
  `/home/lionel/code/fraiseql` and `/home/lionel/code/fraiseql-docval`.
- **G5 — schema-intelligence epic #963/#965**, cross-repo, prerequisite #995. Scheduled inside
  the active program as Phase 14.
- **Deferrals #428 #444 #626 #633**, dispositions commented on each issue. #633 is Phase 11 of
  the active program; #1113 was deferred with #428 in Phase 02.

## Closed programs

| Program | Outcome |
|---|---|
| [_archive/2026-08-06-open-finding-remediation/](_archive/2026-08-06-open-finding-remediation/) | **Closed 2026-08-16.** 86 issues → 22 phases; 80 shut. The 6 remaining are exactly the deliberate deferrals. Its §5 is the wave-by-wave record of what each phase cost. |
| [_archive/2026-07-27-open-issue-remediation/](_archive/2026-07-27-open-issue-remediation/) | **Closed 2026-08-06**, PR #885 merged at `f6f51fa25`. 203 issues → 158 closed on merge + 36 in-phase + 4 deferred. Retrospective: *execution coverage must be a checked artifact*. |

Everything the 2026-05-31 release train and the v2.11–v2.14 trains queued has **shipped**;
those directories are history, not plans. Completed campaigns live in
[_archive/](_archive/) and are never modified after archiving.

## Audit handoffs

- [2026-07-25-audit-pass3/](2026-07-25-audit-pass3/) — passes 1–3 (issues #715–#788), the
  reusable review-workflow script and the dedup issue-map, force-added past the gitignore.
- Pass 4 (2026-08-09, 210 agents, triple adversarial refutation → #1028–#1080) lives at
  `_archive/2026-08-06-open-finding-remediation/audit-pass-4/`. ⚠ It is **gitignored and
  local-only**, and the 53 issue bodies cite that path **without** the `_archive/` segment —
  it moved when the program was archived.

## Convention

- `.phases/<date>-<name>/` — one directory per program, with its own README + phase files.
- `.phases/_archive/<campaign>/` — completed campaigns; never modified after archive.
- One user-owned decision per gate, surfaced in a phase file and answered in writing there.
- A program's handoff is `NEXT-AGENT-PROMPT.md` **inside its own directory**. When a program
  is superseded, say so in this index and in the superseded handoff's first line — a stale
  handoff at the `.phases/` root is what an agent finds first.
