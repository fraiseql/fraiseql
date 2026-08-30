#!/usr/bin/env python3
"""Flight subgraph, and the saga coordinator.

Two jobs. As a subgraph it owns `Query.flight`. As the coordinator it implements
`Mutation.bookTravel`, which is the saga: five steps across five services, with a
compensating action for every step already taken when one fails.

    reserve flight   → local
    reserve hotel    → hotel-service        compensate: cancel
    reserve car      → car-service          compensate: cancel
    charge payment   → payment-service      compensate: refund
    notify           → notification-service (last step; nothing after it to undo)

Compensation runs in REVERSE order of reservation, which is what makes it a saga
rather than a retry loop: the car is released before the hotel, because the hotel
booking is what the car booking was made against.

Federation does not orchestrate this. The router federates the *read* side; the
saga is ordinary service-to-service calls inside this resolver. That distinction
is the thing this example exists to show.

To see the failure path, book with a userId beginning `decline` — payment-service
refuses those deterministically.
"""
import json
import os
import re
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

SERVICE = "flight"
PORT = int(os.environ.get("PORT", "4000"))
TIMEOUT = float(os.environ.get("STEP_TIMEOUT_SECS", "5"))

HOTEL = os.environ.get("HOTEL_URL", "http://hotel-service:4000")
CAR = os.environ.get("CAR_URL", "http://car-service:4000")
PAYMENT = os.environ.get("PAYMENT_URL", "http://payment-service:4000")
NOTIFY = os.environ.get("NOTIFICATION_URL", "http://notification-service:4000")

RESERVATIONS = {}


def _post(url, payload):
    """POST JSON. Returns (status, body); a transport failure is status 0.

    A saga coordinator must distinguish "the step refused" from "the step could
    not be reached" — both mean roll back, but only the first is a business
    outcome worth reporting to the caller as such.
    """
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
            return resp.status, json.loads(resp.read() or b"{}")
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read() or b"{}")
        except json.JSONDecodeError:
            return e.code, {}
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        return 0, {"error": str(e)}


ARG_RE_VAR = r'{name}\s*:\s*\$(\w+)'
ARG_RE_LIT = r'{name}\s*:\s*"([^"]*)"'


def arg(query, variables, name, default=None):
    """Resolve a GraphQL argument from the request.

    A stub that reads `variables[name]` only is silently wrong whenever the
    caller names the variable something other than the argument — the default is
    used, no error is raised, and the operation looks like it succeeded with
    different inputs. So: prefer a variable named for the argument, then follow
    `name: $var` to the variable it names, then take an inline literal.
    """
    if name in variables:
        return variables[name]
    m = re.search(ARG_RE_VAR.format(name=re.escape(name)), query)
    if m and m.group(1) in variables:
        return variables[m.group(1)]
    m = re.search(ARG_RE_LIT.format(name=re.escape(name)), query)
    if m:
        return m.group(1)
    return default


def reserve_flight(flight_id):
    rid = f"f-{uuid.uuid4().hex[:6]}"
    RESERVATIONS[rid] = {"status": "reserved", "flightId": flight_id}
    return {"id": rid, "status": "reserved", "service": SERVICE}


def cancel_flight(rid):
    if rid in RESERVATIONS:
        RESERVATIONS[rid]["status"] = "cancelled"
        return True
    return False


def book_travel(query, variables):
    """The saga. Returns a Booking, confirmed or rolled back."""
    booking_id = f"bk-{uuid.uuid4().hex[:8]}"
    user_id = arg(query, variables, "userId", "u-1")
    done = []            # steps taken, in order, each with its compensation
    compensations = []

    def compensate():
        # Reverse order: undo the most recent step first.
        for step in reversed(done):
            name, undo = step["name"], step["undo"]
            ok = undo()
            compensations.append(f"{name}: {'compensated' if ok else 'compensation failed'}")

    flight = reserve_flight(arg(query, variables, "flightId", "FL-1"))
    done.append({"name": "flight", "undo": lambda: cancel_flight(flight["id"])})

    status, hotel = _post(f"{HOTEL}/internal/reserve",
                          {"hotelId": arg(query, variables, "hotelId"), "userId": user_id})
    if status != 200:
        compensate()
        return booking(booking_id, "rolled_back", [flight], compensations,
                       f"hotel step failed ({status})")
    done.append({"name": "hotel",
                 "undo": lambda: _post(f"{HOTEL}/internal/cancel", {"id": hotel["id"]})[0] == 200})

    status, car = _post(f"{CAR}/internal/reserve",
                        {"carId": arg(query, variables, "carId"), "userId": user_id})
    if status != 200:
        compensate()
        return booking(booking_id, "rolled_back", [flight, hotel], compensations,
                       f"car step failed ({status})")
    done.append({"name": "car",
                 "undo": lambda: _post(f"{CAR}/internal/cancel", {"id": car["id"]})[0] == 200})

    status, payment = _post(f"{PAYMENT}/internal/charge",
                            {"userId": user_id, "amount": 245.0, "bookingId": booking_id})
    if status != 200:
        compensate()
        return booking(booking_id, "rolled_back", [flight, hotel, car], compensations,
                       payment.get("reason", f"payment step failed ({status})"))
    done.append({"name": "payment",
                 "undo": lambda: _post(f"{PAYMENT}/internal/refund", {"id": payment["id"]})[0] == 200})

    # Last step. Nothing follows it, so there is nothing to compensate if it
    # fails — the booking stands and the notification is retried out of band.
    _post(f"{NOTIFY}/internal/reserve", {"userId": user_id, "bookingId": booking_id})

    return booking(booking_id, "confirmed", [flight, hotel, car, payment], compensations, None)


def booking(booking_id, status, reservations, compensations, reason):
    return {
        "bookingId": booking_id,
        "status": status,
        "reason": reason,
        "reservations": [
            {"type": r.get("service", "unknown"), "id": r.get("id", ""),
             "status": r.get("status", "unknown")}
            for r in reservations
        ],
        "compensations": compensations,
    }


def graphql(body):
    query = body.get("query", "")
    variables = body.get("variables", {})
    if "bookTravel" in query:
        return {"data": {"bookTravel": book_travel(query, variables)}}
    if "flight" in query:
        return {"data": {"flight": {
            "id": arg(query, variables, "id", "FL-1"),
            "departure": "BER",
            "arrival": "LIS",
            "price": 245.0,
        }}}
    if "__typename" in query:
        return {"data": {"__typename": "Query"}}
    return {"data": None,
            "errors": [{"message": f"{SERVICE} does not serve this operation"}]}


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
        if self.path == "/graphql":
            self._reply(200, graphql(body))
        elif self.path == "/internal/cancel":
            self._reply(200, {"id": body.get("id"),
                              "status": "cancelled" if cancel_flight(body.get("id")) else "not_found",
                              "service": SERVICE})
        else:
            self._reply(404, {"errors": [{"message": "not found"}]})

    def log_message(self, fmt, *args):
        print(f"[{SERVICE}] {fmt % args}", flush=True)


if __name__ == "__main__":
    print(f"[{SERVICE}] listening on :{PORT}", flush=True)
    HTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
