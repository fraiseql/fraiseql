# FraiseQL Remediation Plans

Internal quality and security improvement tracking. Not shipped to users.

## Start here

**[master-plan.md](./master-plan.md)** — All ~121 issues organized into 8 actionable
batches with severity ratings, file locations, and milestone assignments.
Read this before touching anything in `extensions/`.

## Batch priority order

| Batch | Topic | Gate |
|-------|-------|------|
| **0** | Process bootstrap (GitHub Issues, gitignore) | Do first |
| **1** | Critical security (auth bypass, SQL injection, webhook crypto) | No release before complete |
| **2** | Silent wrong results (date, Vault, operators, state machine) | After batch 1B |
| **3** | Feature theater (APIs that do nothing) | Implement or document as stub |
| **4** | Validation subsystem (one coherent branch) | — |
| **5** | Architecture decisions (need spec issues first) | — |
| **6** | Reliability / resource leaks | Parallel with 4–5 |
| **7** | Proc-macro and CI | Parallel with anything |
| **8** | Docs, tests, hygiene | Evergreen |

## Extension index

Each extension is the detailed finding that feeds a batch in the master plan.

| File | Scope |
|------|-------|
| [ext-00-base.md](./extensions/ext-00-base.md) | Docs accuracy, coverage gates, process |
| [ext-01.md](./extensions/ext-01.md) | Auth bypass (GET handler, RBAC), backup stubs, CSP |
| [ext-02.md](./extensions/ext-02.md) | SQL injection (window/where), hardcoded date, cache no-ops |
| [ext-03.md](./extensions/ext-03.md) | Observer subsystem (health monitor, Jaeger, export stubs) |
| [ext-04.md](./extensions/ext-04.md) | Webhook protocol errors (Twilio, SendGrid, Paddle, replay) |
| [ext-05.md](./extensions/ext-05.md) | JWT audience, LIKE metacharacters, NATS dead-letter |
| [ext-06.md](./extensions/ext-06.md) | generate-views placeholder, OIDC audience, cascade cycles |
| [ext-07.md](./extensions/ext-07.md) | Token refresh stubs, rate-limiter clock failure, W3C trace |
| [ext-08.md](./extensions/ext-08.md) | Arrow Flight SQL injection (3 vectors), OAuth2 PKCE/CSRF |
| [ext-09.md](./extensions/ext-09.md) | proc-macro `.await` unsoundness, CI supply-chain, SDK CI |
| [ext-10.md](./extensions/ext-10.md) | MCP GraphQL injection, trace ID collisions, RBAC masking |
| [ext-11.md](./extensions/ext-11.md) | MCP auth no-op, escape_identifier stub, schema versioning |
| [ext-12.md](./extensions/ext-12.md) | Webhook replay, rate-limiter memory leak, SCRAM panic |
| [ext-13.md](./extensions/ext-13.md) | Vault: no pooling, no timeout, stale cache on rotate |
| [ext-14.md](./extensions/ext-14.md) | rustls CVE, validation test gaps, duplicate function names |
| [ext-15.md](./extensions/ext-15.md) | Tenant SQL injection, file audit atomicity, tracking gap |
| [ext-16.md](./extensions/ext-16.md) | Design API fail-open, field encryption wiring, config gaps |
| [ext-17.md](./extensions/ext-17.md) | Complexity counter, depth bypass, introspection type kinds |
| [ext-18.md](./extensions/ext-18.md) | IN () empty list, LIKE escaping, `#[non_exhaustive]` |
| [ext-19.md](./extensions/ext-19.md) | Observer checkpoint cast, health monitor leak, W3C trace |
| [ext-20.md](./extensions/ext-20.md) | Facade CLI dep, email/phone validator pipeline gap |

## Workflow

1. Pick the lowest-numbered incomplete batch from `master-plan.md`
2. Open the referenced `ext-NN.md` for full context on each issue
3. Fix, test (`cargo nextest run -p <crate>`), clippy clean
4. Open a GitHub Issue per item (see Batch 0 — EE1)
5. Close the issue in the PR that fixes it

## Status tracking

Mark issues done by strikethrough in `master-plan.md` or link to the fixing PR.
When all issues in a batch are resolved, note the date next to the batch header.

> This directory is intentionally not in `.gitignore` so the plans survive
> across sessions. Add `.remediation/` to `.gitignore` only if you move
> tracking to GitHub Issues (Batch 0).
