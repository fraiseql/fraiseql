"""A local OpenID issuer for the oracle's principals.

The server validates bearer tokens only against an asymmetric issuer's JWKS, so the
oracle runs one. Its RSA key is generated for each run and lives in the run's temporary
directory: no key is ever committed.

    python -m oracle.issuer serve --key <path> --port <port>

writes a fresh key to <path> (mode 0600), serves the discovery document and the JWKS on
127.0.0.1:<port>, and prints its URL once it listens.
"""

from __future__ import annotations

import argparse
import json
import os
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING, Any

import jwt
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from jwt.algorithms import RSAAlgorithm

if TYPE_CHECKING:
    from cryptography.hazmat.primitives.asymmetric.rsa import RSAPrivateKey

AUDIENCE = "oracle-transports"
KID = "oracle-1"


def generate_key(path: Path) -> None:
    """Writes a new RSA key to `path`, readable by its owner only."""
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    pem = key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as f:
        f.write(pem)


def load_key(path: Path) -> RSAPrivateKey:
    key = serialization.load_pem_private_key(path.read_bytes(), None)
    if not isinstance(key, rsa.RSAPrivateKey):
        msg = f"{path} does not hold an RSA key"
        raise TypeError(msg)
    return key


def jwks(key: RSAPrivateKey) -> dict[str, Any]:
    jwk = json.loads(RSAAlgorithm.to_jwk(key.public_key()))
    jwk.update(kid=KID, use="sig", alg="RS256")
    return {"keys": [jwk]}


def mint(
    key: RSAPrivateKey,
    issuer: str,
    *,
    sub: str,
    tenant: str | None,
    roles: list[str] | None,
    exp_in: int = 600,
    audience: str = AUDIENCE,
) -> str:
    """A signed token for one principal. A negative `exp_in` mints an expired one."""
    now = int(time.time())
    claims: dict[str, Any] = {
        "sub": sub,
        "iss": issuer,
        "aud": audience,
        "iat": now - 5,
        "nbf": now - 5,
        "exp": now + exp_in,
        "jti": os.urandom(8).hex(),
    }
    if tenant is not None:
        claims["tenant_id"] = tenant
    if roles is not None:
        claims["roles"] = roles
    return jwt.encode(claims, key, algorithm="RS256", headers={"kid": KID})


def serve(key_path: Path, port: int) -> None:
    generate_key(key_path)
    issuer = f"http://127.0.0.1:{port}"
    keys = json.dumps(jwks(load_key(key_path))).encode()
    discovery = json.dumps({"issuer": issuer, "jwks_uri": f"{issuer}/jwks.json"}).encode()

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            if self.path == "/.well-known/openid-configuration":
                body = discovery
            elif self.path == "/jwks.json":
                body = keys
            else:
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

    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(issuer, flush=True)
    server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    s = sub.add_parser("serve")
    s.add_argument("--key", type=Path, required=True)
    s.add_argument("--port", type=int, required=True)
    args = parser.parse_args()
    serve(args.key, args.port)


if __name__ == "__main__":
    main()
