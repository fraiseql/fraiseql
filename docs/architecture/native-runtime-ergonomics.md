# Native runtime — ergonomics from migrating the beta workload

This note captures what migrating a real beta workload off the Python/FastAPI
sidecar onto the in-process TypeScript runtime taught us about the host surface —
what fit, what chafed, and what the follow-up phase should close. It is the
design-in-the-open output of the beta migration. The workloads migrated:

| Workload | Trigger | Path | Example |
|----------|---------|------|---------|
| LLM scoring + next-action | `after:mutation:Deal:update` | fire-and-forget (re-runnable) | `examples/native-functions/deal-scoring.ts` |
| Qonto sync | `after:mutation:Invoice:*` | durable (retry + DLQ) | `examples/native-functions/qonto-sync.ts` |
| Per-user follow-up send | `after:mutation:Deal:update` | fire-and-forget (best-effort; see §4) | `examples/native-functions/follow-up-email.ts` |
| Reply-awareness | `after:ingest:email` | durable | `examples/native-functions/reply-awareness.ts` |

## What fit well

- **Secrets via `env_var`.** Reading an allowlisted API key (`LLM_API_KEY`,
  `QONTO_API_KEY`, `MAIL_API_KEY`) is a one-liner, and deny-by-default keeps a
  function from reaching a secret it was not granted. No change wanted.
- **Outbound HTTP via `http_request`.** The SSRF allowlist
  (`FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`), disabled redirects, and DNS-rebinding
  checks are exactly the guardrails the sidecar hand-rolled. Calling an LLM, the
  Qonto API, and a mail provider all go through the same op.
- **Write-back via `query`.** Persisting a result (a score + next action, a Qonto
  reference) with a GraphQL mutation keeps the function inside the same
  authorization and validation surface as any other client. Idempotent mutations
  (`source: "ai"` losing to a human edit; `recordQontoReference` being a no-op the
  second time) make at-least-once dispatch safe.
- **The durable path.** For the money workload, marking the function
  `re_runnable = false` and letting the dispatcher own retry/backoff/DLQ meant the
  function itself is just "call the API, fail loud on non-2xx" — no bespoke retry
  loop. The guest signals *transient vs permanent* purely by whether it throws,
  and the `!is_client_error` classifier does the rest.
- **The classification gate.** Reply-awareness collapsed to a single
  `classification === "human"` check because the shared normalization layer
  already did the hard part (MIME, threading, bounce/OOO/challenge detection).
  That gate is simultaneously the reply signal and the mail-loop guard.

## Friction and gaps → hardening backlog

These are **known limitations of the opt-in native runtime, not blockers.** Every
workload above runs today; each item below is an ergonomics or reach improvement
slated for a follow-up hardening train, at the end of which the features are
promoted from opt-in to stable.

1. **TypeScript is transpiled-in-name-only (the biggest papercut).** The runtime
   executes JavaScript; a `.ts` file with type annotations is a `SyntaxError`. All
   four examples are written in the type-annotation-free subset of TypeScript
   (valid JS *and* valid TS). This is liveable but surprising, and it blocks
   sharing types with the rest of a TS codebase. **Planned: wire real
   type-stripping** (`deno_ast` / swc) so authors write ordinary TypeScript.

