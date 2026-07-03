# Native runtime — ergonomics from migrating the beta workload

This note captures what migrating a real beta workload off the Python/FastAPI
sidecar onto the in-process TypeScript runtime taught us about the host surface —
what fit, what chafed, and what the follow-up phase should close. It is the
design-in-the-open output of the beta migration (native-runtime-migration Phase
05, Cycle 4). The workloads migrated:

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
- **The durable path (Phase 02).** For the money workload, marking the function
  `re_runnable = false` and letting the dispatcher own retry/backoff/DLQ meant the
  function itself is just "call the API, fail loud on non-2xx" — no bespoke retry
  loop. The guest signals *transient vs permanent* purely by whether it throws,
  and the `!is_client_error` classifier does the rest.
- **The classification gate (Phase 04).** Reply-awareness collapsed to a single
  `classification === "human"` check because the shared normalization layer
  already did the hard part (MIME, threading, bounce/OOO/challenge detection).
  That gate is simultaneously the reply signal and the mail-loop guard.

## Friction and gaps → Phase 06 backlog

1. **TypeScript is transpiled-in-name-only (the biggest papercut).** The runtime
   executes JavaScript; a `.ts` file with type annotations is a `SyntaxError`. All
   four examples are written in the type-annotation-free subset of TypeScript
   (valid JS *and* valid TS). This is liveable but surprising, and it blocks
   sharing types with the rest of a TS codebase. **Phase 06: wire real
   type-stripping** (`deno_ast` / swc) so authors write ordinary TypeScript.

2. **Per-user send is a policy + reference pattern, not yet a host op.** The
   banked constraint — a paired outbound email is sent *from the connected user's
   verified address, never a shared mailbox* — is enforced today by
   `fraiseql_functions::outbound::resolve_sender_identity` (a pure, fail-loud
   policy) plus surfacing the verified `email` in `auth_context`, and the
   `follow-up-email.ts` reference mirrors it in TypeScript. That makes the rule
   real and tested, but the `from` still lives in guest code. **Phase 06: a
   first-class `send_email` host op that injects the bound `from` (structural,
   guest cannot override) over a concrete SMTP / provider transport**, reusing the
   `resolve_sender_identity` policy. The transport is the missing piece — mirror
   the `sql_query` fail-loud-until-wired stance until it lands.

3. **Verified sending address vs. authenticated email.** Today the per-user
   `from` is taken from the authenticated identity's `email`. An outreach tool's
   *sending* mailbox (the connected IMAP/SMTP account, cf. `[imap.<name>]`) can
   differ from the JWT subject's email. **Phase 06: a distinct, verified
   `sending_address` on the security/auth context**, resolved from the connected
   mailbox rather than assumed equal to the login email.

4. **Idempotency keys are author-managed.** The money path is safe only because
   the author derived a deterministic key (`qonto-invoice-${id}`) by hand. Nothing
   in the host nudges them toward it, and a random key would silently double-spend
   on retry. This is also what keeps the per-user follow-up send on the
   *fire-and-forget* path for now: without a stable send-idempotency token, a
   durable retry could double-send, and a fire-and-forget failure is simply lost —
   neither is right for user-facing mail. **Phase 06: a host-provided,
   per-dispatch-stable idempotency token** (stable across retries of the same
   dispatch, distinct per logical operation) the guest passes straight through to
   a downstream money or mail API, so paired sends can move to the durable path
   safely.

5. **Error model is throw-or-return.** Transient vs permanent is inferred from the
   resulting `FraiseQLError` classification, which is the right default, but a
   function cannot yet *say* "this is permanent, do not retry" (e.g. a validation
   failure that happens to surface as a 5xx). **Phase 06: let a function tag a
   thrown error as permanent** so it dead-letters immediately instead of
   exhausting retries.

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
concrete transport (2). Both are Phase 06.
