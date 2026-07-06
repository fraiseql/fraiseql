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

## Friction and gaps → hardening backlog (all DELIVERED)

The native runtime is now **stable and semver-covered.** It remains **opt-in**
behind its Cargo features (`functions-runtime`, `functions-runtime-deno`,
`inbound`, `inbound-email`) so the default binary stays lean — V8 (~30 MB) is only
compiled when `functions-runtime-deno` is enabled — but the feature surface is
stable, not experimental. Every gap the beta migration surfaced (below) is
**delivered**; this section is retained as the delivery record.

> **Scope note (delivery-feedback).** The delivery-feedback surfaces — per-send VERP
> Return-Path correlation, the suppression-store schema, the
> `POST /api/email/suppress[ion]` admin API, and the send-status lifecycle — landed
> immediately before the promotion and have not yet met a real bounce, a provider's
> actual plus-addressing, or a live challenge-response. They are stable-*tracked* but
> may evolve through v2.12 as the first beta feedback lands; treat their exact shapes
> as provisional-within-stable until then.

1. **TypeScript type-stripping — DELIVERED.** The runtime strips `TypeScript`
   types to executable JavaScript before execution (`deno_ast` / swc, a real AST
   transpile — interfaces, `: Type`, generics, `as`, and `enum`s are all handled),
   gated by `DenoConfig.enable_typescript` (on by default). Authors write ordinary
   `.ts` and can share types with the rest of a TS codebase; a host-op `.d.ts`
   (`examples/native-functions/fraiseql-host.d.ts`) types the `Deno.core.ops.fraiseql_*`
   surface, and `examples/native-functions/deal-scoring.ts` is the annotated
   reference. A parse/transpile failure surfaces as a located `SyntaxError`.

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

3. **Verified sending address vs. authenticated email — DELIVERED.** An outreach
   tool's *sending* mailbox (the connected IMAP/SMTP account, cf.
   `[mailbox.<name>]`) can differ from the JWT subject's email, so the `send_email`
   `from` is **not** assumed equal to the login email. It is resolved at send time
   by the `SenderIdentityResolver` seam — the send-side arm of the shared
   `sub → DB → identity` primitive (sibling of the enriched-identity read scoping,
   #539): `DbSenderIdentityResolver` maps `sub → verified from-address` from the
   application's DB, **fail-closed** (a denied subject or a NULL address refuses the
   send; a momentarily-down store is a transient 503, never a fall-back to a shared
   mailbox). `LoginEmailSender` is the degenerate default where the two provably
   coincide (no `[identity.sender]` configured). Ownership is two-sided: the SMTP
   transport only relays for an address with a configured per-account
   `[mailbox.<name>.smtp]`, so a resolved address that no account owns cannot send.

   Resolved **at send time**, not eagerly surfaced on every `auth_context`: the
   `from` is host-owned and a guest cannot set it (a guest-supplied `from` is
   dropped at the type level), so there is nothing for a guest to do with a
   `sending_address` field — and eager resolution would add a per-invocation DB hit
   to functions that never send. `auth_context` still carries the login `email` /
   `display_name` unchanged. **Non-PostgreSQL note:** the DB-backed resolver runs on
   the PostgreSQL-only identity primitive; MySQL/SQLite deployments dispatch
   functions with the `LoginEmailSender` default (`from` = the authenticated
   `email`).

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

## Delivery feedback loop (SMTP `2xx` ≠ delivered)

A `send_email` that returns success means the relay *accepted* the message — **not
that it was delivered**. The real outcome arrives later, *inbound*: a hard bounce, a
greylist tempfail-then-accept, or a challenge-response prompt (Mailinblack, Boxbe)
that holds the message until the sender passes a challenge. The delivery-feedback
loop closes that gap by correlating those inbound events back to the send that
triggered them.

