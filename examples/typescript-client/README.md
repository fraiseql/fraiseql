# Example: typed TypeScript client

A minimal, runnable consumer that generates a typed client from a compiled FraiseQL
schema and uses it. See [`docs/guides/typed-clients.md`](../../docs/guides/typed-clients.md)
for the full guide.

This example ships a ready-made `schema.compiled.json` (a small blog schema:
users, posts, comments, a `CreateUserResult` error union, and a relay connection),
so you can try the generator without compiling or running a database.

## Walkthrough

```sh
# 1. Generate the client from the compiled schema into src/generated/
make codegen

# 2. Type-check the example against the generated client
npm install      # one-time: installs TypeScript
make typecheck
```

`make codegen` runs:

```sh
fraiseql generate-client typescript --schema schema.compiled.json --out src/generated --force
```

(The Makefile uses `cargo run -p fraiseql-cli --` so it works from a checkout
without installing the binary.)

## What to try

Open [`src/main.ts`](src/main.ts). It constructs a `FraiseqlClient` and calls every
operation — note how:

- `getUser` returns `User | null`, and `User` contains only the **leaf** fields the
  default document fetches (`tenant`/`posts` are intentionally absent).
- `createUser` returns the `CreateUserResult` union; `isErrorResult` narrows it to
  the `EmailTakenError` member (with its injected `status`).
- `postsConnection` returns a relay `Connection<Post>`.

Then edit `schema.compiled.json` (e.g. add a field to `User`), re-run `make codegen`,
and watch the generated types — and any now-incorrect code in `main.ts` — update.
The `schema-hash` header in each generated file changes too, which is how a CI
staleness check detects drift.

## Files

```
schema.compiled.json   # input (normally produced by `fraiseql compile`)
src/main.ts            # the consumer code
src/generated/         # produced by `make codegen` (git-ignored)
package.json           # TypeScript devDependency + scripts
tsconfig.json          # strict, noUncheckedIndexedAccess, Bundler resolution
Makefile
```
