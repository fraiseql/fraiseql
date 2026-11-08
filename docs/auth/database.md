# The auth database role

First-party auth keeps its accounts, credentials and sessions in a schema of its own, `auth`,
and reaches it through a connection pool of its own, as a PostgreSQL role of its own. The
engine's role, which runs your queries and mutations, needs no grant on `auth`. The auth role
needs no grant on your application's schemas.

`[auth.database]` names that connection:

```toml
# fraiseql.toml
[auth.database]
url_env = "AUTH_DATABASE_URL"   # the environment variable that holds the auth role's URL
pool_max = 5                    # optional: the most connections the pool opens (default 5)
pool_min = 0                    # optional: connections kept open (default 0)
acquire_timeout_secs = 30       # optional: how long a request waits for one (default 30)
```

```sh
export AUTH_DATABASE_URL="postgresql://fraiseql_auth:<password>@db:5432/app"
```

The compiled schema carries the variable's name, never the URL. The server reads the URL at
boot, and its logs name the variable, never its value.

## When it is required

Every first-party auth surface stores something in `auth`, so each one needs
`[auth.database]`:

- `[auth.local]` (password, OTP, MFA, anonymous sign-up);
- `[auth.social]`;
- `[saml]`;
- `[scim]` with `enabled = true`.

A server with any of them and no `[auth.database]` refuses to boot, naming the surfaces. A
server with `[auth.database]` and none of them refuses too, because nothing would use it.
`[saml]` and `[scim]` are server settings, so a project that uses only those holds
`[auth.database]` alone under `[auth]`.

The boot also refuses:

- a `url_env` that is empty or is not an environment variable name (for example a URL pasted
  where the name goes);
- a variable that is unset or empty;
- a value that is not a PostgreSQL connection URL;
- `pool_max = 0`, and `pool_min` above `pool_max`.

The pool connects on first use. A database that refuses the auth role fails the boot when the
server creates auth's tables, before it listens.

## Setting up the role

The auth role needs to log in and to create schema `auth` in its database. It does not need
to be a superuser:

```sql
CREATE ROLE fraiseql_auth LOGIN PASSWORD '<password>' NOSUPERUSER NOBYPASSRLS;
GRANT CONNECT, CREATE ON DATABASE app TO fraiseql_auth;
```

At boot the server creates schema `auth` as that role, so the role owns the schema and every
table in it. Do not create schema `auth` yourself: the server marks the schema it creates, and
refuses to boot on a schema `auth` it did not create (below).

`[auth.database]` may name a different database from the engine's: auth has no foreign key
into the application's tables.

## An application schema named `auth`

If your database already has a schema named `auth` that FraiseQL did not create, the boot
stops before creating any table in it:

```text
schema "auth" exists and was not created by FraiseQL first-party auth: rename the
application's own schema "auth", or point [auth.database] at another database
```

FraiseQL recognises its own schema by the comment it writes on it,
`fraiseql: first-party auth`. A deployment that configures no first-party auth surface
never touches schema `auth`.