2. **Per-user send — DELIVERED as a host op.** The banked constraint — a paired
   outbound email is sent *from the connected user's verified address, never a
   shared mailbox* — is now enforced structurally by the `send_email` host op: the
   guest supplies only `to`/`subject`/body, and the host injects the `from` from
   the resolved sender identity (the #539 seam — `LoginEmailSender` by default, a
   DB-backed resolver where the sending mailbox differs). A guest-supplied `from`
   is dropped at the type level. The transport is per-connected-account SMTP
   (`[mailbox.<name>.smtp]`, STARTTLS, server-side secrets keyed by mailbox,
   selected by the verified sending address), with a send-warming daily cap
   (10/day → 200/day → unlimited). Failures classify onto durable dispatch: a
   permanent refusal (denied identity, bad recipient, SMTP 5xx, over-cap) is a 4xx
   → dead-letter; a transient one (SMTP timeout/greylist, identity store down) is a
   5xx → retry (effective end-to-end via permanent-error tagging, gap #5).
   `follow-up-email.ts` now calls the op instead of a hand-rolled send. The op runs
   in the stock server binary: the server loads function modules from the compiled
   schema's `module_dir` and mounts the after:mutation dispatch hooks at serve time
   (a missing module fails startup, fail-loud). **Not yet closed:** the DB-backed
   `SendCounter` over the application's mailbox table (the remaining warming piece).

3. **Verified sending address vs. authenticated email.** Today the per-user
   `from` is taken from the authenticated identity's `email`. An outreach tool's
   *sending* mailbox (the connected IMAP/SMTP account, cf. `[mailbox.<name>]`) can
   differ from the JWT subject's email. **Planned: a distinct, verified
   `sending_address` on the security/auth context**, resolved from the connected
   mailbox rather than assumed equal to the login email.

4. **Host-provided idempotency token — DELIVERED.** The guest no longer has to
   hand-derive a deterministic key. The host exposes a per-dispatch idempotency
   token — `Deno.core.ops.fraiseql_idempotency_token()` (WASM:
   `get-idempotency-token`) — that the durable dispatcher derives once from the
   dispatch's stable identity (source + function + trigger + payload data; never
   wall-clock/random) and injects into every retry attempt. So it is **stable
   across retries of the same dispatch and across a resume**, and **distinct per
   logical operation**. The guest passes it straight to a downstream money/mail
   idempotency header, so an at-least-once dispatch stays at-most-once. It is 32
   lowercase hex characters — URL-safe and short enough for a VERP email local
   part (`bounces+<token>@…`), which the delivery-feedback work reuses as the
   per-send correlation id. `qonto-sync.ts` now prefers it, falling back to the
   invoice-derived key only on a non-dispatched invocation (`null` token).
   **Trade-off (both valid):** the host token dedups retries/redeliveries of one
   dispatch; a content-addressed key (`qonto-invoice-${id}`) additionally dedups
   across *different* dispatches touching the same entity — use the latter where
   cross-dispatch money dedup matters. The dead-letter record also carries the
   token for operator inspection/replay.

5. **Permanent-error tagging — DELIVERED.** A function can now say "this failure is
   permanent, do not retry": a guest throws a tagged error
   (`Object.assign(new Error(msg), { fraiseqlPermanent: true })`, or a message
   carrying the `[fraiseql:permanent]` marker), and the runtime maps it to a 4xx
   `FraiseQLError` — which durable dispatch dead-letters on the first attempt rather
   than exhausting retries. Host ops auto-tag: any op that returns a 4xx (client)
   error is permanent by default, so a `send_email` refusal (denied identity, bad
   recipient, SMTP 5xx, over-cap) dead-letters immediately, while a transient one
   (timeout, greylist, identity store down) still retries. Untagged errors are
   unchanged (transient / 501). This makes the op's permanent/transient split
   effective end-to-end across the guest boundary.

6. **Testing: one V8 isolate per process.** Two Deno invocations in a single test
   process abort (V8). Each workload test therefore does exactly one invocation
   and nextest runs one test per process. Determinism claims (e.g. idempotency-key
   stability) are proven by asserting the pure-function output of a single run
   rather than diffing two runs. Worth documenting for anyone adding tests; not a
   runtime problem.

## Bottom line

The host surface — **secrets, HTTP, write-back, auth context, durable dispatch**
— was sufficient to move every sidecar responsibility except the *send transport*
into native TypeScript, with reply-awareness proven end-to-end against a fixture
mailbox. The two changes that would most improve the authoring experience are real
TypeScript transpilation (1) and a first-class per-user `send_email` op with a
concrete transport (2). Both lead the hardening backlog.
