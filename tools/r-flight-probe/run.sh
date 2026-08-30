#!/usr/bin/env bash
# run.sh — run examples/r/fraiseql_client.R against a live Arrow Flight server.
#
# Background (#1260): nothing in this repository ran, linted or even parsed that
# client. `tools/check-r-examples-parse.sh` closed the parsing half as a merge
# gate; this closes the running half, which is the one that matters — #1200
# rewrote the client to perform the Flight handshake and attach the
# `authorization` header, and running it here found two defects that parsing
# could not see:
#
#   * reticulate >= 1.41 provisions an ephemeral uv-managed Python instead of the
#     system interpreter, so `pip install pyarrow` was not enough and the import
#     failed;
#   * `rawToChar()` was called on what reticulate returns for Python bytes — a
#     `python.builtin.bytes` object, not a raw vector — so no handshake could
#     ever complete.
#
# What this proves and what it does not. The server here enforces the same two
# things FraiseQL's does (a "Bearer <jwt>" handshake answered with a session
# token, then `authorization: Bearer <session token>` checked before the ticket
# is read), so it exercises the client's whole half of the exchange. It is not
# fraiseql-server: there is no OIDC validator and no SQL, so this does not
# establish that the two agree on the wire. `crates/fraiseql-arrow/tests/
# flight_auth_test.rs` is what pins the server's side.
#
# Not a merge gate: the first build compiles libarrow from source, about fifteen
# minutes. Run it when the client changes.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
here="${REPO_ROOT}/tools/r-flight-probe"
img="${R_FLIGHT_PROBE_IMAGE:-fraiseql-r-flight-probe:1}"
client="${R_FLIGHT_CLIENT_DIR:-${REPO_ROOT}/examples/r}"

if ! command -v docker >/dev/null 2>&1; then
    echo "✗ docker is required: this probe needs R, the arrow package, reticulate and pyarrow."
    echo "    It does not skip — a probe that passes without running proves nothing."
    exit 1
fi

if [ ! -f "${client}/fraiseql_client.R" ]; then
    echo "✗ no fraiseql_client.R under '${client}'."
    exit 1
fi

echo "→ building ${img} (first build compiles libarrow from source, ~15 min; cached after)"
docker build -t "$img" "$here"

echo "→ running the probe against ${client}/fraiseql_client.R"
docker run --rm -v "${client}:/client:ro" "$img"