- **VERP Return-Path correlation.** Each send sets the SMTP envelope sender
  (`MAIL FROM`) to `bounces+<send-id>@<domain>` while the header `From` stays the
  verified sending address. A bounce/challenge is addressed to the Return-Path, so it
  lands at `bounces+<send-id>@…` and the poll-IMAP adapter recovers the `<send-id>`
  from the recipient plus-tag (falling back to our sent `Message-ID` quoted in the
  reply's `References`). The `<send-id>` is the per-dispatch **HMAC** idempotency
  token — deterministic and resume-stable, but *unforgeable*, so a forged
  `bounces+<token>@…` cannot poison another send's status. Correlation is a built-in
  Rust step in the poll worker (platform infrastructure); app `after:ingest`
  functions still fire afterwards for app logic.

- **Send-status lifecycle — no lying `Delivered`.** A send is recorded `Sent` on
  relay and only transitions on an inbound signal: `Bounced`, `ChallengePending`,
  `Replied`. Delivery is *not* positively observable, so there is **no** `Delivered`
  transition — "no news ≈ delivered". A monitoring view may *infer*
  delivered-after-a-window, but the platform never fabricates the state.

- **Suppression list + GDPR.** A do-not-contact list is checked *before every send*
  (a suppressed recipient is a permanent refusal — the biggest deliverability lever).
  It stores a **keyed HMAC hash of the address, never the raw address**, so a GDPR
  erasure of the recipient's PII elsewhere leaves the "do not contact" fact intact.
  A hard bounce suppresses immediately (permanent); repeated unanswered challenges
  suppress with a ~30-day TTL; a genuine reply lifts a challenge suppression at once.
  Both the send-status and suppression tables carry an explicit `tenant_id` column
  and are RLS-scoped for app-facing reads.

- **Challenge policy (a hard product boundary).** Detect + correlate + **surface** +
  suppress-after-N (`[send] challenge_suppress_after`, default 2, per-recipient
  across campaigns, event-based). Challenges are **never auto-solved** for
  cold/unsolicited outreach — it circumvents the recipient's anti-spam control,
  torches sender-domain reputation, and (Mailinblack being a French product) the
  recipients are under GDPR. A surfaced `ChallengePending` is a human decision point:
  a salesperson personally solves it, or the recipient releases us.

- **Exactly-once send.** Because the send-id is per-dispatch-stable, a durable retry
  of an already-sent dispatch is detected and **skipped** (the recorded response is
  returned), so a transient failure after the relay already accepted cannot
  double-send.

- **Greylisting.** A transient SMTP failure carries a mail-appropriate backoff floor
  (minutes, not the policy's seconds) that the durable dispatcher honors, so a
  greylist tempfail is retried after the greylist window rather than exhausting fast
  retries into the DLQ.

- **Return-Path probe.** Plus-addressing is provider-dependent; a provider that
  strips the `+<send-id>` tag makes every bounce vanish and every send look
  delivered. The opt-in startup probe (`[send] verp_probe_on_start`) sends a
  self-addressed `bounces+probe-<nonce>@…` and confirms it lands with the tag intact,
  turning that silent, deployment-dependent failure into a loud, diagnosable one.

- **IMAP safety.** Bounce/challenge processing is **read-and-move only** — the poll
  adapter `BODY.PEEK`s (no `\Seen`) and nothing flags-deleted + expunges. An IMAP
  mailbox can be the only copy of irreplaceable data; the loop never destroys it.

- **App surface.** Send status is read directly from `_fraiseql_send_status` under
  RLS (keyed by the non-secret send-id — no server key needed). Suppression append +
  query go through the admin API (`POST /api/email/suppress`,
  `POST /api/email/suppression`, bearer-gated) because the address must be hashed
  server-side with the server HMAC key before it touches the store. Both are `POST`
  with the address in the body (never a query string), so the raw address is never
  captured by access logs or proxies.

**Configuration.** `[server] hmac_secret_env` names the env var holding the root
secret — VERP status-tracking activates only when it is set (fail-closed: no secret →
plain token, plain Return-Path, no correlation). `[mailbox.<name>.smtp.return_path]`
overrides the local part / domain (default `bounces`@the sending domain; a mismatched
domain warns, since the envelope sender is the SPF/DMARC alignment target). `[send]`
carries `challenge_suppress_after` and `verp_probe_on_start`.

## Bottom line

The host surface — **secrets, HTTP, write-back, auth context, durable dispatch**
— was sufficient to move every sidecar responsibility except the *send transport*
into native TypeScript, with reply-awareness proven end-to-end against a fixture
mailbox. The two changes that would most improve the authoring experience are real
TypeScript transpilation (1) and a first-class per-user `send_email` op with a
concrete transport (2). Both lead the hardening backlog.
