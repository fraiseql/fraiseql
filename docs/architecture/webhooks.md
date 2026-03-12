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
         │ POST /webhooks/{provider}
         ▼
┌────────────────────────────┐
│ Signature Verification     │  ← constant-time HMAC check
│ (per-provider algorithm)   │
└────────────┬───────────────┘
             │ valid
             ▼
┌────────────────────────────┐
│ Idempotency Check          │  ← deduplicate by event ID
│ (event_id already seen?)   │
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

# Field-change detection
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
