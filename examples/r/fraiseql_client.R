#' FraiseQL Arrow Flight client for R
#'
#' @section Authentication is not optional:
#'
#' The server authenticates `do_get` **before** it decodes the ticket, so a call
#' with no credentials is refused whatever it asks for. Credentials come from a
#' two-step exchange:
#'
#' 1. A Flight `Handshake` whose payload is the literal string `"Bearer <jwt>"`.
#'    The response payload is a **session token**.
#' 2. Every later call carries `authorization: Bearer <session token>` in its
#'    gRPC metadata — the session token, not the original JWT.
#'
#' This client used to do neither, so every call it could make returned
#' `UNAUTHENTICATED` (#1200).
#'
#' @section Why this file uses reticulate directly:
#'
#' `arrow`'s Flight functions are implemented with reticulate over pyarrow, and
#' `arrow::flight_get()` has no parameter for per-call gRPC metadata — so it
#' cannot send the `authorization` header the server requires. This file drives
#' the same pyarrow client the `arrow` package would, and adds the two things it
#' does not expose: a handshake handler, and call options carrying the header.
#'
#' @examples
#' \dontrun{
#' source("fraiseql_client.R")
#' conn <- connect_fraiseql(jwt = Sys.getenv("FRAISEQL_JWT"))
#' df <- query_graphql(conn, "{ users { id name } }")
#' print(df)
#' }

library(arrow)
library(jsonlite)
library(reticulate)

# The handshake handler, defined in Python because it must subclass
# pyarrow.flight.ClientAuthHandler. Our server's handshake is NOT Basic auth, so
# `authenticate_basic_token()` does not apply: the payload is the literal string
# "Bearer <jwt>" and the reply is the session token.
.fraiseql_define_auth_handler <- function() {
  reticulate::py_run_string("
import pyarrow.flight as _fl


class _FraiseQLBearerHandshake(_fl.ClientAuthHandler):
    def __init__(self, jwt):
        super().__init__()
        self._jwt = jwt
        self._token = b''

    def authenticate(self, outgoing, incoming):
        outgoing.write(('Bearer ' + self._jwt).encode())
        self._token = incoming.read()

    def get_token(self):
        return self._token
")
}

#' Connect and complete the handshake
#'
#' There is deliberately no constructor that skips the handshake: a client
#' without a session token cannot make a call the server will answer, so
#' producing one would only move the failure later.
#'
#' @param host Server hostname (default: "localhost")
#' @param port Server port (default: 50051)
#' @param jwt  A token this server accepts. Defaults to `FRAISEQL_JWT`.
#'
#' @return A `fraiseql_flight_client`: the pyarrow client plus its session token.
#'
#' @export
connect_fraiseql <- function(host = "localhost", port = 50051,
                             jwt = Sys.getenv("FRAISEQL_JWT")) {
  if (!nzchar(jwt)) {
    stop("set FRAISEQL_JWT (or pass jwt=) — the Flight surface authenticates every call")
  }

  fl <- reticulate::import("pyarrow.flight")
  client <- fl$FlightClient(paste0("grpc://", host, ":", port))

  py <- .fraiseql_define_auth_handler()
  handler <- py$`_FraiseQLBearerHandshake`(jwt)
  client$authenticate(handler)

  session_token <- rawToChar(handler$get_token())
  if (!nzchar(session_token)) {
    stop("handshake returned an empty session token")
  }

  structure(
    list(client = client, session_token = session_token, flight = fl),
    class = "fraiseql_flight_client"
  )
}

# Call options carrying the session token. Without this header the server answers
# `UNAUTHENTICATED: Missing authorization header - perform handshake first`,
# before it has looked at the ticket at all.
.fraiseql_call_options <- function(conn) {
  conn$flight$FlightCallOptions(
    headers = list(list(
      charToRaw("authorization"),
      charToRaw(paste0("Bearer ", conn$session_token))
    ))
  )
}

.fraiseql_fetch <- function(conn, ticket_data) {
  ticket <- conn$flight$Ticket(charToRaw(toJSON(ticket_data, auto_unbox = TRUE)))
  reader <- conn$client$do_get(ticket, options = .fraiseql_call_options(conn))
  as.data.frame(reader$read_all())
}

#' Execute a GraphQL query
#'
#' @param conn      Connection from connect_fraiseql()
#' @param query     GraphQL query string
#' @param variables Optional query variables (list)
#'
#' @return data.frame with results
#'
#' @export
query_graphql <- function(conn, query, variables = NULL) {
  .fraiseql_fetch(conn, list(
    type = "GraphQLQuery",
    query = query,
    variables = variables
  ))
}

#' Read a view directly, pushing the filter and ordering to the server
#'
#' @param conn     Connection from connect_fraiseql()
#' @param view     View name, e.g. "v_user"
#' @param filter   Optional filter (list)
#' @param order_by Optional column to order by
#' @param limit    Optional row limit
#'
#' @return data.frame with results
#'
#' @export
query_view <- function(conn, view, filter = NULL, order_by = NULL, limit = NULL) {
  .fraiseql_fetch(conn, list(
    type = "OptimizedView",
    view = view,
    filter = filter,
    order_by = order_by,
    limit = limit
  ))
}

#' Send several queries in one round trip
#'
#' @param conn    Connection from connect_fraiseql()
#' @param queries Character vector of GraphQL query strings
#'
#' @return data.frame with results
#'
#' @export
query_batched <- function(conn, queries) {
  .fraiseql_fetch(conn, list(type = "BatchedQueries", queries = queries))
}

# `stream_events` is gone on purpose. `ObserverEvents` is a variant of the ticket
# enum, but the server answers it with `unimplemented`: "this server does not
# produce an Arrow event stream. Query historical events through the GraphQL API
# instead." It was this file's headline example (#1200).

if (identical(environment(), globalenv()) && !interactive()) {
  conn <- connect_fraiseql()
  cat("Handshake complete\n")

  users <- query_graphql(conn, "{ users { id name email } }")
  cat("users:", nrow(users), "row(s)\n")

  view <- query_view(conn, "v_user", order_by = "id", limit = 100)
  cat("v_user:", nrow(view), "row(s)\n")
}
