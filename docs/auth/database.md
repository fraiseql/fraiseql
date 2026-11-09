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

## Development: `fraiseql run --admin-database-url`

On a development database, `fraiseql run` can create both roles before it starts the server.
Give it a URL that can create roles, on the same database the server connects to:

```sh
export FRAISEQL_ENV=development
export DATABASE_URL="postgresql://app_engine:<password>@localhost:5432/app"
export AUTH_DATABASE_URL="postgresql://fraiseql_auth:<password>@localhost:5432/app"
fraiseql run --admin-database-url "postgresql://postgres:<password>@localhost:5432/app"
```

The flag also reads `FRAISEQL_ADMIN_DATABASE_URL`. Before serving, it:

- creates the engine's role, the user of the database URL, and the auth role, the user of the
  URL in the variable `[auth.database] url_env` names, each `LOGIN NOSUPERUSER` with its URL's
  password, when it does not exist;
- sets only the password of a role that exists, from its URL. A re-run never changes a role's
  other attributes;
- grants each role `CONNECT` and `CREATE` on the database, and the engine's role `CREATE` on
  schema `public`;
- leaves alone a URL whose user is the admin URL's user: the admin user is never changed, so a
  setup that runs everything as one superuser keeps working.

Without `[auth.database]`, only the engine's role is created. The flag is refused outside
`FRAISEQL_ENV=development`, and refused when the admin URL names another host, port or
database than the server's. Nothing it prints carries a URL or a password.

## Docker Compose

The canonical `docker-compose.yml` creates the auth role when `.env` sets `AUTH_DB_PASSWORD`:

```sh
# .env
AUTH_DB_PASSWORD=<password>
```

On every `docker compose up`, the one-shot `auth-role` service creates `fraiseql_auth` if the
database does not hold it yet, sets its password, and grants it `CONNECT` and `CREATE` on the
database. The server then starts with `AUTH_DATABASE_URL` naming that role, so the
`fraiseql.toml` the stack mounts says:

```toml
# fraiseql.toml
[auth.database]
url_env = "AUTH_DATABASE_URL"
```

Without `AUTH_DB_PASSWORD`, `AUTH_DATABASE_URL` is empty: a schema with no first-party auth
surface runs as before, and one with `[auth.database]` refuses to boot, naming the variable.

## Helm

The chart passes the auth role's URL to the server, as it does the engine's:

```sh
helm install app deploy/kubernetes/helm/fraiseql \
  --set database.existingSecret=app-db \
  --set authDatabase.existingSecret=app-auth-db
```

- `authDatabase.existingSecret` names a Secret you create; `authDatabase.existingSecretKey`
  (default `url`) names the key that holds the URL.
- `authDatabase.url` makes the chart create that Secret from the value. Like `database.url`, the
  value then lands in your values file and in Helm's release storage.
- `authDatabase.env` (default `AUTH_DATABASE_URL`) is the variable the URL is mounted as: the
  name `[auth.database] url_env` gives.

Setting both `url` and `existingSecret` is refused. The chart runs no database, so it creates
no role: create the auth role as [Setting up the role](#setting-up-the-role) shows.

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
