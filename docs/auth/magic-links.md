# Magic links

With `[auth.local] otp = true`, a sign-in mail carries a six-digit code. Give it a
`magic_link_template` and the mail also carries a link that signs in on its own, from any
device, without the address being typed again:

```toml
# fraiseql.toml
[auth.local]
otp                 = true
email_from          = "support"
magic_link_template = "https://app.example.com/magic#token={token}"
```

`{token}` is replaced by a link token: 256 random bits, base64url, 43 characters. The mail
reads:

```text
Use the link below to sign in. It expires in 10 minutes and can only be used once.

https://app.example.com/magic#token=...

Or enter this code: 123456
```

## Redeeming a link

The page the link opens sends the token to FraiseQL:

```http
POST /auth/v1/verify
Content-Type: application/json

{"token": "..."}
```

The answer is the one a correct code gets: a session, or a second-factor challenge for an
account with `MFA`. `POST /auth/v1/verify` takes either `{"token"}` or `{"email", "code"}`.
Both together, or neither, answers 422 `invalid_request`.

## One credential

The code and the link token in a mail are one credential:

- redeeming either spends both;
- a resend replaces both, so only the newest mail works;
- both expire 10 minutes after the send;
- an unknown, spent, replaced or expired token gets the answer a wrong code gets, 422
  `invalid_otp`.

With `otp_allowed_query` set, a link is checked against the list when it is redeemed, as a
code is, so removing an address invalidates a link it already holds (see
[otp-allow-list.md](otp-allow-list.md)).

The database keeps only the SHA-256 of the code and of the token, so a read of
`auth.tb_otp_code` cannot replay a link.

## Writing the landing page

Mail scanners and link previews open links before people do. Only a `POST` redeems: a `GET`
to `/auth/v1/verify` is refused, so a scanner that fetches the link spends nothing. That
protection holds only if the landing page needs a person, and only that person can tell
whether the account is theirs:

- **Require a click.** Show a "Sign in" button that sends the `POST`. Do not post on page
  load, even with the token already at hand: a scanner that runs scripts would spend the
  link, and the person would get a dead one, and a person who opens someone else's link would
  be signed in before reading anything.
- **Keep the token out of the request line.** Put it in the fragment (`#token={token}`), as
  above. A fragment is never sent to a server, so it does not reach access logs, proxies or
  the `Referer` header. With a query string (`?token={token}`), also send
  `Referrer-Policy: no-referrer` from the landing page.
- **Show the account, and require a second click to continue.** A link signs in to the
  account of whoever asked for it, and that may not be the person who opens it: anyone can
  request a link for their own address and send it on. So after the redeem, show the address
  of the account the session belongs to, "Signed in as alice@example.com", and enter the app
  only when the person clicks "Continue", with "Not you? Sign out" beside it. The address comes
  from your own API, for example your `me` query, read with the new session: the answer to
  `POST /auth/v1/verify` names no account, and an address carried in the link cannot stand
  in for it, because whoever sends the link writes it. "Sign out" ends the session with
  `POST /auth/v1/session/logout` ([sessions.md](sessions.md#logout)), or, where refresh is
  not configured, by dropping the tokens.

To write the mail itself in another language, with the link inside, see
[mail-templates.md](mail-templates.md).

## A template with only `{code}`

A template may carry `{code}` instead of, or beside, `{token}`. A code alone needs the
address as well, so such a link signs in only where the page already knows the address,
typically the device that asked for the mail. `fraiseql compile` warns when the template has
`{code}` and no `{token}`, and refuses a template with neither.
