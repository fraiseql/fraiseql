"""A rewriting HTTP proxy, for the self-test: it simulates a server that changed.

Every request is forwarded to the target unchanged. `rewrite(method, path, status, body)`
may return a replacement `(status, body)` for the response, or None to pass it through.
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import httpx

Rewrite = Callable[[str, str, int, bytes], "tuple[int, bytes] | None"]
HOP = {"connection", "keep-alive", "transfer-encoding", "content-length", "content-encoding"}


class Proxy:
    def __init__(self, target: str, rewrite: Rewrite) -> None:
        client = httpx.Client(timeout=30)
        self.rewrites = 0
        proxy = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def _forward(self) -> None:
                length = int(self.headers.get("content-length") or 0)
                body = self.rfile.read(length) if length else None
                headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
                url = target + self.path
                try:
                    r = client.request(self.command, url, headers=headers, content=body)
                except httpx.RemoteProtocolError:
                    # A kept-alive connection closed before any answer (see protocol.py):
                    # once more on a new connection, as the oracle's own clients do.
                    r = client.request(self.command, url, headers=headers, content=body)
                status, out = r.status_code, r.content
                changed = rewrite(self.command, self.path, status, out)
                if changed is not None:
                    status, out = changed
                    proxy.rewrites += 1
                self.send_response(status)
                for k, v in r.headers.items():
                    if k.lower() not in HOP:
                        self.send_header(k, v)
                self.send_header("content-length", str(len(out)))
                self.end_headers()
                self.wfile.write(out)

            # The handler's method names are fixed by BaseHTTPRequestHandler.
            do_GET = _forward
            do_POST = _forward
            do_DELETE = _forward

            def log_message(self, format: str, *args: object) -> None:
                del format, args

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        self._thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self._thread.start()
        self._client = client

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self._client.close()
