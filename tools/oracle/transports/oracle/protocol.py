"""HTTP/1.1 connection handling, probed over a raw socket.

A server that answers before reading a request's body and then closes the connection
must say so with `Connection: close` (RFC 9112 §9.6). Otherwise a keep-alive client sends
the body, or its next request, into a connection that is already gone, and that next
request fails with no answer. The corpus run retries such a request once on a new
connection, so its cells do not depend on load; this probe keeps the behaviour itself
measured.
"""

from __future__ import annotations

import socket
import time
import urllib.parse

CRLF = b"\r\n"


def _recv(sock: socket.socket, wait: float) -> bytes:
    sock.settimeout(wait)
    try:
        return sock.recv(65536)
    except (TimeoutError, BlockingIOError):
        return b""


ATTEMPTS = 3


def early_refusal(http: str, token: str) -> str | None:
    """A finding when a POST refused before its body is read leaves the connection closed
    without announcing it; None when the connection survives or the close is announced.

    Seeing the defect once is enough, and a loaded machine can hide it from one attempt
    (the early answer arrives late), so up to ATTEMPTS attempts are made. A server without
    the defect passes all of them, so the result does not depend on load."""
    for _ in range(ATTEMPTS):
        finding = _early_refusal_once(http, token)
        if finding:
            return finding
    return None


def _early_refusal_once(http: str, token: str) -> str | None:
    parts = urllib.parse.urlsplit(http)
    body = b'{"query":"{ __typename }"}'
    head = CRLF.join(
        [
            b"POST /graphql HTTP/1.1",
            b"Host: oracle",
            b"content-type: application/json",
            b"authorization: Bearer " + token.encode(),
            b"content-length: " + str(len(body)).encode(),
            b"",
            b"",
        ]
    )
    with socket.create_connection((parts.hostname or "", parts.port or 80), timeout=5) as s:
        s.sendall(head)
        early = _recv(s, 2)
        if not early:
            return None  # the server waited for the body: nothing to announce
        headers = early.split(CRLF + CRLF)[0].decode(errors="replace").lower().splitlines()
        if any(h.startswith("connection:") and "close" in h for h in headers):
            return None
        try:
            s.sendall(body)
            time.sleep(0.2)
            s.sendall(b"GET /health HTTP/1.1" + CRLF + b"Host: oracle" + CRLF + CRLF)
            alive = bool(_recv(s, 2))
        except OSError:
            alive = False
    if alive:
        return None
    status = headers[0] if headers else "?"
    return (
        f"connection: a POST answered before its body was read ({status}) is followed by a "
        "close the response does not announce with `Connection: close`"
    )
