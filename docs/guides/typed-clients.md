# Typed clients (TypeScript, Python, Go & Rust)

Frontend and backend-to-backend callers of a FraiseQL API can be fully typed from
the same `schema.compiled.json` the server runs on — no hand-maintained interfaces,
no silent drift.

```
fraiseql generate-client typescript \
  --schema ./schema.compiled.json \
  --out ./src/generated

fraiseql generate-client python \
  --schema ./schema.compiled.json \
  --out ./app/fraiseql_client

fraiseql generate-client go \
  --schema ./schema.compiled.json \
  --out ./internal/fraiseqlclient

fraiseql generate-client rust \
  --schema ./schema.compiled.json \
  --out ./src/generated
```

`--schema` is auto-detected from conventional locations (`schema.compiled.json`,
`target/fraiseql/schema.compiled.json`, `build/schema.compiled.json`) when omitted.
The command refuses to overwrite an existing generated client without `--force`.

> **`generate-client` is not `generate`.** `fraiseql generate <language>` emits
> server-side **authoring** code — FraiseQL type/query definitions in another
> language, fed back into the compiler. `generate-client` is the inverse: it
> consumes the compiler's *output* to build a client that *calls* your API.

## What you get

| File | Contents |
|---|---|
| `types.ts` | every object/interface type as a TS interface; union aliases; relay `Connection<T>` |
| `enums.ts` | GraphQL enums as string-union types |
| `inputs.ts` | input objects as TS interfaces (only if your schema has any) |
| `queries.ts` | typed query functions, each embedding its GraphQL document |
| `mutations.ts` | typed mutation functions + `isErrorResult` guard (only if any mutations) |
| `relationships.ts` | relationship metadata for generic UI/tooling (only if any) |
| `client.ts` | a tiny `FraiseqlClient` wrapping `fetch` — zero runtime dependencies |
| `index.ts` | re-exports everything that was generated |

## Use

```typescript
import {
  FraiseqlClient,
  getUser,
  createUser,
  isErrorResult,
} from "./generated";

const client = new FraiseqlClient({ endpoint: "https://api.example.com/graphql" });

const user = await getUser(client, { id: "abc" });
// user: User | null — fully typed

const result = await createUser(client, { input: { email: "a@b.c", role: "EDITOR" } });
if (isErrorResult(result)) {
  console.error(result.status, result.message); // narrowed to the error member
} else {
  console.log(result.id);                         // narrowed to User
}
```

Need auth headers? Pass static headers or a function (re-evaluated per request):

```typescript
const client = new FraiseqlClient({
  endpoint: "https://api.example.com/graphql",
  headers: () => ({ authorization: `Bearer ${getToken()}` }),
});
```

## Python

The Python client mirrors the TypeScript one file-for-file — same modules, same
operations, and **byte-identical GraphQL documents** (both generators share one
document builder, pinned by a cross-language test). It targets **Python ≥ 3.12**
and has zero dependencies beyond the standard library (`urllib` transport).

| File | Contents |
|---|---|
| `types.py` | object/interface types as `TypedDict`s; PEP 695 union aliases; relay `Connection[T]` |
| `enums.py` | GraphQL enums as `Literal` aliases |
| `inputs.py` | input objects as `TypedDict`s (`NotRequired` for optional fields) |
| `queries.py` / `mutations.py` | typed operation functions + `is_error_result` |
| `client.py` | `FraiseqlClient` over `urllib` — subclass and override `request` for httpx/async |
| `__init__.py` | re-exports everything that was generated |

```python
from fraiseql_client import FraiseqlClient, getUser, createUser

client = FraiseqlClient(
    "https://api.example.com/graphql",
    headers=lambda: {"authorization": f"Bearer {get_token()}"},
)

user = getUser(client, id="abc")          # User | None — fully typed
result = createUser(client, input={"email": "a@b.c", "role": "EDITOR"})
if result["__typename"] == "EmailTakenError":   # TypedDict-union narrowing
    print(result["status"])
else:
    print(result["id"])
```

Two Python-specific notes:

- **Narrow result unions on `__typename`**, as above — that is the discriminant
  type checkers narrow `TypedDict` unions by. `is_error_result(result)` is the
  runtime convenience; it returns a plain `bool` and does not narrow (the
  standard library has no `TypeIs` on 3.12).
