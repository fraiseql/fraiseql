# GraphQL conformance

FraiseQL compiles a schema into SQL over database views. It has no arbitrary resolvers,
so it can reasonably decline parts of GraphQL. What it may not do is answer a document
differently from the specification while presenting itself as a GraphQL server.

Every document sent to FraiseQL therefore ends in exactly one of two outcomes:

1. **The answer graphql-js gives**: the same `data`, the same presence of `errors`, and
   the same error `path` and `locations`. graphql-js is the reference implementation.
2. **A refusal naming a restriction listed below.** The response has no `data`, and every
   error carries the restriction's code in `extensions.code`.

There is no third outcome. Answering differently is a defect, and so is executing a
document that graphql-js rejects. Neither is ever documented here as a restriction.

The differential oracle in `tools/oracle/graphql-js` enforces this. It reads the table
below and fails when a refusal carries a code the table does not list, and when an entry
lists a refusal the server does not produce.

## Never restricted

These must match graphql-js exactly. None of them can appear in the table below.

- **Validation** (spec §5): every rule, for every document. A document graphql-js
  rejects is rejected, before any execution.
- **Introspection** (§4): `__schema`, `__type` and `__typename` answer as graphql-js
  answers over the same SDL. This holds wherever introspection is enabled; turning it off
  is policy, and refuses with `INTROSPECTION_DISABLED`.
- **Field collection and merging** (§5.3.2, §6.3.2): aliases, fragments, inline fragments.
- **`@skip` and `@include`** (§3.13).
- **Coercion** (§3.5, §5.6, §6.1.2): variable coercion, input coercion, and list input
  coercion. A custom scalar coerces as its `@specifiedBy` reference defines, and a value
  outside that definition is a coercion error, as the spec requires. Such a value is not
  a restriction.
- **Null propagation** (§6.4.4).
- **The error format** (§7.1.2): `message`, `locations`, `path`, and the code in
  `extensions.code`. No other top-level key.

## Restrictions

| Id | Refuses | `extensions.code` |
|---|---|---|
| `depth-limit` | an operation nested deeper than `[validation] max_query_depth` | `LIMIT_EXCEEDED` |
| `complexity-limit` | an operation whose complexity exceeds `[validation] max_query_complexity` | `LIMIT_EXCEEDED` |
| `alias-limit` | an operation with more aliases than the configured maximum | `LIMIT_EXCEEDED` |
| `page-size-cap` | a `limit` above the maximum page size | `LIMIT_EXCEEDED` |
| `non-negative-pagination` | a negative `limit` or `offset` | `ARGUMENT_OUT_OF_RANGE` |
| `no-batching` | a request body that is a JSON array of operations | `UNSUPPORTED_FEATURE` |
| `trusted-documents` | a raw document, or an unknown document id, in trusted-documents strict mode | `FORBIDDEN_QUERY`, `DOCUMENT_NOT_FOUND` |
| `subscription-subset` | a subscription over a transport other than WebSocket, or outside the shape described below | `UNSUPPORTED_FEATURE`, `SUBSCRIPTION_REFUSED` |

A `LIMIT_EXCEEDED` error names the limit in `extensions.limit` (`depth`, `complexity`,
`aliases` or `page_size`), along with `extensions.max` and `extensions.actual`.

### depth-limit

**Reason.** Each nesting level of a query can become a join or a nested aggregation, so
depth bounds the SQL a document can generate. A server-side ceiling is a deliberate
policy. The spec allows a service to refuse a document by policy.

**Refused today at** `crates/fraiseql-server/src/routes/graphql/handler/stages.rs:276`,
as `VALIDATION_ERROR` ("Query exceeds maximum depth: N > M").

**Status.** Refused, but under the code shared with spec validation failures, so a client
cannot tell a policy refusal from an invalid document. It becomes a restriction when it
carries `LIMIT_EXCEEDED`.

### complexity-limit

**Reason.** It bounds the work an operation can request in one round trip.

**Refused today at** `crates/fraiseql-server/src/routes/graphql/handler/stages.rs:282`,
as `VALIDATION_ERROR`.

**Status.** Same as `depth-limit`.

### alias-limit

