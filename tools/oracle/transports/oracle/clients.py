"""One client per FraiseQL transport, each returning a `Response`.

A `Response` carries what the transport answered, reduced to what can be compared across
transports: the HTTP (or gRPC) status, the root value, and the error codes. Codes are read
from where each transport puts them; where that is not `extensions.code`, the response
records it (`code_at`), so the error-format metric can count it. Messages are kept for the
report only: nothing is ever decided from a message.
"""

from __future__ import annotations

import asyncio
import contextlib
import http.client
import json
import urllib.parse
from dataclasses import dataclass, field
from typing import Any

import httpx
import pyarrow as pa
import pyarrow.flight as fl
import websockets
from websockets.typing import Subprotocol


@dataclass
class Response:
    status: str
    data: Any = None
    has_data: bool = False
    codes: list[str] = field(default_factory=list)
    code_at: str = ""
    messages: list[str] = field(default_factory=list)
    raw: str = ""


def _error_codes(errors: list[Any]) -> tuple[list[str], str, list[str]]:
    codes: list[str] = []
    places: set[str] = set()
    messages: list[str] = []
    for e in errors:
        if not isinstance(e, dict):
            codes.append("-")
            continue
        ext = e.get("extensions")
        if isinstance(ext, dict) and isinstance(ext.get("code"), str):
            codes.append(ext["code"])
            places.add("extensions")
        elif isinstance(e.get("code"), str):
            codes.append(e["code"])
            places.add("top-level")
        else:
            codes.append("-")  # no code: allowed by the spec, so no place is recorded
        messages.append(str(e.get("message", ""))[:200])
    return codes, ",".join(sorted(places)), messages


def _bodyless(status: int | str) -> str:
    """The code of a response with no JSON body: its HTTP status, when that is an error."""
    return f"http-{status}" if str(status).isdigit() and int(status) >= 400 else "non-json"


def _get_unbounded(url: str, headers: dict[str, str]) -> tuple[int, str]:
    """A GET whose URL httpx refuses to send for its length: the server must answer it."""
    parts = urllib.parse.urlsplit(url)
    conn = http.client.HTTPConnection(parts.hostname or "", parts.port, timeout=30)
    try:
        conn.request("GET", f"{parts.path}?{parts.query}", headers=headers)
        r = conn.getresponse()
        return r.status, r.read().decode(errors="replace")
    finally:
        conn.close()


def graphql_body(status: int | str, text: str, root: str) -> Response:
    """A GraphQL-over-HTTP response body, reduced."""
    try:
        body = json.loads(text)
    except ValueError:
        return Response(
            status=str(status), codes=[_bodyless(status)], code_at="none", raw=text[:300]
        )
    if not isinstance(body, dict):
        return Response(status=str(status), codes=["non-object"], raw=text[:300])
    data = body.get("data")
    value = data.get(root) if isinstance(data, dict) else None
    has_data = isinstance(data, dict) and value is not None
    errors = body.get("errors")
    codes, code_at, messages = _error_codes(errors if isinstance(errors, list) else [])
    return Response(
        status=str(status),
        data=value,
        has_data=has_data,
        codes=codes,
        code_at=code_at,
        messages=messages,
        raw=text[:300],
    )


def get_target(doc: str, variables: dict | None) -> str:
    """The request target `GET /graphql` sends for a document."""
    q = {"query": doc}
    if variables is not None:
        q["variables"] = json.dumps(variables)
    return f"/graphql?{urllib.parse.urlencode(q)}"


def rest_target(path: str, params: dict[str, Any]) -> str:
    """The request target a REST read sends."""
    target = f"/rest/v1{path}"
    if params:
        target += "?" + urllib.parse.urlencode(params, safe="(),.[]:", quote_via=urllib.parse.quote)
    return target


@dataclass
class Endpoints:
    http: str
    flight: str


