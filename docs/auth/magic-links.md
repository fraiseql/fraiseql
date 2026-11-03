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
`core.tb_otp_code` cannot replay a link.

## Writing the landing page

Mail scanners and link previews open links before people do. Only a `POST` redeems: a `GET`
to `/auth/v1/verify` is refused, so a scanner that fetches the link spends nothing. That
protection holds only if the landing page needs a person:

- **Require a click.** Show a "Sign in" button that sends the `POST`. Do not post on page
  load: a scanner that runs scripts would spend the link, and the person would get a dead
  one.
- **Keep the token out of the request line.** Put it in the fragment (`#token={token}`), as
  above. A fragment is never sent to a server, so it does not reach access logs, proxies or
  the `Referer` header. With a query string (`?token={token}`), also send
  `Referrer-Policy: no-referrer` from the landing page.
- **Show which account signed in.** Anyone can mail you a link to *their* account. Saying
  "Signed in as alice@example.com" after the redeem lets a person notice they are in someone
  else's account.

## A template with only `{code}`

A template may carry `{code}` instead of, or beside, `{token}`. A code alone needs the
address as well, so such a link signs in only where the page already knows the address,
typically the device that asked for the mail. `fraiseql compile` warns when the template has
`{code}` and no `{token}`, and refuses a template with neither.
