# The mail `[auth.local]` sends

`[auth.local]` sends three mails: the sign-in code (`otp`), the password-reset link (`reset`)
and the email-verification link (`verification`). Each is plain English text by default. Give
one a subject, a text body and an `HTML` body in your own language, and a sender name for all
of them:

```toml
# fraiseql.toml
[auth.local]
otp                 = true
email_from          = "support"
email_from_name     = "Équipe Exemple"
magic_link_template = "https://app.example.com/magic#token={token}"

[auth.local.email.otp]
subject       = "Votre code de connexion"
text_template = "templates/otp.txt"
html_template = "templates/otp.html"   # optional
```

```text
# templates/otp.txt
Votre code : {code}

Ou ouvrez ce lien : {link}

Il expire dans {expires_minutes} minutes.
```

`[auth.local.email.reset]` and `[auth.local.email.verification]` take the same keys. A mail
with no section keeps its built-in text. A section with only a `subject` changes the subject
and keeps the built-in body.

## Placeholders

| Placeholder         | `otp` | `reset` | `verification` | Value                                                                       |
|---------------------|:-----:|:-------:|:--------------:|-----------------------------------------------------------------------------|
| `{code}`            |   ✓   |         |                | The six-digit code.                                                         |
| `{link}`            |   ✓   |    ✓    |       ✓        | The link, built from `magic_link_template`, `reset_url_template` or `verification_url_template`. |
| `{expires_minutes}` |   ✓   |    ✓    |       ✓        | How long the code or link lasts: 10 for `otp`, 60 for the others.           |

- A sign-in body needs `{code}` or `{link}`. `{link}` needs a `magic_link_template`; see
  [magic-links.md](magic-links.md).
- A reset or verification body needs `{link}`.
- A name in braces that is not a placeholder of its mail, such as a misspelt `{cdoe}`, is
  refused. Braces around anything else are text, so `HTML` styles like `a {color: red}` are
  fine.
- Values are substituted in one pass: a value is never read for placeholders again.
- In the `HTML` body every value is escaped (`&` becomes `&amp;`, and so on), so a link
  stays intact inside `href="{link}"`. The text body gets the values as they are.

## The subject and the sender name

The subject is sent as written: it may not hold a placeholder, so a code never shows in a
notification or on a lock screen. The subject and `email_from_name` must each be one line.

## Compiling

`text_template` and `html_template` are paths relative to `fraiseql.toml`. `fraiseql compile`
reads the files into the compiled schema and drops the paths, so the server needs no access
to them. Change a template, compile again.

`fraiseql compile` refuses a file it cannot read, a body that breaks the rules above, an
`HTML` body without a text one, and a body written inline in `fraiseql.toml`. The server
checks the compiled schema with the same rules at boot.
