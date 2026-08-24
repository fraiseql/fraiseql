# Webhooks Architecture

FraiseQL has two webhook-related subsystems with complementary roles. This document
explains both and how to choose between them.

---

## Inbound vs. Outbound

| Subsystem | Crate | Direction | Purpose |
|-----------|-------|-----------|---------|
| **Webhook Receiver** | `fraiseql-webhooks` | Inbound ← | Receive HTTP callbacks from Stripe, GitHub, Shopify, … |
| **Observer Notifier** | `fraiseql-observers` | Outbound → | Emit notifications when your database rows change |

### When to use `fraiseql-webhooks`

Use it when a **third-party service** needs to push events to you:

- Stripe sends `payment.succeeded` after a charge succeeds
- GitHub sends `push` after a commit is made to a repository
- Shopify sends `order.created` after a customer checks out

### When to use `fraiseql-observers`

Use it when **your own data changes** and you want downstream consumers to know:

- A row is inserted into `orders` → notify fulfilment service
- A `status` column changes to `"shipped"` → send email to customer
- An aggregate crosses a threshold → trigger an alert

---

## Inbound Webhook Receiver (`fraiseql-webhooks`)

> **Status: mounted (opt-in).** Behind the opt-in `inbound` Cargo feature, the receiver
> is mounted as an HTTP route — `POST /webhooks/{segment}` — as the first **push adapter**
> of the inbound-source model (see below). The route verifies the signature via this
> pipeline, normalizes the delivery to an `InboundMessage`, and persists it onto the durable
> inbound spine *inside the receiver transaction*, so persistence is atomic with the
> idempotency claim. Without the feature the whole inbound path is compiled out.

### Inbound as a source

The receiver is one adapter of a general primitive — *an external message becomes a
normalized `InboundMessage` on a durable spine that `after:ingest[:<source>]` functions
consume* — the symmetric mirror of the outbound observer→signed-webhook path. A `Source`
models both **push** (ack-based, e.g. a provider webhook) and **pull** (cursor-based, e.g.
the poll-IMAP email adapter behind the `inbound-email` feature) transports; the shared
normalization above transport (idempotency/thread keys, bodies, attachments, declared
routing) lives once in `InboundMessage`.

Each normalized delivery is deduplicated by `(source, idempotency_key)` on the spine
(`_fraiseql_inbound_message`) and fires `after:ingest[:<source>]` functions on the same
I/O-capable host context as `after:mutation`, reusing the durable dispatch path (retry +
dead-letter).

> **Durability boundary.** "Durable dispatch" means dispatch *failures* are retried and
> land in the dead-letter queue. It does not mean dispatch survives process death: the
> spine row is committed before dispatch, but nothing reads it back, so a crash between
> the commit and the dispatch's completion loses that dispatch — and the committed row
> makes the provider's redelivery a `duplicate`. See the `inbound::spine` module docs. A declared routing rule maps a message to an entity by dedicated address +
plus-tag (`support+ticket-42@…` → `Ticket`/`42`); an `after:ingest` handler receives the
whole message and can route it itself. See `docs/architecture/inbound-email.md` for the
poll-IMAP adapter and `docs/architecture/functions.md` for the `after:ingest` host surface.

### Supported Providers

| Provider | Signature Algorithm | Header |
|----------|---------------------|--------|
| Stripe | HMAC-SHA256 | `Stripe-Signature` |
| GitHub | HMAC-SHA256 | `X-Hub-Signature-256` |
| Shopify | HMAC-SHA256 | `X-Shopify-Hmac-Sha256` |
| SendGrid | ECDSA | `X-Twilio-Email-Event-Webhook-Signature` |
| Paddle | RSA-SHA256 | `Paddle-Signature` |
| Custom | Pluggable | Implement `WebhookProvider` trait |

### Security Properties

- **Constant-time comparison** — all HMAC/signature comparisons use `subtle::ConstantTimeEq`
  to prevent timing attacks.
- **Replay protection** — Stripe and Paddle webhook signatures include a timestamp;
  requests older than 5 minutes are rejected.
- **Idempotency** — each webhook carries a provider-issued event ID. If the same ID
  arrives twice, the second delivery is silently discarded without running the handler.
- **Transaction boundaries** — each webhook handler runs inside a database transaction.
  If the handler function raises an error, the transaction is rolled back and the HTTP
  response is 500 so the provider retries.

### Processing Flow