**Reason.** Aliases multiply the root fields a single document executes, so they are
bounded separately from depth.

**Refused today at** `crates/fraiseql-server/src/routes/graphql/handler/stages.rs:295`,
as `VALIDATION_ERROR`.

**Status.** Same as `depth-limit`.

### page-size-cap

**Reason.** One request may not read an unbounded number of rows.

**Refused today at** `crates/fraiseql-core/src/runtime/executor/runners/query_params.rs:726`
(`enforce_max_page_size`), as `VALIDATION_ERROR`. REST refuses the same thing at
`crates/fraiseql-server/src/routes/rest/params/mod.rs:893`.

**Status.** Same as `depth-limit`.

### non-negative-pagination

**Reason.** `limit` and `offset` are FraiseQL-defined arguments, and a negative value has
no meaning for them. GraphQL has no unsigned integer type, so the schema cannot express
the domain. FraiseQL refuses the value rather than returning an unpaginated result.

**Refused today at** `crates/fraiseql-core/src/runtime/executor/runners/query_params.rs:711`,
as `VALIDATION_ERROR`.

**Status.** Same as `depth-limit`, with `ARGUMENT_OUT_OF_RANGE`.

### no-batching

**Reason.** Batching is not part of the GraphQL-over-HTTP specification. It would also
let one request multiply what the per-request limits above allow.

**Refused today at** `crates/fraiseql-server/src/routes/graphql/handler.rs:59`, where the
JSON body extractor fails with 422 and a plain-text body that is not a GraphQL response.

**Status.** Not refused cleanly. It needs a GraphQL error response carrying
`UNSUPPORTED_FEATURE`.

### trusted-documents

**Reason.** In strict mode the operator lets only reviewed operations run.

**Refused today at** `crates/fraiseql-server/src/routes/graphql/handler/stages.rs:151`
(`FORBIDDEN_QUERY`) and `:155` (`DOCUMENT_NOT_FOUND`).

**Status.** Refused, with distinct codes. The codes are currently a top-level `code` key;
they move to `extensions.code` with the error format above.

### subscription-subset

**Reason.** A subscription is a read of one type, pushed from that type's change log. It
is not a resolver stream.

The supported subset:

- **Transport.** WebSocket only, with `graphql-transport-ws` or `graphql-ws`, and only
  where subscriptions are enabled. A subscription operation sent over HTTP POST, GET or
  SSE is refused with `UNSUPPORTED_FEATURE`.
- **One operation.** Each subscribe message carries exactly one subscription operation.
- **Selection.** At the top level, only the fields in the subscription's compiled field
  list. Below it, any field the subscribed type declares.
- **Delivery.** Each event serves the write's after-image, and nothing is fetched. An
  event the subscriber may not read is never sent. No error frame is sent for it.

A shape outside this subset is refused with `SUBSCRIPTION_REFUSED`.

The following are spec validation, not restrictions. Each answers as a validation error,
under "Never restricted" above:

- **More than one root field.** Spec rule §5.2.3.1. It is counted after `@skip`/`@include`
  are evaluated and fragments are expanded, as the spec counts it.
- **A subscription field the schema does not declare** (§5.3.1).
- **A field the subscribed type does not declare** (§5.3.1).

**Refused today at** `crates/fraiseql-core/src/runtime/executor/subscription_plan.rs:400`
for a field outside the compiled list. It carries `SUBSCRIPTION_REFUSED`, which
`crates/fraiseql-server/src/routes/subscriptions.rs:1520` selects.

**Status.**

- The refusal of a field outside the compiled list is clean: it carries a distinct code.
- An unknown subscription answers as validation (`VALIDATION_ERROR`, from
  `subscription_plan.rs:221`).
- Two of the validation cases do not yet answer as validation:
  - Before planning, `crates/fraiseql-server/src/routes/subscriptions.rs:1879` refuses a
    second root field with `PARSE_ERROR`. It counts before directives are evaluated.
  - A field the type does not declare answers `SUBSCRIPTION_REFUSED` from
    `subscription_plan.rs:369`. It should answer as the §5.3.1 validation error it is.