class Clients:
    def __init__(self, ends: Endpoints) -> None:
        self.ends = ends
        self.http = httpx.Client(timeout=30)
        self._mcp_sessions: dict[tuple[str | None, str], str] = {}
        self._mcp_id = 100
        self._flight: dict[str | None, tuple[fl.FlightClient, bytes]] = {}
        self.retries_seen = 0

    def close(self) -> None:
        self.http.close()
        for client, _ in self._flight.values():
            client.close()

    def _send(self, method: str, url: str, **kw: Any) -> httpx.Response:
        """One request; retried once on a new connection when a kept-alive one was closed
        before any answer. `oracle.protocol` measures that close on its own."""
        try:
            return self.http.request(method, url, **kw)
        except httpx.RemoteProtocolError:
            self.retries_seen += 1
            return self.http.request(method, url, **kw)

    @staticmethod
    def _headers(token: str | None, extra: dict[str, str]) -> dict[str, str]:
        h = {"content-type": "application/json", **extra}
        if token:
            h["authorization"] = f"Bearer {token}"
        return h

    # -- GraphQL over HTTP ---------------------------------------------------------------

    def post(
        self, doc: str, variables: dict | None, token: str | None, hdr: dict[str, str], root: str
    ) -> Response:
        body: dict[str, Any] = {"query": doc}
        if variables is not None:
            body["variables"] = variables
        r = self._send(
            "POST",
            f"{self.ends.http}/graphql",
            headers=self._headers(token, hdr),
            content=json.dumps(body),
        )
        return graphql_body(r.status_code, r.text, root)

    def get(
        self, doc: str, variables: dict | None, token: str | None, hdr: dict[str, str], root: str
    ) -> Response:
        h = self._headers(token, hdr)
        del h["content-type"]
        url = self.ends.http + get_target(doc, variables)
        try:
            r = self._send("GET", url, headers=h)
        except httpx.InvalidURL:
            return graphql_body(*_get_unbounded(url, h), root)
        return graphql_body(r.status_code, r.text, root)

    def sse(
        self, doc: str, variables: dict | None, token: str | None, hdr: dict[str, str], root: str
    ) -> Response:
        """POST asking for an event stream. `next` events merge, `@stream` items included."""
        body: dict[str, Any] = {"query": doc}
        if variables is not None:
            body["variables"] = variables
        h = self._headers(token, hdr)
        h["accept"] = "text/event-stream"
        r = self._send("POST", f"{self.ends.http}/graphql", headers=h, content=json.dumps(body))
        if "text/event-stream" not in r.headers.get("content-type", ""):
            return graphql_body(r.status_code, r.text, root)
        payloads = []
        for block in r.text.split("\n\n"):
            event, data = None, []
            for line in block.split("\n"):
                if line.startswith("event:"):
                    event = line[6:].strip()
                elif line.startswith("data:"):
                    data.append(line[5:].strip())
            if event == "next" and data:
                payloads.append(json.loads("\n".join(data)))
        if not payloads:
            return Response(status=str(r.status_code), codes=["sse-no-event"], raw=r.text[:300])
        out = graphql_body(r.status_code, json.dumps(payloads[0]), root)
        for p in payloads[1:]:
            for inc in p.get("incremental") or []:
                if "items" in inc and isinstance(out.data, list):
                    out.data.extend(inc["items"])
            if p.get("errors"):
                codes, code_at, messages = _error_codes(p["errors"])
                out.codes += codes
                out.code_at = ",".join(sorted({*out.code_at.split(","), code_at} - {""}))
                out.messages += messages
        return out

    # -- REST ----------------------------------------------------------------------------

    def rest(
        self, path: str, params: dict[str, Any], token: str | None, hdr: dict[str, str]
    ) -> Response:
        url = self.ends.http + rest_target(path, params)
        try:
            r = self._send("GET", url, headers=self._headers(token, hdr))
            status, text = r.status_code, r.text
        except httpx.InvalidURL:
            status, text = _get_unbounded(url, self._headers(token, hdr))
        try:
            body = json.loads(text)
        except ValueError:
            return Response(
                status=str(status), codes=[_bodyless(status)], code_at="none", raw=text[:300]
            )
        if status >= 400:
            err = body.get("error") if isinstance(body, dict) else None
            code = err.get("code") if isinstance(err, dict) else None
            return Response(
                status=str(status),
                codes=[code or f"http-{status}"],
                code_at="rest-error" if code else "none",
                messages=[str((err or {}).get("message", ""))[:200]],
                raw=text[:300],
            )
        data = body.get("data") if isinstance(body, dict) and "data" in body else body
        return Response(status=str(status), data=data, has_data=data is not None, raw=text[:300])

    # -- MCP -----------------------------------------------------------------------------

    def _mcp_rpc(
        self, token: str | None, hdr: dict[str, str], method: str, params: dict[str, Any]
    ) -> tuple[int, dict[str, Any] | None, str]:
        key = (token, json.dumps(hdr, sort_keys=True))
        h = self._headers(token, hdr)
        h["accept"] = "application/json, text/event-stream"
        url = f"{self.ends.http}/mcp"
        if key not in self._mcp_sessions:
            init = {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "oracle", "version": "1"},
                },
            }
            r = self._send("POST", url, headers=h, content=json.dumps(init))
            sid = r.headers.get("mcp-session-id")
            if r.status_code != 200 or not sid:
                return r.status_code, None, r.text
            self._send(
                "POST",
                url,
                headers={**h, "mcp-session-id": sid},
                content=json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            )
            self._mcp_sessions[key] = sid
        h["mcp-session-id"] = self._mcp_sessions[key]
        self._mcp_id += 1
        rid = self._mcp_id
        r = self._send(
            "POST",
            url,
            headers=h,
            content=json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}),
        )
        message = None
        for line in r.text.split("\n"):
            if line.startswith("data:") and line[5:].strip():
                with contextlib.suppress(ValueError):
                    m = json.loads(line[5:].strip())
                    if m.get("id") == rid:
                        message = m
        if message is None:
            with contextlib.suppress(ValueError):
                message = r.json()
        return r.status_code, message, r.text

    def mcp(
        self, tool: str, args: dict[str, Any], token: str | None, hdr: dict[str, str], root: str
    ) -> Response:
        status, msg, raw = self._mcp_rpc(
            token, hdr, "tools/call", {"name": tool, "arguments": args}
        )
        if msg is None:
            return Response(
                status=str(status), codes=[_bodyless(status)], code_at="none", raw=raw[:300]
            )
        if "error" in msg:
            return Response(
                status=str(status),
                codes=[f"rpc:{msg['error'].get('code')}"],
                code_at="rpc",
                messages=[str(msg["error"].get("message"))[:200]],
            )
        result = msg.get("result") or {}
        content = result.get("content") or []
        text = content[0].get("text", "") if content else ""
        out = graphql_body(status, text, root)
        if result.get("isError") and not out.codes:
            out.codes = ["tool-error"]
            out.code_at = "none"
        return out

    # -- Arrow Flight --------------------------------------------------------------------

    def _flight_session(self, token: str | None) -> tuple[fl.FlightClient, bytes]:
        if token not in self._flight:
            client = fl.FlightClient(self.ends.flight)
            handshake = _BearerAuth(token or "")
            client.authenticate(handshake)
            self._flight[token] = (client, handshake.session or b"")
        return self._flight[token]

    def flight(
        self, doc: str, variables: dict | None, token: str | None, hdr: dict[str, str], root: str
    ) -> Response:
        """A GraphQL ticket. The request's headers travel as gRPC metadata."""
        ticket: dict[str, Any] = {"type": "GraphQLQuery", "query": doc}
        if variables is not None:
            ticket["variables"] = variables
        try:
            client, session = self._flight_session(token)
            opts = fl.FlightCallOptions(
                headers=[
                    (b"authorization", b"Bearer " + session),
                    *((k.encode(), v.encode()) for k, v in hdr.items()),
                ]
            )
            table = client.do_get(fl.Ticket(json.dumps(ticket).encode()), opts).read_all()
        except pa.ArrowException as e:  # FlightError, and ArrowInvalid for INVALID_ARGUMENT
            self._flight.pop(token, None)
            return Response(
                status=type(e).__name__,
                codes=[f"flight:{type(e).__name__}"],
                code_at="grpc-status",
                messages=[str(e)[:200]],
            )
        rows = table.to_pylist()
        if table.column_names == ["result"]:
            text = rows[0]["result"] if rows else "null"
            return graphql_body("OK", text if isinstance(text, str) else json.dumps(text), root)
        return Response(status="OK", data=rows, has_data=True)