```
Provider (Stripe, GitHub, …)
         │
         │ POST /webhooks/{segment}
         ▼
┌────────────────────────────┐
│ Signature Verification     │  ← constant-time HMAC check
│ (per-provider algorithm)   │
└────────────┬───────────────┘
             │ valid
             ▼
┌────────────────────────────┐
│ Idempotency Check          │  ← deduplicate by (route, event id)
│ (seen on THIS route?)      │
└────────────┬───────────────┘
             │ new event
             ▼
┌────────────────────────────┐
│ Event Router               │  ← dispatch by (provider, event_type)
│ e.g. "stripe/payment.succeeded" → fn_handle_payment_succeeded()
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│ Database Transaction       │  ← handler runs inside BEGIN…COMMIT
│ fn_handle_payment_succeeded($1::jsonb)
└────────────────────────────┘
```

### Body formats

The route reads the request's `Content-Type` and parses accordingly:

| Declared type | Parsed as |
|---|---|
| `application/x-www-form-urlencoded` | a JSON object — values percent-decoded, `+` as space; a key that repeats becomes the array of its values in wire order |
| anything else | JSON; a body that does not parse is `400` |

Form support is not Twilio-specific, but Twilio is why it exists: it posts SMS and
voice callbacks form-encoded, and the form arm of its signing scheme is built for
exactly that shape. While the route rejected every non-JSON body it did so *before*
verification, so a correctly configured Twilio route answered `400` to 100% of
genuine SMS callbacks (#1044).

Verification is unaffected by any of this — it reads the raw request bytes, never
the parsed value — so parsing a form body cannot weaken a signature check.

### Dedup scope: the route, not the provider

Several `[webhooks.*]` routes may serve one `provider`, and they are meant to: two
partners signing with the generic `hmac-sha256` scheme under separate secrets, a
live/test pair, two accounts of one multi-tenant provider. Each sender numbers its
own events, so the same event id turns up on both.

Both dedup layers are therefore namespaced by **route** — the `/webhooks/{segment}`
path segment, which is a route's `path` override or, absent one, its config key:

* the delivery ledger claims `(route, event_id)`;
* the durable spine claims `(source, "<route length>:<route>:<event id>")`.

The spine has to flatten its half into one column, and the sender chooses the event
id — so the join is length-prefixed to keep it injective. A bare `<route>:<id>` join
is not: route `a` receiving the id `b:1` lands on route `a:b`'s event `1`.

Keying on the provider instead meant the second sender's genuine delivery met the
first's claim, was answered `200 {"status":"duplicate"}`, never reached the spine
and never fired `after:ingest` — and, since the 200 reads as success, was never
retried (#1046). A route segment is a sound namespace because the server refuses at
boot to mount two routes resolving to one segment (#1048).

What is *not* route-scoped is the `after:ingest:webhook:<provider>` trigger
discriminant: it stays provider-shaped, so co-provider routes fire the same declared
handlers. A handler that must distinguish its senders should read the payload.

### Configuration

```toml
# fraiseql.toml
[webhooks.stripe]
secret = "whsec_..."        # signing secret from Stripe Dashboard
endpoint_path = "/webhooks/stripe"

[webhooks.github]
secret = "my-github-secret"
endpoint_path = "/webhooks/github"
```

---

## Outbound Observer Notifier (`fraiseql-observers`)

### Overview

Observers watch the PostgreSQL change feed (via logical replication or polling) and
emit events when rows are inserted, updated, or deleted. Each observer has a
**condition** (a small DSL, see `condition/`) and a set of **actions** to fire when
the condition is true.

### Condition DSL

```
# Field comparisons
status == 'shipped'
total > 100

# Field-change detection — requires the producing mutation to record a
# pre-image (`changelog_pre_image = true`, off by default). Without it these
# conditions error loudly at evaluation instead of firing.
field_changed_to('status', 'shipped')
field_changed_from('status', 'pending')

# Existence checks
has_field('deleted_at')

# Logical operators
(total > 100) && field_changed_to('status', 'shipped')
```

### Action Types

- **HTTP webhook** — POST a JSON payload to an external URL
- **NATS message** — publish to a NATS topic
- **Email** — send a transactional email via configured provider
- **Database function** — call a PostgreSQL function as a side-effect

### Configuration

```toml
# fraiseql.toml
[[observers]]
table = "orders"
condition = "field_changed_to('status', 'shipped')"

[[observers.actions]]
type = "webhook"
url = "https://fulfillment.example.com/notify"
method = "POST"
```

---

## Relationship Between the Two Subsystems

The two subsystems are independent and can be used together:

```
External event (Stripe)
       │
       │ inbound webhook
       ▼
fraiseql-webhooks
       │ writes to database
       ▼
Database row changes
       │
       │ observer detects change
       ▼
fraiseql-observers
       │ outbound notification
       ▼
Your fulfilment service or analytics pipeline
```

This pattern creates a fully event-driven pipeline where external events flow through
FraiseQL's data layer and trigger downstream notifications — all with transactional
guarantees.
