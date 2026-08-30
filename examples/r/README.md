# FraiseQL R Client

Arrow Flight client for FraiseQL enabling statistical analysis and data manipulation in R.

## Installation

### From Source

```r
# Install dependencies
install.packages(c("arrow", "jsonlite"))

# Load the client
source("fraiseql_client.R")
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

- **Zero-copy**: Arrow data consumed directly without serialization overhead
- **Memory efficient**: Batch processing for large datasets
- **Speed**: 50x faster than HTTP/JSON for 100k+ rows

## Requirements

- R 4.0+
- arrow package (CRAN: `install.packages("arrow")`)
- jsonlite package (CRAN: `install.packages("jsonlite")`)
- FraiseQL server running on accessible host:port

## Examples

See `fraiseql_client.R` for runnable examples in the `if (interactive())` section.