- **Optional arguments default to `None` and are omitted from the request**, so
  the server applies its own defaults. To send an explicit JSON `null`, call
  `client.request(document, variables)` directly.

## Go

One package (`fraiseqlclient`), Go ≥ 1.21 (the Relay `Connection[T]` needs
generics), and nothing outside the standard library — the transport is
`net/http`.

| File | Contents |
|---|---|
| `types.go` | object/interface types as structs with `json` tags; unions; relay `Connection[T]` |
| `enums.go` | GraphQL enums as defined string types plus one constant per value |
| `inputs.go` | input objects as structs (`*T` + `omitempty` for optional fields) |
| `queries.go` / `mutations.go` | operations as methods on `*Client`, plus `IsErrorResult` |
| `client.go` | `Client` over `net/http`, with a per-request `Headers` hook |

```go
c := fraiseqlclient.NewClient("https://api.example.com/graphql")
c.Headers = func() map[string]string {
    return map[string]string{"authorization": "Bearer " + token()}
}

user, err := c.GetUser("abc")          // *User — nil when the server returns null

created, err := c.CreateUser(fraiseqlclient.CreateUserInput{
    Email: "a@b.c",
    Role:  fraiseqlclient.UserRoleEditor,
})
switch {
case created.EmailTakenError != nil:   // exactly one member pointer is set
    log.Println(created.EmailTakenError.Message)
case created.User != nil:
    log.Println(created.User.Id)
}
```

Three Go-specific notes:

- **Operations are methods on `*Client`, not package functions.** Go has one
  exported namespace per package, and the canonical schema puts a `user` query
  next to a `User` type — as functions the two would be the same identifier. A
  method lives in the receiver's namespace, so both fit.
- **A union is a struct with one pointer per member** and a generated
  `UnmarshalJSON` that fills the member its `__typename` names. Go has no sum
  type; flattening the members into one struct would lose which one arrived,
  which is the only thing a union is for. A `__typename` outside the union's
  members is an error, not a zero value.
- **Optional arguments are nilable and omitted when nil.** They are checked at
  the typed parameter, before being boxed — a typed nil inside an `any` is not
  `== nil`, so filtering the map afterwards would send `null` for every unset
  argument.

## Rust

A module tree you drop into your crate next to a `pub mod generated;`. It depends
on `serde` (with `derive`) and `serde_json`, and on nothing else.

| File | Contents |
|---|---|
| `types.rs` | object/interface types as structs; unions as `#[serde(tag = "__typename")]` enums; relay `Connection<T>` |
| `enums.rs` | GraphQL enums as unit-variant enums with `#[serde(rename)]` |
| `inputs.rs` | input objects as structs (`Option<T>` + `skip_serializing_if` for optional fields) |
| `queries.rs` / `mutations.rs` | typed operation functions + `is_error_typename` |
| `client.rs` | `FraiseqlClient<T: Transport>`, the `Transport` trait, and `Error` |
| `mod.rs` | submodule declarations and the data-type re-exports |

```rust
use generated::{CreateUserInput, CreateUserResult, Error, FraiseqlClient, UserRole, mutations, queries};

// std has no HTTP client, so the transport is yours — any
// `Fn(&str) -> Result<String, Error>` is one.
let client = FraiseqlClient::new(|body: &str| {
    ureq::post(ENDPOINT)
        .set("content-type", "application/json")
        .send_string(body)
        .and_then(|r| Ok(r.into_string()?))
        .map_err(|e| Error::transport(e.to_string()))
});

let user = queries::get_user(&client, "abc".to_string())?;   // Option<User>

match mutations::create_user(&client, CreateUserInput {
    email: "a@b.c".to_string(),
    display_name: None,
    role: UserRole::Editor,
})? {
    CreateUserResult::User(user) => println!("{}", user.id),
    CreateUserResult::EmailTakenError(error) => eprintln!("{}", error.message),
}
```

Three Rust-specific notes:

- **The transport is a trait, not a built-in HTTP client.** Rust's standard
  library has none, and a generated module is the wrong place to pick one for
  you — `reqwest`, `ureq`, blocking or async are all five lines away.
- **Operations stay under `queries::` / `mutations::`.** `GraphQL` lets a query
  and a mutation share a name; re-exporting both at the top level would hand you
  an ambiguity at your own use site.
