# FraiseQL `.phases/`

Phase plans for this repository, in the phased-TDD format described in
`~/.claude/CLAUDE.md`.

**This file is the only tracked one in `.phases/`** — the directory is gitignored, so the
plans live on the machine that wrote them and only the code they produce reaches git history.
That is why this index is worth keeping accurate: it is the sole in-repo record of what is
running.

---

## ⭐ Active — v2.15.0 Release Program (2026-08-16)

[2026-08-16-v2.15.0-release/](2026-08-16-v2.15.0-release/) — take `dev` from `c1608e6cd` to a
tagged, published release, and nothing else. Ten phases: two blockers in the *release record*
(a changelog that omits 48 closed issues including a whole security wave; a published
dependency-risk policy whose justification is false against the tree), three optional HIGH
fixes, the version decision, and the cut.

Two decisions belonged to the human and were asked in
[P00](2026-08-16-v2.15.0-release/phase-00-baseline-decisions-and-filings.md). Both are now
answered, and recorded there: **V — 2.15.0**, with the changelog's SemVer-adherence sentence
reworded in P08 so the release ships one claim rather than two against 158 `### Breaking`
bullets; **H — all three HIGHs hold**, so P05 (#1080), P06 (#1079) and P07 (#1068) all run
and the release notes carry no HIGH as a known issue. Start at its
[README](2026-08-16-v2.15.0-release/README.md).

P00 also filed the eight findings the readiness pass had surfaced but deliberately not fixed,
as **#1127–#1134**.

Supporting documents at this level:

- [RELEASE-READINESS-2.15.0.md](RELEASE-READINESS-2.15.0.md) — the 2026-08-16 verification
  pass on `c1608e6cd`: what is green, what the blockers are, what was deliberately not run.
  **Plan from it; do not redo it.**
- [BACKLOG-2026-08-16.md](BACKLOG-2026-08-16.md) — the 101 open issues, the traps, the gate
  list, the rig. Current.
- [NEXT-AGENT-PROMPT.md](NEXT-AGENT-PROMPT.md) — the live handoff.

## Not in the release program — decided separately

- **The 101-issue backlog** (48 audit pass 4 · 47 the last program's rule-6 residue ·
  6 deferrals). Needs a triage decision, not a phase. Analysis in
  [BACKLOG-2026-08-16.md](BACKLOG-2026-08-16.md).
- **G5 — schema-intelligence epic #963/#965**, cross-repo, prerequisite #995.
- **Deferrals #428 #444 #626 #633**, dispositions commented on each issue.

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
