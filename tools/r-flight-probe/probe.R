# Drive examples/r/fraiseql_client.R against a live Flight server.
#
# sys.source() into a fresh environment rather than source(): the client's
# trailing demo block guards on `identical(environment(), globalenv())`, which is
# TRUE for a plain source() from a script too, so sourcing it would fire the demo
# against the default host:port.
env <- new.env()
sys.source("/client/fraiseql_client.R", envir = env)
cat("sourced OK\n")

port <- as.integer(Sys.getenv("PROBE_PORT", "15051"))
conn <- env$connect_fraiseql(host = "127.0.0.1", port = port, jwt = "a-jwt")
stopifnot(nzchar(conn$session_token))
cat("handshake OK, session token:", conn$session_token, "\n")

df <- env$query_graphql(conn, "{ users { id name } }")
stopifnot(is.data.frame(df), nrow(df) == 3L)
cat("query_graphql OK:", nrow(df), "rows,", paste(names(df), collapse = ","), "\n")

v <- env$query_view(conn, "v_user", order_by = "id", limit = 100)
stopifnot(is.data.frame(v), nrow(v) == 3L)
cat("query_view OK:", nrow(v), "rows\n")

b <- env$query_batched(conn, c("{ users { id } }", "{ posts { id } }"))
stopifnot(is.data.frame(b), nrow(b) == 3L)
cat("query_batched OK:", nrow(b), "rows\n")

cat("PROBE OK\n")
