# Pass-3 audit — next-agent prompt

Hand this verbatim to a fresh agent in another terminal. It is self-contained (assumes no prior session context). If you have Workflow orchestration and want the same fan-out shape pass 2 used, add the keyword `ultracode` or say "use a workflow" when you hand it over, and adapt `pass2-audit-workflow.js` in this folder.

---

```
Do a THIRD-PASS bug-hunting audit of FraiseQL (~840k lines Rust, 19 crates + 11 SDKs +
docs), at /home/lionel/code/fraiseql (branch dev, repo fraiseql/fraiseql).

CONTEXT — two prior passes already ran; do NOT re-report their findings:
- Pass 1 (2026-07-25): sampling audit, ~95 files, filed issues #715–#738.
- Pass 2 (2026-07-25): 13-reviewer deep audit, ~250 files, filed issues #739–#788.
Before starting, READ memory/project_quality_sampling_2026_07_25.md IN FULL — it has the
per-crate grades, the complete issue↔finding map for both passes, the rejected findings
(do not re-investigate them), and the pass-3 coverage gaps. Then run
`gh issue list --state open --limit 200` and skim titles to avoid duplicates. When you
file, the next free number is around #789.

META-PATTERN (battle-tested across BOTH passes — steer hard by it): FraiseQL's Postgres +
GraphQL happy path is heavily tested and clean. Essentially every real defect is a SILENT
FAIL-OPEN, FABRICATED SUCCESS, or SILENT DROP on a path that has no real-system integration
test. Concretely, the recurring seams are: `.ok()?` / `unwrap_or_default()` on parse or
deserialize results (drops a filter/check → widens results or skips auth); `if let Ok(...)`
fallback chains and `let _ = <result>` (side effect silently didn't happen); stub/`cfg`-off
arms that return Ok/true/allow where the real arm enforces something; hand-built SQL/JSON via
`format!`; producer↔consumer serde drift (`#[serde(default)]` with no `deny_unknown_fields`
turns a renamed/missing key into a silent empty default — this class alone produced #755–#758
and the historical SpecQL rejection); magic-string sentinels; configured-backend that silently
downgrades to in-memory on error; and doc/comment claims ("constant-time", "parameterized",
"validated", "enforced", "idempotent", "atomic") that the code does not honor — verify EVERY
such claim against the code.

PRIORITY TARGETS neither pass sampled (spend the budget here):
1. fraiseql-core COMPILER (largely unsampled, and it's where SQL generation lives):
   src/compiler/ (parser.rs, validator.rs, ir.rs, aggregation.rs, enum_validator.rs,
   window_functions/, window_allowlist.rs, fact_table/), src/filters/, src/types/,
   src/aggregate_* , src/window* , src/relay.rs, src/native_columns.rs, src/partial_period/,
   src/tenancy/, src/http/, src/subscription/ (thin so far). Hunt: filter/where-clause
   generation that drops a predicate on an unrecognized operator; window/aggregate allowlist
   bypass; enum/type validation that fail-opens; IR lowering that loses a field.
2. fraiseql-db DIALECT IMPLEMENTATIONS (the #1 fail-open zone — never faced a real DB;
   cross-ref #374/#721/#722): MySQL/SQLite/SQL Server SQL generation, connection pooling,
   transaction handling, per-dialect type coercion. Go DEEPER than pass 1's #721/#722.
3. fraiseql-storage (ENTIRELY unsampled): bucket policy routing, key-prefix isolation,
   signed-URL generation/verification, image-transform pipeline, upload path traversal.
   Classic fail-open zone (policy bypass, path traversal, signature reuse).
4. fraiseql-functions (unsampled): runtime function execution, V8 isolate lifecycle,
   entry-point dispatch, x-api-key auth (ADR-0018: NOT Bearer — JWT middleware eats Bearer).
5. fraiseql-observers RUNTIME dispatch/transports (the compiled-config seam is already
   #631–#634): retry/at-least-once/dedup logic, action dispatchers. And fraiseql-cdc-sinks
   beyond #718 (ordering, offset handling, delivery guarantees). And fraiseql-wire beyond
   #729 (framing/protocol correctness). And fraiseql-secrets KMS backends + rotation beyond
   #726/#727.
6. THE OTHER 9 SDKs (sdks/official/{go,java,php,ruby,rust,csharp,dart,elixir,fsharp} — pass 1
   only covered python + typescript): each emits schema.json consumed by the CLI compile
   schema. The producer↔consumer seam keeps biting (#755–#758). Diff each SDK's emitted field
   names/shapes against what crates/fraiseql-cli/src/schema/intermediate/ deserializes.
7. fraiseql-cli COMMAND surfaces not yet read: doctor, migrate, generate_views,
   generate_capture_triggers, introspect_facts, cost, analyze, dependency_graph, sbom,
   sources, perf. And fraiseql-server remaining: mcp/, usage/, resilience/, pool/, tls,
   trusted_documents, tenancy/, storage routes, the full REST router.
8. Cross-layer ENV-OVERRIDE MATRIX: pass 2 found #774 (compiled rate-limit shadows CLI/env).
   Do the full precedence audit — for EVERY security/operational key, is the documented
   CLI > env > compiled order actually implemented, and does it survive SIGUSR1 hot-reload?

METHOD (this is what worked — replicate it): fan out parallel reviewers, one per target area
above; each reads its files IN FULL (Read, not grep-skim), traces each value producer→consumer,
confirms the bad path is REACHABLE (not dead/test-only/impossible-cfg), and adversarially
re-checks its top findings before reporting. Then run an independent adversarial verifier per
candidate finding that DEFAULTS TO REFUTING and corrects severity. DYNAMIC VERIFICATION is
required for any "fails at runtime" claim: run targeted `cargo nextest run -p <crate> <module>`,
or boot the server against docker/e2e/schema.compiled.json (see .github/workflows/release-smoke.yml
for the recipe), or run the offending SQL against a live PG (there is a running test PG; pass 2
used `docker-postgres-test-1` / port 5433) — file only what survives a repro or decisive source
read. (If you have Workflow orchestration available and the user opted in, the pass-2 shape was:
`pipeline(AREAS, review→schema, review→parallel(verify per finding))`; ~13 reviewers, ~80 agents.)

CALIBRATION: clippy pedantic+deny, unwrap_used=deny, missing_docs=deny are enforced. Do NOT
report style, naming, missing docs, missing `// Reason:` on #[allow], or unwrap in #[cfg(test)]
code — the lint wall already blocks those. Every finding needs a concrete failure scenario:
specific inputs/state → specific wrong output/crash/leak/contract-violation. Cosmetic worst-case
= low severity at most.

FILING: file confirmed findings as GitHub issues in the house style of #715–#788 (title
"area: summary"; body = ## Summary (with **Severity** + **Location** `file:line`) / ## Details
(file:line refs) / **Failure scenario** / **Repro** if runtime / ## Suggested fix; footer
"_Found in the 2026-07-<date> third-pass audit_"; labels from bug/security/documentation/
enhancement). File HIGH/MEDIUM individually; GROUP low-severity into per-crate checklist issues.
Cross-reference #374 (multi-db parity) and any related #715–#788 issue. When done, append a
pass-3 section to memory/project_quality_sampling_2026_07_25.md with the new issue numbers, a
coverage map, and the remaining gaps — and update the MEMORY.md index line.
```