- **Descriptions are `//` comments, not `///` doc comments.** A doc comment is
  markdown, and a fenced block inside one becomes a doctest your `cargo test`
  would try to compile — schema-author text must never escape into code.

## How the types are designed (and why)

**Result types are selection-scoped, not schema-mirrors.** Each generated default
document selects only the *leaf* fields of its return type — scalars, enums, and
`__typename`. The matching TypeScript interface therefore contains exactly those
fields. Relationship fields (e.g. `User.posts`, `Post.author`) are **not** part of
the generated type, because the default document does not fetch them. This is
deliberate: a type that claimed `user.posts` while the request never asked for it
would be a lie the compiler can't catch — and a trap for both developers and AI
coding agents. If you need nested data today, pass your own document to
`client.request(...)`; first-class nested selection is a planned follow-up.

**Mutations are result unions discriminated by `__typename`.** FraiseQL mutations
return their declared type directly — typically a union of the success entity and
one or more `@fraiseql.error` types. The generated client mirrors that: the return
type is the union, error types carry an injected `status` field, and `isErrorResult`
narrows the union to its error members. There is no synthetic `{ succeeded, data }`
envelope, because the server does not send one.

For this discrimination to work, the **compiled schema must actually contain the
union and the error type** — the server resolves success vs. error by matching the
`app.mutation_response` row against the mutation's union members (`succeeded` →
the success member; an error → the union's `@fraiseql.error` member). A mutation
that returns only the bare success type (no union) has nothing to discriminate
against, so a failed row is mapped onto the success type. There are two ways to
get the union into the schema:

*Author it explicitly.* Note that a Python union return annotation
(`-> Order | OrderError`) is **rejected** by the SDK — declare a `@fraiseql.union`
marker class and use it as the return type:

```python
@fraiseql.type
class Order: ...

@fraiseql.error            # is_error: true; fields populated from mutation_response
class MutationError:
    message: str
    status: str | None      # the server injects this from error_class

@fraiseql.union(name="CreateOrderResult", members=[Order, MutationError])
class CreateOrderResult: ...   # marker class — body ignored

@fraiseql.mutation(sql_source="app.create_order")
def createOrder(input: CreateOrderInput) -> CreateOrderResult: ...
```

*Or synthesize it automatically.* Set `auto_error_union` in `fraiseql.toml` and the
compiler generates a shared `MutationError` type and a per-mutation
`<Mutation>Result` union for every object-returning mutation, rewriting its return
type to that union:

```toml
[fraiseql.mutations]
auto_error_union = true
```

The synthesized `MutationError` exposes `status` (the error-class discriminator),
`message`, `httpStatus`, and `errorClass`, all populated from the
`app.mutation_response` composite at runtime. Mutations that already return a union,
and those returning a scalar/enum, are left untouched — explicit declarations always
win — and an existing type name is never overwritten.

## CI staleness check

Every generated file is stamped with a hash of the schema it came from:

```typescript
// AUTO-GENERATED by fraiseql-codegen. DO NOT EDIT.
// schema-hash: 7a4b1e9c3f...
// fraiseql-codegen: 2.3.2
```

In CI, recompute the live schema's hash and compare against the stamp; a mismatch
means someone changed the schema without regenerating the client. The hash is a
canonical (recursively key-sorted) `sha256` of the compiled schema, so it is stable
across serializer settings.

## Limitations (v1)

- **Four target languages.** All four share one document core, pinned by a
  cross-language test; a fifth would plug into the same seam.
- **Scalar-default documents.** Nested relationship fields are not auto-selected;
  pass a custom document to `client.request` for deep fetches. Bounded-depth
  expansion / a selection builder is a follow-up.
- **`auto_params` (where/orderBy/limit/offset)** are not yet rendered as typed
  arguments — their names depend on the schema's `naming_convention`. Relay
  queries expose forward pagination (`first`/`after`).
- **No subscriptions client yet.** The server speaks WebSocket
  (`graphql-transport-ws`); a generated WebSocket helper is a follow-up.
- **Custom scalars** map to `string` with a `// TODO: brand` note; brand them with
  `zod`/`io-ts` downstream if you need refinement.
- **No normalised cache or framework adapters** — the generated client is
  framework-agnostic; layer Apollo Client / urql / React Query on top if needed.
