# What the June 2026 audit found, and what we did about it

On 2026-06-11 we ran a full audit of the FraiseQL workspace: the Rust crates, the SDKs and
the deployment artifacts. We publish the report and the status of every finding because a
security policy that describes only the good news is not one anyone should rely on. The
report is tracked verbatim at [2026-06-11.md](2026-06-11.md); the ledger that gives each
finding a status and a proof is [2026-06-11-status.md](2026-06-11-status.md). A gate in the
pre-merge checks keeps that ledger complete against the report, so this page cannot quietly
outlive the facts.

## What it found

One critical, forty-six high and fifty-six medium findings, plus lower-severity code-quality,
dependency and coverage notes. The four we consider to have been exploitable:

- **C1** — SQL injection in the MySQL stored-procedure mutation path: quote-only escaping,
  breakable with a backslash, reachable from GraphQL mutation arguments.
- **H1** — a GROUP BY alias taken from GraphQL variables and interpolated unvalidated.
- **H2** — Relay `node(id:)` resolved without the caller's security context: no row-level
  policy, no injected parameters, no role check.
- **H3** — federation `_entities` key values escaped for single quotes only.

Beyond those, the pattern that worried us most was not any single injection. It was
**controls that existed on paper and did nothing**: a token revocation store that was never
read, error sanitization not mounted on REST, an observer action layer whose SMS, push and
search actions reported success and sent nothing, a saga orchestrator that fabricated
success, SDK `config()` calls that were no-ops, and an SDK publishing pipeline frozen at one
version for several releases.

## What shipped, and when

- **v2.7.0, 2026-06-13** — two days after the report: the security wave. C1, H1, H2 and H3,
  each with a test that asserts the audit's own payload is refused; the revocation store
  consulted on every authenticated route (H8); REST errors sanitized (H7); the dead audit
  logger removed (H13); a schema that asks for field encryption refused at boot (H12).
- **v2.8.0, 2026-06-18** — the correctness train: unsupported observer actions rejected at
  config load instead of pretending (H24); saga execution made to fail loudly and then made
  real (H32); the SDK no-ops deleted (H29); the hidden dev server deleted (H23); SDK
  publishing repaired, with a release that refuses to publish a manifest whose version does
  not match the tag and a post-publish check that fails when a registry never shows the
  version (H30).
- Later releases carried the rest. As of 2026-09-21 the ledger reads: 92 fixed, 6
  unreachable, 4 deleted with their feature, 1 accepted, none unaddressed.

## What we deleted rather than fixed

Some findings lived in code that had no business shipping. The MySQL, SQLite and SQL Server
backends, where the critical finding lived, were fixed in v2.7.0 and then removed altogether
in the 2.15.0 line: FraiseQL is PostgreSQL only. The secrets-side audit logger, the hidden
dev server and the SDK `config()` no-ops went the same way. Deleting a feature is not a fix
to be proud of, but it is honest: what does not exist cannot be advertised.

## What refuses to run

Field-level encryption was documented as shipped and its write half was never wired. Rather
than ship half of it, the server now **refuses to start** when a schema marks a field for
encryption. The four medium findings about its key handling are therefore unreachable, and
the ledger says exactly that rather than "fixed". Field encryption is not on the roadmap. If
it returns, it returns with a design, a test that fails without it, and the refusal removed
in the same change, not as a flag that turns something half-built on.

## What remains open

The audit was not the last word. Two further passes in July filed sixty-five more findings
as issues, and the backlog program that followed has been closing them; the next release is
tagged when the open backlog is closed. Four of the open issues are security findings from
those later passes, #1351, #1353, #1354 and #1359, and they close with the rest before the
tag. Real transports for the observer actions that are rejected today are #428. One accepted
risk remains, the `.trivyignore` list, on a documented review cadence. The tracker, the
changelog and the ledger are the record of when each of these closes; this page is not.

## What changed in how we work

- Every claim in a document is now something a gate can check: the changelog cites findings
  by ID, the ledger must cover the audit, documents may not name backends or files the tree
  does not have, and the post-publish package validations must be able to fail.
- Every fix is revert-checked alone: the test that guards it must fail with the fix removed.
- The dependency-advisory policy is one document, kept in lockstep with the tooling by a
  gate, after we caught our own security policy describing an accepted RSA advisory as
  unreachable MySQL-only code when the real path was the JWT library in the default build.
- The audit's worst findings were not the injections. They were controls that existed in
  documentation, behind tests that could not fail. If we could tell another maintainer one
  thing: write the test that fails first, and make every sentence in your documents something
  a machine can check against the tree.

## Where to check

- The audit: [2026-06-11.md](2026-06-11.md)
- Every finding's status and proof: [2026-06-11-status.md](2026-06-11-status.md)
- The gates: `make preflight`, listed in the repository `Makefile`
- To report a vulnerability: [SECURITY.md](../../../SECURITY.md)
