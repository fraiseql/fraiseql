# FraiseQL R Client

Arrow Flight client for FraiseQL enabling statistical analysis and data manipulation in R.

**What is verified** (#1260). Two things, both reproducible:

* it parses under a real R on every push — `tools/check-r-examples-parse.sh`;
* `make probe-r-flight` runs it against a live Arrow Flight server that enforces
  the same handshake and `authorization` header FraiseQL's does, and checks that a
  tampered session token is refused, so the pass is not vacuous.

Running it is what found the two defects parsing could not: reticulate provisioning
its own Python instead of the one you `pip install`ed into, and `rawToChar()` on a
`python.builtin.bytes` object, which meant no handshake could ever complete.

What is **not** covered: neither runs against `fraiseql-server`, so this client and
that server are not known to agree on the wire.

## Installation

### From Source

```r
# Install dependencies. reticulate is not optional: this client drives pyarrow
# through it, because arrow::flight_get() has no parameter for per-call gRPC
# metadata and so cannot send the header the server requires.
install.packages(c("arrow", "jsonlite", "reticulate"))

# Load the client
source("fraiseql_client.R")
```

pyarrow must be importable from the Python that reticulate binds to:

```bash
pip install pyarrow
```

### Build as Package

```bash
# Build and install
R CMD build .
R CMD INSTALL fraiseqlclient_0.1.0.tar.gz
```

## Usage

### Connect to Server

```r
library(fraiseqlclient)

# Authentication is not optional. `connect_fraiseql()` performs the Flight
# handshake and holds the session token every later call needs; there is no
# constructor that skips it.
Sys.setenv(FRAISEQL_JWT = "<a token this server accepts>")
conn <- connect_fraiseql(host = "localhost", port = 50051)
```

The server authenticates `do_get` **before** it decodes the ticket, so a call
with no credentials is refused whatever it asks for. The exchange is:

1. A Flight `Handshake` whose payload is the literal string `"Bearer <jwt>"`;
   the response payload is a **session token**.
2. Every later call carries `authorization: Bearer <session token>` — the
   session token, not the original JWT.

The server also needs `FLIGHT_SESSION_SECRET` set, or the handshake fails with
`FLIGHT_SESSION_SECRET not configured`.

### Execute GraphQL Queries

```r
# Basic query
df <- query_graphql(conn, "{ users { id name email } }")
head(df)

# With summarization
df <- query_graphql(conn, "{ orders { id total customerId } }")
summary(df$total)
```

### Read a View Directly

```r
# Pushes the filter and ordering to the server
df <- query_view(conn, "v_user", order_by = "id", limit = 100)
head(df)
```

### Several Queries in One Round Trip

```r
df <- query_batched(conn, c("{ users { id } }", "{ posts { id } }"))
```

### Observer events are not available

`ObserverEvents` is a variant of the Flight ticket enum, but this server answers
it with `unimplemented`:

> ObserverEvents is not implemented: this server does not produce an Arrow event
> stream. Query historical events through the GraphQL API instead.

`stream_events()` and `stream_events_batched()` used to be this client's headline
examples and could never have returned data (#1200). They are gone rather than
left to fail at runtime.

### Integration with dplyr

```r
library(dplyr)

# Execute query and manipulate with dplyr
orders <- query_graphql(conn, "{ orders { id total status } }") %>%
  filter(status == "completed") %>%
  group_by(status) %>%
  summarize(avg_total = mean(total), count = n())

print(orders)
```

## Performance

* **Zero-copy**: Arrow record batches are consumed directly, with no row-by-row
  JSON deserialization
* **Memory efficient**: results arrive as batches rather than one materialized
  document

No speed figure is quoted here. This client has never been run against a server,
so any number for it would be borrowed from a different client on different
hardware.

## Requirements

* R 4.0+
* arrow package (CRAN: `install.packages("arrow")`)
* jsonlite package (CRAN: `install.packages("jsonlite")`)
* reticulate package (CRAN: `install.packages("reticulate")`)
* pyarrow, importable from the Python reticulate binds to (`pip install pyarrow`)
* FraiseQL server running on accessible host:port, with `FLIGHT_SESSION_SECRET`
  set and an OIDC validator configured — `do_get` is authenticated before the
  ticket is decoded, so an unconfigured server refuses every call

## Examples

`fraiseql_client.R` ends with a block that runs when the file is executed
non-interactively (`Rscript fraiseql_client.R`) rather than sourced from a session.
It needs `FRAISEQL_JWT` set and a reachable server; it has not been run.
