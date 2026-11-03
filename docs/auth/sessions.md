# Session lifetime, refresh and logout

Every sign-in FraiseQL serves itself (password, email OTP, MFA, social and SAML) answers with
an access token and a refresh token. `[auth.session]` sets how long the access token lives, and
whether the refresh token can get a new pair:

```toml
# fraiseql.toml
[auth.session]
ttl_secs          = 900      # each access token lives 15 minutes
absolute_ttl_secs = 2592000  # a session can be refreshed for 30 days after sign-in
```

Both keys are optional, and the compiler and the server's boot refuse values that could not
work:

- `ttl_secs` must be at least 60.
- `absolute_ttl_secs` must be longer than the access token's lifetime, which is `ttl_secs`, or
  3600 when that is unset. Zero is refused for both.
- Any other key is refused.

## Without `[auth.session]`

Nothing changes. Password, OTP and MFA sign-ins mint an access token that lives one hour; social
and SAML sign-ins mint one that lives seven days. The session ends with its access token. No
refresh or logout route is mounted.

## Access-token lifetime

`ttl_secs` sets the lifetime of the access token from every sign-in path, and of every token a
refresh mints. When only `absolute_ttl_secs` is set, every path mints one-hour access tokens,
social and SAML included.

The token validator accepts a token for **60 seconds past its `exp`**, to absorb clock drift
between servers. An access token is therefore accepted for `ttl_secs + 60` seconds. That leeway
is not configurable, and it is why `ttl_secs` cannot be under a minute.

Anonymous sessions (`[auth.local] anonymous = true`) ignore `[auth.session]`: they keep a fixed
seven days and are never refreshable.

## Refresh

`absolute_ttl_secs` opts in to refresh. It mounts:

```http
POST /auth/v1/session/refresh
Content-Type: application/json

{"refresh_token": "..."}
```

| Answer | When |
|--------|------|
| `200` `{"access_token", "refresh_token", "expires_in"}` | The token was current. It is now used up: keep the new pair. |
| `409` `{"error": "refresh_token_already_rotated"}` | The token was exchanged in the last 10 seconds. Nothing is minted and nothing is ended. This is a client racing itself, for instance two tabs refreshing at once; use the pair the first exchange returned. |
| `401` `{"error": "invalid_refresh_token"}` | Anything else: an unknown, revoked or expired token, a session past its absolute limit, or a deactivated account. Sign in again. |

Each refresh rotates the refresh token. Presenting a rotated token **more than 10 seconds** after
it was exchanged means two parties hold the session, so the whole session ends: every refresh
token it issued is revoked, the newest included. Other sessions of the same account are not
touched.

A refresh mints the account's tenant as it is now, and refuses a deactivated account. It never
asks for a second factor: the session already passed MFA when it was signed in.

`absolute_ttl_secs` counts from sign-in, not from the last refresh. Past it, refresh answers
`401` and the user signs in again.

## Logout

```http
POST /auth/v1/session/logout
Content-Type: application/json

{"refresh_token": "..."}
```

Logout revokes every refresh token of the session and answers `204` whether the token was known
or not, so the route says nothing about which tokens exist.

**Logout does not end an access token already issued, unless `[security.token_revocation]` is
configured.** The validator checks an access token's signature and expiry; it never reads the
session table. Without token revocation, an access token issued before logout keeps working
until it expires, up to `ttl_secs + 60` seconds. With token revocation, logout also denies
every access token the session issued, and a failure to do so answers `500` rather than a
`204` that would claim the session ended. A short `ttl_secs` bounds the exposure in both cases.

## Why the routes live under `/auth/v1/session/`

A browser client can keep the refresh token in an `HttpOnly` cookie scoped with
`Path=/auth/v1/session`. Both routes that consume it are under that path, so the browser sends
the cookie to them and to nothing else: not to `/graphql`, and not to the sign-in routes.

## Rate limits

The routes spend the existing per-client budgets in `[security.rate_limiting]`:
`auth_refresh_max_requests` / `auth_refresh_window_secs` for refresh, and
`auth_logout_max_requests` / `auth_logout_window_secs` for logout.

## The `_system.sessions` table

One row per refresh token. Its `expires_at` is when that refresh token stops working, and means
one of two things:

- For a session that cannot be refreshed, the access token's expiry, as before.
- For a refreshable session, the session's absolute limit, copied unchanged to every row its
  refreshes create. The access token's expiry is in the token, not in the table.

`SessionData::expires_at`, which `SessionStore::get_session` returns, carries the same value.

Rows that share a `family_id` belong to one session; `rotated_at` records when a row's token was
exchanged. Every server that mints sessions deletes rows past their `expires_at` every five
minutes. A rotated row is kept until then, so a reused token is still recognised as reuse.
