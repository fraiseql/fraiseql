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
#411 identity tables it FK-references):

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

## Deferred

Named here so they are tracked, not assumed handled:

- **Rate limiting / lockout** on repeated failures. Argon2's cost throttles online
  guessing only so far; per-account/IP backoff is a follow-up (`fraiseql-auth`'s
  `rate_limiting` primitives are the building block). A lockout is itself a disabled-state
  with the same disclosure trade-off as above.
- **Non-enumerable signup.** `EmailAlreadyRegistered` (409) is a signup existence oracle.
  The standard "we emailed you" mitigation needs the email-action path (#349), not yet
  shipped, so v1 returns the distinct error and documents it.
- **Password reset / email verification** — #367, reusing the #349 email path.
- **Configurable password policy.** v1 enforces a fixed minimum (12 bytes) and maximum
  (4096 bytes, a DoS guard) length.
