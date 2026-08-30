#!/usr/bin/env python3
"""Notification subgraph — the saga's final, non-compensatable step.

Part of the saga-complex example. Runs on the Python standard library: the
services are stubs that demonstrate the saga's control flow, so adding a
framework and a package index to the container start would buy nothing.

Endpoints
  POST /graphql          the subgraph's GraphQL surface (federated by the router)
  POST /internal/reserve reserve, and remember it so it can be compensated
  POST /internal/cancel  the compensating action for a reserve
  GET  /health           liveness
"""
import json
import os
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

SERVICE = "notification"
PORT = int(os.environ.get("PORT", "4000"))

# Reservations this service is holding, by id. A real service would persist
# these; the saga only needs them to exist long enough to be compensated.
STATE = {}


def reserve(payload):
    rid = f"n-{uuid.uuid4().hex[:6]}"
    STATE[rid] = {"status": "reserved", "request": payload}
    return {"id": rid, "status": "reserved", "service": SERVICE}


def cancel(payload):
    rid = payload.get("id")
    if rid in STATE:
        STATE[rid]["status"] = "cancelled"
        return {"id": rid, "status": "cancelled", "service": SERVICE}
    # Compensation must be idempotent: the coordinator may retry, and a
    # compensation for something never reserved is a no-op, not an error.
    return {"id": rid, "status": "not_found", "service": SERVICE}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _reply(self, code, obj, ctype="application/json"):
        raw = (obj if isinstance(obj, str) else json.dumps(obj)).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def _read_chunked(self):
        """Read a chunked request body.

        The Apollo Router sends `Transfer-Encoding: chunked`, not
        Content-Length. A handler that reads only Content-Length gets an empty
        body AND leaves the chunk framing in the socket, where the next
        readline parses a hex chunk size as a request line — the symptom is
        `Bad request syntax ('68')` in the log and a redacted subgraph error at
        the router.
        """
        buf = b""
        while True:
            line = self.rfile.readline()
            if not line:
                break
            size = int(line.split(b";")[0].strip() or b"0", 16)
            if size == 0:
                # Consume the trailer section, up to and including its blank line.
                while True:
                    trailer = self.rfile.readline()
                    if trailer in (b"\r\n", b"\n", b""):
                        break
                break
            buf += self.rfile.read(size)
            self.rfile.read(2)  # the CRLF that ends the chunk
        return buf

    def _body(self):
        if "chunked" in (self.headers.get("Transfer-Encoding") or "").lower():
            raw = self._read_chunked()
        else:
            length = int(self.headers.get("Content-Length") or 0)
            raw = self.rfile.read(length) if length else b""
        if not raw:
            return {}
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {}

    def do_GET(self):
        if self.path == "/health":
            self._reply(200, "OK", "text/plain")
        else:
            self._reply(404, {"errors": [{"message": "not found"}]})

    def do_POST(self):
        body = self._body()
        if self.path == "/internal/reserve":
            self._reply(200, reserve(body))
        elif self.path == "/internal/cancel":
            self._reply(200, cancel(body))
        elif self.path == "/graphql":
            self._reply(200, graphql(body))
        else:
            self._reply(404, {"errors": [{"message": "not found"}]})

    def log_message(self, fmt, *args):
        print(f"[{SERVICE}] {fmt % args}", flush=True)


def graphql(body):
    """This subgraph owns no federated field.

    It participates in the saga through /internal/* only, which is the point:
    a saga step is not necessarily a GraphQL field. `__typename` is answered so
    the router's health probing sees a well-formed subgraph.
    """
    if "__typename" in body.get("query", ""):
        return {{"data": {{"__typename": "Query"}}}}
    return {{"data": None,
            "errors": [{{"message": f"{{SERVICE}} participates via /internal/* only"}}]}}



if __name__ == "__main__":
    print(f"[{SERVICE}] listening on :{PORT}", flush=True)
    HTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
