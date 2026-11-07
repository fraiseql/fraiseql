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

In [cookie transport](#cookie-transport) the refresh token is in an `HttpOnly` cookie scoped with
`Path=/auth/v1/session`. Both routes that consume it are under that path, so the browser sends
the cookie to them and to nothing else: not to `/graphql`, and not to the sign-in routes.

## Cookie transport

By default every route above, and every sign-in, hands the tokens to the client in the `JSON`
body, and the client sends the access token as a bearer token. A browser application then keeps
both tokens where its own scripts can read them. `[auth.local] session_transport = "cookie"`
keeps them in `HttpOnly` cookies instead:

```toml
# fraiseql.toml
[auth.local]
password          = true
session_transport = "cookie"   # default: "json"

[auth.session]
ttl_secs          = 900
absolute_ttl_secs = 2592000    # required by cookie transport
```

| Cookie | Holds | Attributes |
|--------|-------|------------|
| `__Host-access_token` | the access token | `Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=` the token's lifetime |
| `__Secure-refresh_token` | the refresh token | `Path=/auth/v1/session; HttpOnly; Secure; SameSite=Strict; Max-Age=absolute_ttl_secs` |

In cookie transport:

- **Sign-in** (password signup and login, email `OTP` and magic link, `MFA` verify, anonymous
  sign-up) sets both cookies. The body is `{"expires_in": …}`, plus `user_id` for anonymous
  sign-up, and carries no token. An `MFA` challenge is not a session, and sets no cookie.
- **Refresh** takes `{}` as its body, reads the refresh token from its cookie, and answers `200`
  with the next pair in the cookies and `{"expires_in": …}` as the body. The `409` and `401`
  answers set no cookie. A refresh token sent in the body is still accepted, and answered in
  the body.
- **Logout** takes `{}`, ends the session the refresh cookie names, and clears both cookies.
- **The access cookie is a credential** on `/graphql` (`POST`, `GET` and `QUERY`),
  `/operations/v1/…`, and `/auth/v1/mfa/{enroll,confirm,unenroll}`. A bearer token in the
  `Authorization` header still wins over the cookie. The subscription socket (`/ws` by default), the `REST`
  transport and `/auth/v1/email/verify/{start,confirm}` do not accept the cookie: send them a
  bearer token.

The compiler and the server's boot refuse cookie transport without `[auth.session]
absolute_ttl_secs`, because that is what mounts the logout route, and nothing else can clear an
`HttpOnly` cookie. The boot also refuses it with `require_json_content_type = false`.

The `Secure` attribute means a browser stores the cookies only over `HTTPS`, or from
`http://localhost`.

### What stops another site using the cookies

A browser attaches cookies to requests that other pages make it send. Cookie transport relies on
three things to keep those requests from acting with the session:

- **`SameSite=Strict`.** A request that another site starts carries neither cookie. A
  page on another host on the same site (the same registrable domain) is exempt, though, and its
  requests do carry them.
- **Only `JSON` bodies.** A form, the one cross-origin request that a page can send without
  the browser asking the server first, cannot send `Content-Type: application/json`. `/graphql`
  refuses a `POST` that is not `JSON` (`415`), and so do refresh, logout, async-operation
  submission and the `MFA` routes. The other routes that accept the access cookie only read
  (`GET` and `QUERY` on `/graphql` never run a mutation), or use a method a form cannot send.
- **No credentialed cross-origin requests.** A `JSON` request from another origin is preceded by
  a preflight, and the server never answers it with `Access-Control-Allow-Credentials`, so the
  browser does not send the request with the cookies.

The routes that do not accept the access cookie are the ones where one of these does not hold:
a socket handshake has no body to check, a `REST` write reads its body whatever its content
type, and `email/verify/start` has no body at all (`email/verify/confirm` shares its mount).

### The same cookie under OIDC

With `[auth]` (OIDC), the sign-in callback sets the same `__Host-access_token` cookie, and the
same rule decides where it is a credential. The OIDC layer accepts it on `/graphql`,
`/operations/v1/…`, the playground, `GET /auth/me`, introspection, schema export and metadata,
and on `POST /auth/revoke`, `/auth/revoke-all` and the design audit, whose bodies are `JSON`. The
subscription socket, the `REST` transport and the observer admin endpoints (`/api/observers/…`)
do not accept it. Send them `Authorization: Bearer`.

## Rate limits

The routes spend the existing per-client budgets in `[security.rate_limiting]`:
`auth_refresh_max_requests` / `auth_refresh_window_secs` for refresh, and
`auth_logout_max_requests` / `auth_logout_window_secs` for logout.

## The `auth.sessions` table

One row per refresh token. Its `expires_at` is when that refresh token stops working, and means
one of two things:

- For a session that cannot be refreshed, the access token's expiry, as before.
- For a refreshable session, the session's absolute limit, copied unchanged to every row its
  refreshes create. The access token's expiry is in the token, not in the table.

`SessionData::expires_at`, which `SessionStore::get_session` returns, carries the same value.

Rows that share a `family_id` belong to one session; `rotated_at` records when a row's token was
exchanged. Every server that mints sessions deletes rows past their `expires_at` every five
minutes. A rotated row is kept until then, so a reused token is still recognised as reuse.