class _BearerAuth(fl.ClientAuthHandler):
    def __init__(self, token: str) -> None:
        super().__init__()
        self.token = token
        self.session: bytes | None = None

    def authenticate(self, outgoing: Any, incoming: Any) -> None:
        outgoing.write(f"Bearer {self.token}".encode())
        self.session = incoming.read()

    def get_token(self) -> bytes | None:
        return self.session


async def _subscribe(url: str, doc: str, token: str | None, wait: float) -> list[dict]:
    frames: list[dict] = []
    headers = {"authorization": f"Bearer {token}"} if token else {}
    async with websockets.connect(
        url, subprotocols=[Subprotocol("graphql-transport-ws")], additional_headers=headers
    ) as sock:
        await sock.send(json.dumps({"type": "connection_init", "payload": {}}))
        ack = json.loads(await asyncio.wait_for(sock.recv(), 5))
        if ack.get("type") != "connection_ack":
            return [ack]
        await sock.send(json.dumps({"id": "1", "type": "subscribe", "payload": {"query": doc}}))
        with contextlib.suppress(asyncio.TimeoutError):
            while True:
                frame = json.loads(await asyncio.wait_for(sock.recv(), wait))
                if frame.get("type") == "ping":
                    await sock.send(json.dumps({"type": "pong"}))
                    continue
                frames.append(frame)
                if frame.get("type") in ("error", "complete"):
                    break
    return frames


def subscribe(http: str, doc: str, token: str | None, wait: float = 1.5) -> list[dict]:
    return asyncio.run(_subscribe(http.replace("http", "ws", 1) + "/ws", doc, token, wait))
