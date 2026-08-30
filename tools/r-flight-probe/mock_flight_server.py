"""A Flight server that enforces the same two things FraiseQL's does:

  1. a Handshake whose payload is the literal "Bearer <jwt>", answered with a
     session token;
  2. an `authorization: Bearer <session token>` header on every later call,
     checked BEFORE the ticket is looked at.

It is not FraiseQL: there is no OIDC validator and no SQL. It exists to exercise
a client's half of the exchange, which is the half examples/r/fraiseql_client.R
has never had run.
"""
import json
import sys

import pyarrow as pa
import pyarrow.flight as fl

SESSION_TOKEN = b"session-token-for-test"


class AuthHandler(fl.ServerAuthHandler):
    def authenticate(self, outgoing, incoming):
        payload = incoming.read()
        if not payload.startswith(b"Bearer "):
            raise fl.FlightUnauthenticatedError(
                f"handshake payload was not 'Bearer <jwt>': {payload!r}")
        print(f"[server] handshake ok, payload={payload!r}", flush=True)
        outgoing.write(SESSION_TOKEN)

    def is_valid(self, token):
        return b"user"


class HeaderCheck(fl.ServerMiddlewareFactory):
    def start_call(self, info, headers):
        name = getattr(info, "method", None)
        if name is not None and "HANDSHAKE" in str(name).upper():
            return None
        got = headers.get("authorization")
        if not got:
            raise fl.FlightUnauthenticatedError(
                "Missing authorization header - perform handshake first")
        value = got[0] if isinstance(got, (list, tuple)) else got
        expected = "Bearer " + SESSION_TOKEN.decode()
        if value != expected:
            raise fl.FlightUnauthenticatedError(f"bad authorization header: {value!r}")
        print(f"[server] authorization header accepted on {name}", flush=True)
        return None


class Server(fl.FlightServerBase):
    def __init__(self, location):
        super().__init__(location, auth_handler=AuthHandler(),
                         middleware={"hdr": HeaderCheck()})

    def do_get(self, context, ticket):
        req = json.loads(ticket.ticket.decode())
        print(f"[server] do_get ticket={req}", flush=True)
        table = pa.table({
            "id": [1, 2, 3],
            "name": ["alice", "bob", "carol"],
            "ticket_type": [req.get("type", "?")] * 3,
        })
        return fl.RecordBatchStream(table)


if __name__ == "__main__":
    location = f"grpc://0.0.0.0:{int(sys.argv[1])}"
    print(f"[server] listening on {location}", flush=True)
    Server(location).serve()
