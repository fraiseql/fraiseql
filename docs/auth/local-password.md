# Local password authentication

`LocalPasswordAuthenticator` adds email + password sign-in on top of the
[persistent identity store](identity-store.md). Passwords are hashed with **Argon2id**
and stored in `core.tb_password_credential` — never adjacent to plaintext, never in the
user row. It is always compiled in; you opt in by constructing it and wiring routes.

## Why

The OAuth/OIDC, API-key, TOTP, and email-OTP methods all assume an external identity
provider or a second factor. Email + password is the baseline a self-contained app
expects, and it is the prerequisite for the password-reset flow (#367).

## Usage

```rust
use std::sync::Arc;
use fraiseql_auth::{AccountStore, LocalPasswordAuthenticator, PostgresAccountStore};
use sqlx::postgres::PgPoolOptions;

let pool = PgPoolOptions::new().connect(&database_url).await?;
let accounts: Arc<dyn AccountStore> = Arc::new(PostgresAccountStore::new(pool.clone()));
let auth = LocalPasswordAuthenticator::new(pool, accounts);
auth.init().await?; // idempotent; ensures the #411 identity tables + the credential table

// Signup and login both return the stable `user_id`; mint a session from it as usual.
let user_id = auth.signup("alice@example.com", "correct horse battery staple").await?;
let user_id = auth.login("alice@example.com", "correct horse battery staple").await?;
```

The connecting `PgPool` role must **own** (or carry `BYPASSRLS` for) the `core` tables —
calling `init()` creates them, so the connecting role owns them by construction.

Use `LocalPasswordAuthenticator::with_params(pool, accounts, m_cost, t_cost, p_cost)` to
raise the Argon2id cost above the OWASP default (`new`); credentials hashed with weaker
parameters upgrade automatically on the next successful login (see *Rehash* below).

## Schema

One table in the `core` schema, created idempotently by `init()` (which also ensures the
# 411 identity tables it FK-references):

| Table | Purpose |
| --- | --- |
| `core.tb_password_credential` | One row per local credential: `fk_user`/`user_id`, the Argon2id `password_hash`, `disabled_at`, `tenant_id`. `UNIQUE (fk_user)` → at most one local password per account. |

It mirrors the #411 tables exactly: Trinity `pk_`/`fk_`/`id` columns and deny-by-default
RLS (`ENABLE`, not `FORCE`; the owning store bypasses, any other non-`BYPASSRLS` role
reads zero rows without the `fraiseql.tenant_id` GUC; `REVOKE ALL … FROM PUBLIC`). v1 is
single-tenant (`tenant_id` left `NULL`); per-tenant scoping is a forward-compatible
extension, identical to the identity store.

## Security design

These are deliberate decisions, not defaults — they shape the API behaviour.

### Credential identity: `provider_id` is the normalized email

Signup resolves or creates the user through the `AccountStore` with provider `"local"`
and `provider_id = normalize_email(email)`. Login reuses the
`UNIQUE (provider, provider_id)` index on `core.tb_auth_identity` as the email → `user_id`
lookup key — a single source of truth, no extra column.

### Signup is fail-closed

Signup links with `email_verified = false`. A local signup therefore keys its **own**
`(local, email)` account and can never auto-merge into an existing verified-email account
(e.g. a prior Google sign-in for the same address). This is the H26 protection: an
attacker cannot sign up locally with a victim's email and reach the victim's account.
`core.tb_user.email` stays `NULL` for a local account until a verification flow promotes
it; cross-linking is deferred to that flow (#367).

### Login is non-enumerable

An **unknown user** and a **wrong password** return the *same* `InvalidCredentials`
(identical 401 body) and pay the *same* Argon2 cost:

- The email → credential lookup runs on **both** paths, so the database round-trip cannot
  leak existence.
- An unknown user is verified against a pre-computed **dummy hash** built from the *same*
  Argon2 parameters as live credentials, so verification timing cannot leak existence.
  The verification always runs.

The server audit log records the precise reason (`unknown_user` / `wrong_password` /
`disabled`) under `AuthFailure`, so operators keep full diagnostics while the client sees
one merged error.

### Disabled is a narrow, deliberate disclosure

`set_password_disabled(user_id, true)` administratively suspends local sign-in. A login
then returns the distinct `AccountDisabled` (403) **only when the supplied password is
correct**; a wrong password against a disabled account still returns `InvalidCredentials`.
Disclosing "this account is disabled" to a party that already holds valid credentials is
an accepted trade-off for this threat model — it is never reachable without the correct
password, so it is not an existence oracle. If "disabled" must be invisible even to a
credentialed party for your deployment, fold it into `InvalidCredentials`.

### Rehash on policy change

When a successful login's stored hash used weaker Argon2 parameters than the current
policy, the (correct) password is transparently re-hashed and the stored hash is updated.
A rehash failure never fails the login — the next login retries.

## Password reset (#367)

`start_password_reset` / `confirm_password_reset` add a single-use, one-hour,
non-enumerable reset on top of the same credential store. Like signup/login they ship as a
library primitive: you construct the authenticator and wire your own routes — no HTTP
endpoint or SMTP client is built in.

```rust
use fraiseql_auth::{ResetEmailSender, SessionStore};

let auth = LocalPasswordAuthenticator::new(pool, accounts)
    .with_email_sender(email_sender)    // Arc<dyn ResetEmailSender>: delivers the link
    .with_session_store(session_store); // Arc<dyn SessionStore>: revoked on a reset
auth.init().await?; // also creates core.tb_password_reset_token

// Non-enumerable: always Ok(()), whether or not the email has a local account.
auth.start_password_reset("alice@example.com").await?;
// The user clicks the emailed link; your route hands the token back here.
auth.confirm_password_reset(&token, "a brand new passphrase!").await?;
```

### Token design: selector + verifier

The opaque token handed to the user is `selector.verifier` (base64url). The store
(`core.tb_password_reset_token`, FK-linked to `core.tb_user`, same deny-by-default RLS as
above) keeps only the **selector** (indexed, non-secret) and **`sha256(verifier)`** — never
the raw token. Redemption looks the row up by selector (no secret in the `WHERE`, so the
lookup is not an existence oracle), then compares the SHA-256 of the presented verifier
against the stored hash in **constant time**. A full database read cannot forge a usable
token: it would need a SHA-256 preimage of a 256-bit CSPRNG verifier. SHA-256 (not Argon2)
suffices precisely because the verifier is high-entropy — there is no brute-force surface.

### Start is non-enumerable

`start_password_reset(email)` always returns `Ok(())`. The credential lookup runs on every
path, a token is issued only for an email that has a local credential, and the link is
dispatched in a spawned task — so an unknown or OAuth-only email returns indistinguishably
from one that issued a token. The audit log records the precise reason
(`no_local_credential` on the no-op path; success otherwise).

### Confirm is single-use and rotates everything

`confirm_password_reset(token, new_password)` enforces the same password-length policy as
signup, then: verifies the token, rejects it if expired or already used, sets the new
Argon2id hash, marks the token used under an atomic guard (`WHERE used_at IS NULL AND
expires_at > now()` — a concurrent second redemption affects zero rows and is rejected),
invalidates the user's **other** outstanding tokens, and revokes the user's sessions via
the wired `SessionStore`. Any unredeemable token (unknown / malformed / expired / used /
wrong verifier) returns one generic `InvalidToken`; the audit log records the precise
reason.

Email delivery is abstracted behind `ResetEmailSender` so `fraiseql-auth` carries no SMTP
dependency; provide a concrete sender (e.g. `lettre`, or one bridging the #349 observer
SMTP path) when you wire routes. Without a sender, a token is still issued and persisted but
a warning is logged rather than delivering it; without a session store, the password changes
but a warning notes that outstanding sessions were not revoked.

## Deferred

Named here so they are tracked, not assumed handled:

- **Rate limiting / lockout** on repeated failures. Argon2's cost throttles online
  guessing only so far; per-account/IP backoff is a follow-up (`fraiseql-auth`'s
  `rate_limiting` primitives are the building block). A lockout is itself a disabled-state
  with the same disclosure trade-off as above.
- **Non-enumerable signup.** `EmailAlreadyRegistered` (409) is a signup existence oracle.
  The standard "we emailed you" mitigation needs the email-action path (#349), not yet
  shipped, so v1 returns the distinct error and documents it.
- **HTTP endpoints and a concrete `ResetEmailSender`** for the reset flow above — deferred
  to the step that wires the local-auth routes (login/signup have none yet either). The
  service primitive and the `ResetEmailSender` trait ship now.
- **Email verification** — the remaining #367 sub-flow, reusing the same #349 email path;
  it is what will promote `core.tb_user.email` from `NULL` for a local account.
- **Configurable password policy.** v1 enforces a fixed minimum (12 bytes) and maximum
  (4096 bytes, a DoS guard) length, shared by signup and password reset.
- **Reset rate limiting.** `start_password_reset` is not yet rate-limited against
  token-issuance flooding — the same follow-up as login rate limiting above.
