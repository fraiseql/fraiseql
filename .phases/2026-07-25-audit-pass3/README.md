# FraiseQL bug-hunting audit — pass 3 handoff (2026-07-25)

This folder lets a fresh agent in another terminal pick up the **third pass** of the
framework-wide bug-hunting audit. It is self-contained.

## What's here

| File | Purpose |
|---|---|
| `NEXT-AGENT-PROMPT.md` | **Start here.** Drop-in prompt for the pass-3 agent (cold-start, no prior context needed). |
| `pass2-audit-workflow.js` | The pass-2 Workflow script (13 area reviewers → adversarial per-finding verifiers). Reusable template — edit `AREAS`/`DEDUP` for pass 3. |
| `pass2-issue-map.md` | The 50 issues pass 2 filed (#739–#788), with labels + `file:line`. Dedup reference — don't re-report these. |

## State of the audit

- **Pass 1** (2026-07-25): sampling audit, ~95 files → filed issues **#715–#738**.
- **Pass 2** (2026-07-25): 13-reviewer deep audit, ~250 files, 81 agents → 65 confirmed
  findings → filed **#739–#788** (20 HIGH + 22 MEDIUM individual, 3 combined MEDIUM
  #781–#783, 5 per-crate LOW checklists #784–#788).
- **Pass 3**: not started. See `NEXT-AGENT-PROMPT.md`.

Fuller detail (per-crate grades, both-pass issue↔finding maps, the 3 rejected pass-2
findings) lives in the project auto-memory on this machine at
`~/.claude/projects/-home-lionel-code-fraiseql/memory/project_quality_sampling_2026_07_25.md`.
A cross-machine agent can work from this README + the prompt alone.

## The meta-pattern (holds across both passes — steer pass 3 by it)

FraiseQL's Postgres + GraphQL happy path is heavily tested and clean. **Essentially every
real defect is a silent fail-open, fabricated success, or silent drop on a path with no
real-system integration test.** Recurring seams: `.ok()?` / `unwrap_or_default()` on
parse/deserialize; `if let Ok(...)` fallback + `let _ = <result>`; `cfg`-off stub arms that
return Ok/true/allow; hand-built SQL/JSON via `format!`; producer↔consumer serde drift
(`#[serde(default)]` + no `deny_unknown_fields` → renamed/missing key becomes a silent empty
default); configured backend silently downgrading to in-memory on error; and doc claims
("constant-time", "parameterized", "validated", "enforced", "idempotent") the code doesn't
honor.

## Pass-3 priority targets (neither prior pass sampled)

1. **fraiseql-core compiler** (`src/compiler/`, `filters/`, `types/`, aggregate/window) —
   where SQL generation lives; largely unread across all three passes.
2. **fraiseql-db dialect impls** (MySQL/SQLite/SQL Server) — the #1 fail-open zone; go
   deeper than pass-1 #721/#722. Cross-ref #374.
3. **fraiseql-storage** — entirely unsampled (bucket policy routing, signed URLs, uploads).
4. **fraiseql-functions**, **observers runtime**, **cdc-sinks beyond #718**,
   **wire beyond #729**, **secrets KMS/rotation beyond #726/#727**.
5. **The other 9 SDKs** (`sdks/official/{go,java,php,ruby,rust,csharp,dart,elixir,fsharp}`) —
   pass 1 only covered python + typescript; the schema.json↔CLI-compile seam keeps biting
   (#755–#758).
6. **CLI commands** not yet read (doctor/migrate/generate_*/introspect_facts/…) and the full
   **env-override precedence matrix** vs compiled config + SIGUSR1 hot-reload (pass 2 #774).

## How to run pass 3

1. Open a terminal at `/home/lionel/code/fraiseql`, launch a fresh agent.
2. Paste the contents of `NEXT-AGENT-PROMPT.md`. To reuse the multi-agent fan-out, also say
   "use a workflow" (or `ultracode`) and point it at `pass2-audit-workflow.js`.
3. File confirmed findings as issues in the #715–#788 house style; the next free number is
   ~#789.
