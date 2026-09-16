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

### Supported schemes

Every value `provider` accepts, read off the schemes `scheme::build_scheme`
constructs — which is the one construction the boot check and the mounted router
are both served from, so this table cannot describe a scheme the server does not
have (#1338).

| `provider` | What is signed | Credential | Timestamp | Key material in `secret_env` |
|---|---|---|---|---|
| `stripe` | `HMAC-SHA256("{t}.{body}")`, hex | `Stripe-Signature` (`t=`,`v1=`; several `v1` during rotation) | inside the header | shared secret |
| `github` | `HMAC-SHA256(body)`, hex behind `sha256=` | `X-Hub-Signature-256` | — | shared secret |
| `shopify` | `HMAC-SHA256(body)`, base64 | `X-Shopify-Hmac-Sha256` | — | shared secret |
| `gitlab` | nothing — a static token compared in constant time | `X-Gitlab-Token` | — | the token |
| `slack` | `HMAC-SHA256("v0:{ts}:{body}")`, hex behind `v0=` | `X-Slack-Signature` | `X-Slack-Request-Timestamp` | signing secret |
| `twilio` | `HMAC-SHA1(url + sorted form params)`, base64; JSON bodies sign the URI including `bodySHA256` | `X-Twilio-Signature` | — | auth token |
| `sendgrid` | ECDSA P-256 over `timestamp + body`, DER in base64 | `X-Twilio-Email-Event-Webhook-Signature` | `X-Twilio-Email-Event-Webhook-Timestamp` | PEM **public** key |
| `postmark` | `HMAC-SHA256(body)`, base64 | `X-Postmark-Signature` | — | shared secret |
| `paddle` | `HMAC-SHA256("{ts}:{body}")`, hex | `Paddle-Signature` (`ts=`,`h1=`) | inside the header | shared secret |
| `lemonsqueezy` | `HMAC-SHA256(body)`, hex | `X-Signature` | — | shared secret |
| `discord` | Ed25519 over `timestamp + body`, hex | `X-Signature-Ed25519` | `X-Signature-Timestamp` | hex **public** key |
| `standard-webhooks` | `HMAC-SHA256("{id}.{timestamp}.{body}")`, base64 behind `v1,` | `webhook-signature` (space-separated list; several during rotation) | `webhook-timestamp` | base64 secret behind `whsec_` |
| `clerk` | the same, under Svix's header spelling | `svix-signature` | `svix-timestamp` | base64 secret behind `whsec_` |
| `hanko` | a JWT in the body field `token`, checked against the tenant's JWKS | `body:token` | the token's own `exp` | **none** — keys are published |
| `kinde` | a JWT that **is** the body (`application/jwt`) | `body` | the token's own `exp`, if it carries one | **none** — keys are published |
| `fusionauth` | a JWT in a header, bound to the body by a `request_body_sha256` claim | `header:X-FusionAuth-Signature-JWT` | the token's own `exp` | **none** — keys are published |
| `jwt-jwks` | a JWT checked against a published JWKS | configurable — see below | the token's own `exp` | **none** — keys are published |
| `hmac-sha256` | `HMAC-SHA256(body)` | configurable — see below | — | shared secret |
| `hmac-sha1` | `HMAC-SHA1(body)` | configurable — see below | — | shared secret |

Paddle is **HMAC-SHA256**, not RSA; this table said otherwise, and listed 5 of the 13
schemes that existed when #1338 was filed, plus a `WebhookProvider` trait that has never
existed. The extension point is
[`SignatureVerifier`](https://docs.rs/fraiseql-webhooks), which since #1321 is handed the
whole request and returns **what it authenticated** rather than a boolean.

A scheme whose sender is not on this list, but which signs the raw body with a shared
secret, is `hmac-sha256` (or `hmac-sha1`) plus configuration — see
[Describing a scheme yourself](#describing-a-scheme-yourself) below. That is how Lago,
a self-hosted sender, or a bespoke integration is received without a code change.

### Standard Webhooks (Svix, Clerk) — `standard-webhooks` / `clerk`

Every [Standard Webhooks](https://www.standardwebhooks.com/) sender signs
`"{id}.{timestamp}.{body}"` and puts the three values in three headers. `clerk` is that
scheme under Svix's `svix-*` spelling and is otherwise identical, so it is a preset rather
than a second implementation; a sender using some third spelling is `standard-webhooks`
plus `header_prefix`.

```toml
[webhooks.clerk]
provider   = "clerk"
secret_env = "CLERK_WEBHOOK_SECRET"       # the whsec_… from Clerk's dashboard

[webhooks.partner]
provider      = "standard-webhooks"
secret_env    = "PARTNER_WEBHOOK_SECRET"
header_prefix = "svix"                    # webhook (default) | svix | any token
```

**The dedup key is the signed `{prefix}-id`.** This is the one scheme where the delivery's
identity comes out of a header, and it is trustworthy only because the signature covers
it. #751 was the same header keyed *before* anything signed it: one captured delivery
replayed under a fresh id claimed a fresh idempotency key and re-fired every
`after:ingest` function. The event **type** still comes out of the signed body, because
here the body is signed too.

**The event type comes from the signed body's `type` field.** Clerk sends `type`, so a
`clerk` route records the type you expect. The spec says nothing about body shape, though,
and Svix's own example payload uses `event_type` — a sender spelling it that way records an
empty event type and no message `subject`. The **dedup key is unaffected**: it comes from
the signed header. If you hit this, say so on #1323 rather than renaming the field at the
sender; the fix is a per-scheme extraction rule, not a second field name tried for all
fifteen schemes.

**Several `v1,` signatures are accepted, and any match is enough.** A sender rotating its
signing secret emits one entry per active secret, space-separated, in no guaranteed order.
Each candidate is compared in constant time whether or not an earlier one matched.

**An unrecognised version tag is skipped, not refused.** A `v1a,` or future `v2,` entry
beside a good `v1,` one is ignored, so a sender adding a version mid-migration does not
break every delivery. Nothing in the spec text states this rule; it is the only behaviour
compatible with a sender growing a version, and it is pinned by test rather than by
citation.

**The secret's decoded length is not validated.** The spec states that secrets are 24–64
bytes. Svix's own manual-verification documentation publishes
`whsec_plJ3nmyCDGBKInavdOK15jsl`, which decodes to **18** — so a decoder enforcing the
spec's range refuses the provider's documented key. What is validated is that the secret
decodes at all.

**Asymmetric `v1a` (Ed25519) is not implemented, and says so at boot.** A route whose
`secret_env` holds `whpk_…` or `whsk_…` key material is refused when the server starts,
naming the prefix and the version. The alternative — mounting the route and answering 401
to every genuine delivery — is a gap an operator has to read the logs to discover.

### Identity providers that sign with a JWT — `hanko` / `kinde` / `fusionauth` / `jwt-jwks`

These four do not send an HMAC over the body. They send a **token** signed by a key
the provider publishes in a JWKS, and for three of them the event is *inside* the
token rather than in the request body.

| | `hanko` | `kinde` | `fusionauth` |
|---|---|---|---|
| where the token is | body field `token` | the whole body | header `X-FusionAuth-Signature-JWT` |
| what the signature covers | the claims `evt` + `data`. The outer `event` field is **not signed** | the claims | the **body**, via a `request_body_sha256` claim = base64(SHA-256(raw bytes)) |
| the dispatched event | the signed `evt` / `data` | the signed `type` / `data` | the body, which the digest claim makes verified material |
| what the ledger keys on | SHA-256 of the verified token | the signed `event_id` | the body's own id, as the receiver's rules find it |
| what bounds replay | the token's `exp` (`iat` + 300) | **the delivery ledger alone** — Kinde documents no `iat`/`exp` and retries for up to 36 h | the token's `exp` |
| key types | tenant JWKS, RSA | RSA | RSA **or** EC — both accepted out of the box, since either is an ordinary FusionAuth setup. An EdDSA or HMAC key is refused **by name** |

Two facts an operator cannot guess:

**A route for these carries no `secret_env`, and setting one refuses the boot.**
There is no shared secret: the provider publishes the key. A `secret_env` on such a
route is key material nothing consults, and this configuration refuses rather than
ignores it. Conversely a `jwks_uri` is **required**, and a route without one does not
boot.

**Kinde's replay window is the delivery ledger, not a timestamp.** Its tokens carry
no `exp`, so nothing about a captured token goes stale. What stops a replay is that
the ledger already holds its `event_id`. Do not truncate
`webhooks.tb_inbound_delivery` for a route serving Kinde.

And one for Hanko specifically: its tokens carry **no `jti` and no event id**, so the
delivery id is a digest of the verified token. If Hanko re-signs on retry — a fresh
`iat` gives a fresh digest — each attempt is a distinct delivery and the ledger cannot
coalesce retries. Treat delivery as at-least-once and write idempotent
`after:ingest` handlers. (This is not yet confirmed against a captured retry.)

#### Token confusion: why each preset checks claims, not just the signature

A provider's webhook JWKS is usually the **same key set that signs its end-user
sessions**. Hanko's is — FraiseQL's issuer-less OIDC mode exists to validate exactly
those tokens — and Kinde's webhook JWKS is its access-token JWKS.

So a scheme that accepted "any token that verifies against this key set" would accept
**any logged-in end user POSTing their own session token as a webhook**, and the
signature would be genuine. Each preset therefore fixes the claims that tell a
webhook token from a user token and refuses a token without them:

| preset | required |
|---|---|
| `hanko` | `sub == "hanko webhooks"`, plus `evt` and `data` present |
| `kinde` | `event_id`, `type` and `source` present |
| `fusionauth` | the `request_body_sha256` claim — which is also what binds the body, so it is one check and not two |

`audience` is **defence in depth on top of that**, and is validated whenever it is
set. It is not mandatory only because a provider's webhook token may carry no `aud`
at all, and a route that demanded one would refuse every genuine delivery from such
a provider. Set it whenever the provider sends one.

```toml
[webhooks.hanko]
provider = "hanko"
jwks_uri = "https://your-tenant.hanko.io/.well-known/jwks.json"
audience = "my-app"          # the service name you registered at Hanko
# secret_env is NOT set — and setting it refuses the boot
```

#### What a preset fixes, and what your deployment still chooses

A preset fixes what the **provider** decided about its own tokens: where the token
is, which claims carry the event, and which claims tell a webhook token from a user
token. Configuring any of those on a preset route refuses the boot, because it
would be a knob nothing consults.

What your **deployment** decided stays yours, and a preset reads all four:
`jwks_uri` (a tenant's key set is per deployment, so not even a preset can fix it),
`audience` (it names this service), `algorithms` (which of the algorithms the
provider supports this route will accept — FusionAuth can be configured with an RSA
or an EC key, so narrowing it is a real choice), and `max_age_secs` (a freshness
policy on top of the token's own `exp`, which is your risk appetite and nothing the
provider states).

`jwt-jwks` is the same machinery with the provider's details left to you, for a
JWT-signing sender that is not one of the three: `credential` says where the token
is — **required**, there is no default — `algorithms` is the allow-list (`RS256` by
default; `none` and the `HS*` family are refused whatever it says), and
`event_type_claim` / `payload_claim` / `id_claim` name where the event sits inside
the token. `body_hash_claim` is the FusionAuth shape — setting it means the **body**
is the event, so it cannot be combined with those three.

### Security Properties

- **Constant-time comparison** — all HMAC/signature comparisons use `subtle::ConstantTimeEq`
  to prevent timing attacks.
- **Replay protection** — the six timestamped schemes (Stripe, Paddle, Slack, Discord,
  SendGrid, Standard Webhooks) reject a delivery outside a 5-minute window, through one
  shared freshness check so the rule cannot drift between them. The window is inclusive at
  its edge: exactly 300 s old verifies, 301 s does not.
- **Idempotency** — a delivery is deduplicated on `(route, event id)`, and the id comes out of
  **verification**, never out of the unverified request (#751/#1321). For every scheme above the
  body is the event, so the id is read from the verified body — except
  `standard-webhooks` / `clerk`, whose signed content covers an id carried in its own
  header, and which report that id. A scheme that authenticates an event carried entirely
  in signed material reports that event's id instead. A dedup key taken from an unverified
  header or envelope would put the whole replay defence under the sender's control.
- **Nothing is read out of the request before the scheme runs** — not the credential, not the
  body. A route that refused a request with no signature header, or an unparseable body, before
  verifying could not serve a scheme whose credential is elsewhere, and answered an
  unauthenticated caller about the endpoint's shape.
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

- the delivery ledger claims `(route, event_id)`;
- the durable spine claims `(source, "<route length>:<route>:<event id>")`.

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
provider   = "stripe"                 # selects the signature verifier
secret_env = "STRIPE_WEBHOOK_SECRET"  # NAME of the env var holding the signing secret

[webhooks.github]
provider   = "github"
secret_env = "GITHUB_WEBHOOK_SECRET"
```

`provider` is required and has no default: a route without one fails to deserialize
and the server does not boot.

`secret_env` is required **by most schemes but not by all**, and which way round is
the scheme's answer rather than the configuration format's (#1322). Every
shared-secret scheme needs it, and a route missing one is refused in production (and
skipped with a warning in development, so an unfinished local setup answers 404
rather than 500). The four JWT schemes need the opposite: they verify against keys
the provider publishes, so a `secret_env` there is key material nothing consults and
**refuses the boot in every environment**.

`secret_env` is the *name of an environment variable*, not the secret. The signing
secret never appears in `fraiseql.toml`, which is a file that gets committed.

Optional keys:

| Key | Read by | Meaning |
|---|---|---|
| `path` | every scheme | the path **segment** this route mounts under, overriding the route name. The route is served at `/webhooks/{segment}` — so `path = "stripe-eu"`, not `path = "/webhooks/stripe-eu"`. |
| `public_url` | URL-signing schemes | the exact public URL the provider knows this route by. **Required** for a scheme whose signature covers the request URL (Twilio signs scheme + host + path + query). Reconstructing it from `Host` / `X-Forwarded-*` would put the signed material under the sender's control, so the server refuses to boot instead (#781). |
| `credential` | `hmac-sha256`, `hmac-sha1` | where the credential is: `header:<Name>`. Defaults to `header:X-Signature`. |
| `encoding` | `hmac-sha256`, `hmac-sha1` | `hex` (default) or `base64`. |
| `prefix` | `hmac-sha256`, `hmac-sha1` | a literal stripped before decoding, e.g. `sha256=`. |
| `header_prefix` | `standard-webhooks` | the spelling of the Standard Webhooks header triple — `{prefix}-id`, `{prefix}-timestamp`, `{prefix}-signature`. Defaults to `webhook`, the spec's own; Svix and Clerk send `svix`. The `clerk` preset **is** that spelling and refuses the key. |
| `jwks_uri` | `jwt-jwks`, `hanko`, `kinde`, `fusionauth` | where the provider serves the keys its tokens are signed by. **Required** for these schemes — a route without one does not boot. Must be `https`, or `http` on a loopback host for a local development IdP. A tenant's key set is per deployment, which is why even a preset reads this. |
| `audience` | the four JWT schemes | the `aud` a token must carry. Validated when set; see [token confusion](#token-confusion-why-each-preset-checks-claims-not-just-the-signature) for why you want it set. |
| `algorithms` | the four JWT schemes | the `alg` allow-list. Defaults to `["RS256"]`, except `fusionauth`, which defaults to RSA **and** EC because its signing key may be either. Checked **before** any key lookup, so a token this route would refuse on its header alone costs no request to the provider. `none` and the `HS*` family are refused at boot whatever this says: an HMAC algorithm verified against a *public* key set means anyone who can read that key set can forge a token. |
| `max_age_secs` | the four JWT schemes | an additional freshness window on `iat`, beyond the token's own `exp`. Leave it unset unless you know your provider re-signs on retry: a provider that retries for hours with a reused token would have every retry past this age refused. A route that sets it and receives a token with no `iat` refuses the delivery — the age cannot be established. |
| `credential` | `jwt-jwks` | where the token is: `header:<Name>`, `body`, or `body:<field>`. **Required** — the generic scheme has no default, because no provider puts a JWT in the HMAC families' `X-Signature` and inheriting that spelling would only produce a 401 per delivery. The three presets fix their own and refuse the key. |
| `event_type_claim`, `payload_claim`, `id_claim` | `jwt-jwks` | which claims carry the event's type, its payload, and its id. `id_claim` unset means the id is a digest of the verified token. The presets fix their own. |
| `body_hash_claim` | `jwt-jwks` | the claim carrying `base64(SHA-256(raw body))`, which binds a token in a header to the body it arrived with. Setting it makes the **body** the event, so it cannot be combined with the three claim keys above. |

**An unknown key refuses to boot, and so does a key the chosen scheme does not read**
(#1321). `encoding = "base64"` on a `stripe` route is not ignored — Stripe fixes its own
signing details, so the key would be configuration nothing consults, which is the same
silent drop one level down. Before this, a mistyped `encodng = "base64"` parsed exactly
like the correct spelling and the route quietly served the default scheme.

Each scheme declares the keys it reads, rather than the rule being per *family*. Before
`header_prefix` the split was binary — a preset read nothing, the two HMAC families read
all three credential keys — and a fourth key read by exactly one other scheme had nowhere
to be refused: it would have been accepted on an `hmac-sha256` route and ignored.

### Describing a scheme yourself

```toml
[webhooks.lago]
provider   = "hmac-sha256"
secret_env = "LAGO_WEBHOOK_SECRET"
credential = "header:X-Lago-Signature"   # where the MAC is
encoding   = "base64"                    # how it is written

[webhooks.selfhosted]
provider   = "hmac-sha256"
secret_env = "SELFHOSTED_WEBHOOK_SECRET"
credential = "header:X-Hub-Signature-256"
encoding   = "hex"
prefix     = "sha256="                   # stripped before decoding
```

Header names are matched case-insensitively, so the spelling here is the operator's and
the one on the wire is the sender's.

The `credential` grammar is `header:<Name>` | `body` | `body:<field>`, shared by every
scheme family so the next one does not introduce a second key meaning the same thing. The
two HMAC families accept `header:` only, and refuse the body forms **at boot**: their
credential is a MAC over the request body, so it cannot also be part of that body.

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
