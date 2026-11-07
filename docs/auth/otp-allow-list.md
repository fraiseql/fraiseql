# Restricting email OTP sign-in to approved addresses

With `[auth.local] otp = true`, anyone can ask for a code for any address, and completing the
flow creates an account. An invite-only deployment refuses addresses with `otp_allowed_query`:

```toml
# fraiseql.toml
[auth.local]
otp               = true
email_from        = "support"
otp_allowed_query = "SELECT 1 FROM tb_member WHERE email = $email AND active"
```

A row admits the address. No row, or a query that fails, refuses it. A refused address gets no
mail, no row in `auth.tb_user` and no session. Leaving the key unset admits every address.

## Writing the query

- It must be one `SELECT` that refers to `$email`. `$email` is the only parameter, it is always
  bound and never written into the text, and it may appear more than once. Comments and `;` are
  refused, at compile time and at boot.
- It runs in a read-only transaction with a 2-second timeout, so it cannot change anything. A
  query that writes or runs out of time refuses the address.
- `$email` is trimmed and lowercased. Compare it with a lowercased column, or
  `lower(email) = $email`, or an address stored in mixed case is refused.

## What a caller can learn

Nothing about who is approved. Both addresses get the same status and body at `POST /auth/v1/otp`
and `POST /auth/v1/verify`, past both per-address limits. A refused address gets a stored code
that is never mailed, so the limits are charged the same way. The list is checked again at
verify, so removing an address invalidates a code it already holds, and a refused address sees
the answer a wrong code gets.

## Mail leaves off the request

With the key set, mail goes through a queue of 256, delivered one at a time, so the relay's speed
and failures cannot tell a caller who is approved. That has two costs:

- `POST /auth/v1/otp` answers 200 even when delivery fails. A failure is logged at error level
  with the address removed, and is the only sign of it: alert on `OTP mail delivery failed`.
- When the relay stalls long enough to fill the queue, further mail is dropped and logged as
  `OTP mail dropped`. No request waits. Mail still queued at shutdown is lost.

With the key unset, mail is delivered on the request and a failure answers 500.

## What it does not cover

The key governs OTP only. Password signup and social sign-in still create accounts for any
address, and the compiler warns when either is enabled beside it. `[identity.enrichment]` remains
the place to refuse a subject on every request, however it signed in.
