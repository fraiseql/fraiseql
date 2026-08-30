#!/usr/bin/env python3
"""Payment subgraph — the saga step that is allowed to fail.

A saga example whose steps all succeed does not demonstrate a saga; it
demonstrates a sequence. This service declines deterministically so the
compensation path is reachable from a test: any `userId` beginning with
`decline` is refused. Everything else is charged.

Endpoints
  POST /graphql          answers __typename only; this subgraph owns no field
  POST /internal/charge  charge, or decline for a `decline*` user
  POST /internal/refund  the compensating action for a charge
  GET  /health           liveness
"""
import json
import os
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

SERVICE = "payment"
PORT = int(os.environ.get("PORT", "4000"))

# The prefix that makes a charge fail. Named rather than random: a saga test has
# to be able to ask for the failure branch and get it every time.
DECLINE_PREFIX = os.environ.get("DECLINE_PREFIX", "decline")

CHARGES = {}


def charge(payload):
    user = str(payload.get("userId", ""))
    if user.startswith(DECLINE_PREFIX):
        return {"status": "declined", "service": SERVICE,
                "reason": f"card declined for {user}"}, False
    cid = f"ch-{uuid.uuid4().hex[:6]}"
    CHARGES[cid] = {"status": "charged", "amount": payload.get("amount")}
    return {"id": cid, "status": "charged", "service": SERVICE}, True


def refund(payload):
    cid = payload.get("id")
    if cid in CHARGES:
        CHARGES[cid]["status"] = "refunded"
        return {"id": cid, "status": "refunded", "service": SERVICE}
    return {"id": cid, "status": "not_found", "service": SERVICE}


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
        if self.path == "/internal/charge":
            result, ok = charge(body)
            # A declined charge is a business outcome, not a transport error:
            # 402 so the coordinator can tell it apart from a service that is
            # simply down, which is a different saga decision.
            self._reply(200 if ok else 402, result)
        elif self.path == "/internal/refund":
            self._reply(200, refund(body))
        elif self.path == "/graphql":
            if "__typename" in body.get("query", ""):
                self._reply(200, {"data": {"__typename": "Query"}})
            else:
                self._reply(200, {"data": None, "errors": [
                    {"message": f"{SERVICE} participates via /internal/* only"}]})
        else:
            self._reply(404, {"errors": [{"message": "not found"}]})

    def log_message(self, fmt, *args):
        print(f"[{SERVICE}] {fmt % args}", flush=True)


if __name__ == "__main__":
    print(f"[{SERVICE}] listening on :{PORT}", flush=True)
    HTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
