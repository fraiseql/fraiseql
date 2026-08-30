# The probe passing means nothing unless the authorization header is load-bearing.
# Complete a real handshake, then tamper with the session token: the server must
# refuse. Without this, a mock that accepted anything would report the client as
# working no matter what it sent — the shape #1260 exists to prevent.
env <- new.env()
sys.source("/client/fraiseql_client.R", envir = env)

port <- as.integer(Sys.getenv("PROBE_PORT", "15051"))
conn <- env$connect_fraiseql(host = "127.0.0.1", port = port, jwt = "a-jwt")
conn$session_token <- "not-the-session-token"

refused <- tryCatch({
  env$query_graphql(conn, "{ users { id } }")
  FALSE
}, error = function(e) {
  cat("refused:", sub("\n.*", "", conditionMessage(e)), "\n")
  TRUE
})

if (!refused) {
  cat("RED CHECK FAILED: a wrong session token was accepted, so this probe proves nothing\n")
  quit(status = 1L)
}
cat("RED CHECK OK\n")