- Over HTTP, a subscription operation is not refused as such. It reaches the query
  lookup and fails as `VALIDATION_ERROR`. It needs an explicit `UNSUPPORTED_FEATURE`
  refusal before execution.

## Transports

FraiseQL serves the same data over POST, GET and SSE on `/graphql`, over WebSocket
subscriptions, REST, MCP and Arrow Flight. On every transport, a request gets the answer
`POST /graphql` gives it, or a difference listed in the table below. POST is the
reference because the outcomes above define it.

Every other difference between a transport and POST is a defect.

The cross-transport oracle in `tools/oracle/transports` enforces this. It reads the table
below and counts a cell that matches a listed difference as documented. It also probes
each entry on both sides of its boundary. It fails when an entry stops holding, and when
a cell differs from POST in a way the table does not list.

| Id | Transport | Difference from POST |
|---|---|---|
| `flight-requires-auth` | Arrow Flight | an unauthenticated call is refused with gRPC `UNAUTHENTICATED`, whatever it asks |
| `mcp-requires-auth` | MCP | with `[mcp] require_auth = true`, an unauthenticated call is refused with HTTP 401 |
| `url-length-cap` | GET `/graphql`, REST | a request target longer than 65 534 bytes is refused with HTTP 414 |
| `rest-filter-cap` | REST | a `filter` parameter longer than 4096 bytes is refused with HTTP 400 `BAD_REQUEST` |
| `flight-ticket-cap` | Arrow Flight | a ticket larger than 256 KiB is refused with gRPC `INVALID_ARGUMENT` |
| `mcp-field-omission` | MCP | a field the caller may not read is left out of the rows, where POST refuses the request |

### flight-requires-auth

**Reason.** A Flight stream is a bulk export channel. It serves authenticated callers
only, so the server checks the token before it reads the ticket.

**Boundary.** The same ticket is served with a valid token, and refused without one.

### mcp-requires-auth

**Reason.** An MCP client is an agent acting for a user. `require_auth` makes that user
explicit before any tool runs.

**Boundary.** The same tool call is served with a valid token, and refused without one.

### url-length-cap

**Reason.** GET and REST carry the request in the URL. The HTTP server limits a request
target to 65 534 bytes. A larger document goes by POST, which has no such cap.

**Boundary.** A request target of 65 534 bytes is served, and one of 65 535 bytes is
refused with 414.

### rest-filter-cap

**Reason.** The REST `filter` parameter is parsed before planning. 4096 bytes bounds that
parse. A larger filter goes by POST `/graphql`, which depth and complexity bound instead.
The cap is checked before field authorisation, so a request that POST refuses for a field
it may not read is refused here with `BAD_REQUEST`.

**Boundary.** A `filter` of 4096 bytes is served, and one of 4097 bytes is refused.

### flight-ticket-cap

**Reason.** The ticket is decoded before anything else. 256 KiB bounds that decode. Above
gRPC's 4 MiB message limit, gRPC itself refuses the call first, with `OUT_OF_RANGE`.

**Boundary.** A ticket of 256 KiB is served, and one a byte larger is refused.

### mcp-field-omission

**Reason.** An MCP tool call names no selection. It returns the fields the caller may
read. POST refuses a selection that names a field the caller may not read; MCP never
names one. This row covers refused fields only: a field POST serves null
(`on_deny = "mask"`) must be served null on MCP too.

**Boundary.** A caller without the field's scope gets rows without the field. A caller
with the scope gets rows with it.

### Error carriers

Each transport carries a refusal's code in its own place:

| Transport | Carrier |
|---|---|
| POST, GET, SSE | `errors[].extensions.code` |
| WebSocket | an `error` frame's `errors[].extensions.code`, or a close code: 4400 bad request, 4401 unauthorised, 4403 forbidden |
| REST | the HTTP status, and `error.code` in the body |
| Arrow Flight | the gRPC status |
| MCP | the tool result's error |

The error format of "Never restricted" applies to the GraphQL transports: POST, GET, SSE,
and WebSocket `error` frames.

## Changing this document

An entry has to argue for its place, and it is never a place to put a defect that has not
been fixed yet. Adding, removing or changing an entry changes what the oracle accepts, so
it is reviewed as a change to the conformance contract.
