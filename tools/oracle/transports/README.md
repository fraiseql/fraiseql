# Cross-transport differential oracle

FraiseQL serves the same data over several transports. One logical request (a root, a
selection, a filter, an order, a page and a principal) must get the same answer on each
of them. This oracle sends each request over every transport it has a
spelling on, and compares every cell with `POST /graphql`.

`POST /graphql` is the reference column. The [graphql-js oracle](../graphql-js/README.md)
grades it against the GraphQL reference implementation, so agreement with POST is
agreement with the spec, transitively.

## Columns

| Column | Client |
|---|---|
| POST | `POST /graphql` (the reference) |
| GET | `GET /graphql?query=` |
| SSE | `POST /graphql` asking for `text/event-stream`; `next` events and `@stream` items merged |
| REST | `GET /rest/v1/<resource>` with `select`, `filter`, `sort`, `limit` and `offset` |
| MCP | `tools/call` over streamable HTTP on `/mcp` |
| FLIGHT | Arrow Flight `do_get` with a `GraphQLQuery` ticket; the request's headers travel as gRPC metadata |
| WS | `subscription { orderCreated { … } }` over graphql-transport-ws on `/ws` (see [The WS column](#the-ws-column)) |
| GRPC | pending: no cells until it is mounted |

A request with no spelling on a transport has no cell there. GraphQL-only selections
(directives, aliases, `__typename`, nested objects) have none on REST or MCP.
`generate.applicable` decides this from the request's shape, never from an outcome.

The GRPC column is read from the server's `/health` capabilities. While gRPC is not
mounted, the baseline records it as pending. Once it is mounted, that line changes, and
the run fails until the column gets a client and the baseline is blessed.

## Outcomes

Each cell is reduced to a class and its rows. The class comes from the response's codes
only, through `compare.CODE_CLASS`, which maps each transport's spelling of a refusal to
one class: `DENIED`, `UNAUTH`, `INVALID` or `NOTFOUND`.

- A code not in the table keeps its own name (`ERR:<code>`), so it never merges into a
  known class.
- A response with no JSON body takes its HTTP status as its code (`http-401`).
- Messages are kept for the report and never decide anything.

Rows are compared on the selected fields. MCP takes no selection and returns the fields
the caller may read, so its rows are compared on the selected fields it returned. A
selected field it left out is the issue `omitted:<fields>`: it is counted, not skipped.

## Metrics

| Metric | Target |
|---|---|
| Agreement with POST, over every compared cell (and per column) | 100% |
| Refused on POST, served elsewhere: POST refuses, another transport returns rows | 0 |
| No row on POST, served elsewhere: POST returns no row, another transport returns rows | 0 |
| Undocumented divergences: every other disagreeing cell | 0 |
| Cells with a code outside `extensions.code` | 0 |
| Pending columns | 0 |

With a baseline present, each metric line shows its change from the baseline, and
whether that change is better or worse.

"No row on POST, served elsewhere" is also gated as a ratchet. A value above the
baseline's fails the run, and a value below it is progress to record with `--bless`. It
stands at 0. A baseline that lacks the value counts as 0.

## The WS column

Subscriptions are graded on the rows the run writes, against what POST reads for the
same row, principal and selection:

1. one subscription per principal and selection (`oracle.ws.SELECTIONS`) opens, and is
   acknowledged or refused;
2. once all are open, each writer (`oracle.ws.WRITERS`, one per tenant) creates one row
   through POST `createOrder`;
3. the subscriptions collect events until each has one per row, or the wait ends;
4. each (subscription, row) cell is compared with POST `order(id:)` for that principal
   and selection.

Every subscription is open before the first write, so every subscriber is graded on
every row, whatever the timing.

- A principal POST serves the row to must receive exactly one event with the same data.
  A repeated delivery is a data difference.
- A principal POST serves nothing must receive no event.
- A refusal (the HTTP status of the upgrade, a close code or an error frame) is compared
  by class, like any other cell.

The column runs after the corpus, since the corpus reads the fixture. The rows it writes
are deleted at the end, so a later run, or the self-test's next measurement, sees the
fixture unchanged. Events come from the change log, so the server needs the `observers`
feature, and the fixture enables `[observers]`.

## Connection handling

`oracle/protocol.py` checks over a raw socket how the server closes a connection after
answering a POST before reading its body. If the server closes the connection, the
response must say so with `Connection: close` (RFC 9112 §9.6). A finding is gated like
any other line.

During the corpus run, a request on a kept-alive connection that was closed before any
answer is retried once on a new connection. Without the retry, which cell meets a closed
connection would depend on machine load. The run prints the retry count but never gates
it; the protocol line gates the cause.

## The baseline

`baseline.json` records each disagreeing cell (request fingerprint, column, issues), each
error-format line (column, code, place), the column lines and the protocol lines. The gate
is equality in both directions, as for the graphql-js oracle:

- **A cell not in the baseline** is a regression. The run fails and prints the request
  and both responses.
- **A baseline entry that no longer occurs** is progress. The run fails until
  `--bless` records it in the same change.

## Running it

```bash
make oracle-transports        # builds the server with every transport, then runs the oracle (Docker, uv)
tools/oracle/transports/run.sh --report /tmp/transports.json   # every disagreeing cell, both responses
```

`run.sh` starts the following, all on free ports:

- a PostgreSQL 18 container, unless `ORACLE_ADMIN_URL` is set;
- the fixture database (`fraiseql-cli setup`, then `fixture/data.sql`);
- a local OpenID issuer (`oracle/issuer.py`), whose RSA key is generated for the run in
  its temporary directory, so no key is ever committed;
- the server, from `fixture/server.toml.in`.

| Variable | Meaning |
|---|---|
| `FRAISEQL_CLI`, `FRAISEQL_SERVER` | The binaries under test. The server needs the features `cli,rest,grpc,mcp,arrow,observers`. |
| `ORACLE_ADMIN_URL` | A PostgreSQL URL allowed to create databases. When unset, a throwaway container is started and removed on exit. |
| `ORACLE_PRIVATE_MODULE` | A Python file, kept outside the repository, that may export `REQUESTS` and `PRINCIPALS`, and extend the generator's corpus with `ROOTS`, `ORDER_FIELDS`, `FILTERS`, `ORDER_GATED`, `HEADERS` (values for the `x-probe` header) and `WS_SELECTIONS`. It runs against its own `--baseline`. |

Exit codes: 0 means the run matches the baseline; 1 means the gate failed; 2 means the
harness itself failed, and `run.sh` then prints the end of the server's log.

## The self-test

`make oracle-transports-selftest` (`run.sh --selftest`, about four minutes) proves the
gate goes red. It needs a server that matches the baseline.

- Pure checks:
  - an unknown code keeps its own class;
  - a bodyless 401 is `UNAUTH` by its status;
  - a cell served where POST refused is a class divergence;
  - a WS event for another row is not this row's event;
  - a refused WS subscription answers its refusal, whatever it received;
  - a WS row delivered twice differs from POST's one row;
  - an MCP cell that leaves out a selected field is counted.
- The ratchet fails in both directions and names the metric. A baseline without the
  value ratchets at 0.
- Every list in the baseline is a gate site. A one-entry change to it fails the run in
  both directions and names that entry. The sites come from the baseline itself, not
  from the gate's list, so a list the gate stops reading fails here. A changed corpus
  fails too.
- `oracle/proxy.py`, a rewriting proxy, stands in for a server that changed:
  - REST serves rows where it refused: only REST cells are reported, and "refused on
    POST, served elsewhere" rises;
  - GET alters one value: only GET cells are reported, each as `data`.
- The proxy cannot carry WebSockets, so `oracle.ws.run` takes a `tamper` hook for the
  self-test only. It changes what the subscriptions received before they are graded:
  - every live B-viewer subscription receives tenant A's row: exactly those cells are
    reported, on WS only, and "no row on POST, served elsewhere" rises by as many and
    trips its ratchet;
  - no subscription receives anything: only WS cells are reported, each as `data`.

## Measuring a fix

```bash
make oracle-transports
```

The metric lines show the change from the baseline. A fix that removes divergences fails
the run as unrecorded progress until `run.sh --bless` records it, and the `baseline.json`
diff is then part of the change.

## Development

```bash
cd tools/oracle/transports
uv sync --frozen
uv run ruff format . && uv run ruff check . && uv run ty check
```

The dependencies are pinned in `pyproject.toml` and `uv.lock`, and the interpreter in
`.python-version` (pyarrow publishes no wheel for newer ones yet, and a source build fails). `exclude-newer` keeps every
resolved version at least two weeks older than the lock.
