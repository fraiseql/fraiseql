"""A local OpenID issuer for the oracle's principals.

The server validates bearer tokens only against an asymmetric issuer's JWKS, so the
oracle serves one from a thread. Its RSA key is generated for each run and is never
written to disk.
"""

from __future__ import annotations

import json
import os
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

import jwt
from cryptography.hazmat.primitives.asymmetric import rsa
from jwt.algorithms import RSAAlgorithm

AUDIENCE = "oracle-concurrency"
KID = "oracle-1"
TENANT_A = "0000000a-0000-4000-8000-000000000000"
TENANT_B = "0000000b-0000-4000-8000-000000000000"


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Issuer:
    """Serves discovery and the JWKS on 127.0.0.1, and mints tokens signed by its key."""

    def __init__(self) -> None:
        self._key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        port = free_port()
        self.url = f"http://127.0.0.1:{port}"
        jwk = json.loads(RSAAlgorithm.to_jwk(self._key.public_key()))
        jwk.update(kid=KID, use="sig", alg="RS256")
        keys = json.dumps({"keys": [jwk]}).encode()
        discovery = json.dumps({"issuer": self.url, "jwks_uri": f"{self.url}/jwks.json"}).encode()
        bodies = {"/.well-known/openid-configuration": discovery, "/jwks.json": keys}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                body = bodies.get(self.path)
                if body is None:
                    self.send_response(404)
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                del format, args

        self._server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
        threading.Thread(target=self._server.serve_forever, daemon=True).start()
        self._tokens: dict[tuple[str, str, tuple[str, ...]], str] = {}

    def token(
        self, sub: str = "alice", tenant: str = TENANT_A, roles: tuple[str, ...] = ("viewer",)
    ) -> str:
        """A token for one principal, valid for the run; the same principal gets the same token."""
        k = (sub, tenant, roles)
        if k not in self._tokens:
            now = int(time.time())
            claims: dict[str, Any] = {
                "sub": sub,
                "iss": self.url,
                "aud": AUDIENCE,
                "iat": now - 5,
                "nbf": now - 5,
                "exp": now + 7200,
                "jti": os.urandom(8).hex(),
                "tenant_id": tenant,
                "roles": list(roles),
            }
            self._tokens[k] = jwt.encode(claims, self._key, algorithm="RS256", headers={"kid": KID})
        return self._tokens[k]

    def close(self) -> None:
        self._server.shutdown()
