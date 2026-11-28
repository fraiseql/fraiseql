"""Probe cells: requests the corpus cannot express, graded like corpus cells.

The private module may export `PROBES`, a list of functions of a `ProbeContext`. Each
returns `ProbeCell`s: a request, the column it was sent on, the reference answer and the
answer under test. The oracle grades each cell exactly as it grades a corpus cell, so a
probe never grades itself: it supplies a reference, and `compare.compare_cell` decides.

A reference is POST on the same server, or the same column on the main server when the
probe measures what a server variant changes (a cell must not depend on the variant).

The context can start a server variant: the fixture's configuration, server settings or
schema, edited. A variant shares the fixture database and the issuer, and stops when its
`with` block ends.
"""

from __future__ import annotations

import contextlib
import json
import os
import re
import secrets
import subprocess
import time
from collections.abc import Callable, Generator
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

import httpx
import jwt

from .clients import Clients, Endpoints, Response
from .issuer import AUDIENCE, KID, load_key

if TYPE_CHECKING:
    from . import generate as gen

FIXTURE = Path(__file__).resolve().parents[1] / "fixture"
BOOT_WAIT = 60.0


@dataclass
class ProbeCell:
    """One graded cell. `request` names the cell and carries the selected fields."""

    request: gen.Request
    column: str
    ref: Response
    got: Response


@dataclass
class Binaries:
    cli: Path
    server: Path
    work: Path


@dataclass
class ProbeContext:
    main: Endpoints
    issuer: str
    key_path: Path
    db_url: str
    tokens: dict[str, str | None]
    binaries: Binaries | None
    _clients: list[Clients] = field(default_factory=list)

    def clients(self, ends: Endpoints | None = None) -> Clients:
        """Clients for the main server, or a variant's. Closed after the probes run."""
        c = Clients(ends or self.main)
        self._clients.append(c)
        return c

    def claims(self, **override: Any) -> dict[str, Any]:
        """The claims the issuer signs for a principal, with `override` applied. A value
        of None removes the claim."""
        now = int(time.time())
        claims: dict[str, Any] = {
            "sub": "probe",
            "iss": self.issuer,
            "aud": AUDIENCE,
            "iat": now - 5,
            "nbf": now - 5,
            "exp": now + 600,
            "jti": os.urandom(8).hex(),
        }
        claims.update(override)
        return {k: v for k, v in claims.items() if v is not None}

    def sign(self, claims: dict[str, Any], *, key: Any = None, kid: str = KID) -> str:
        """`claims`, signed RS256 with the issuer's key, or with `key`."""
        return jwt.encode(
            claims, key or load_key(self.key_path), algorithm="RS256", headers={"kid": kid}
        )

    @contextlib.contextmanager
    def variant(
        self,
        name: str,
        *,
        config: Callable[[str], str] = lambda s: s,
        server: dict[str, Any] | None = None,
        schema: Callable[[dict[str, Any]], dict[str, Any]] = lambda d: d,
    ) -> Generator[Endpoints]:
        """A server built from the fixture with `config` applied to fraiseql.toml's
        text, `server` setting top-level keys of the server's TOML, and `schema` applied
        to schema.json. Started on free ports; stopped when the block ends."""
        if self.binaries is None:
            msg = "server variants need --cli, --server and --work"
            raise RuntimeError(msg)
        b = self.binaries
        d = b.work / f"variant-{name}"
        d.mkdir(parents=True, exist_ok=False)
        (d / "fraiseql.toml").write_text(config((FIXTURE / "fraiseql.toml").read_text()))
        (d / "schema.json").write_text(
            json.dumps(schema(json.loads((FIXTURE / "schema.json").read_text())))
        )
        compiled = subprocess.run(
            [
                str(b.cli),
                "compile",
                "fraiseql.toml",
                "--types",
                "schema.json",
                "-o",
                "schema.compiled.json",
            ],
            cwd=d,
            capture_output=True,
            text=True,
            check=False,
        )
        if compiled.returncode:
            msg = f"variant {name}: compile failed: {compiled.stdout}{compiled.stderr}"
            raise RuntimeError(msg)
        http_port, flight_port = _free_port(), _free_port()
        text = (FIXTURE / "server.toml.in").read_text()
        for k, v in {
            "@HTTP_PORT@": str(http_port),
            "@FLIGHT_PORT@": str(flight_port),
            "@SCHEMA@": str(d / "schema.compiled.json"),
            "@DATABASE_URL@": self.db_url,
            "@ISSUER@": self.issuer,
        }.items():
            text = text.replace(k, v)
        (d / "server.toml").write_text(_set_keys(text, server or {}))
        env = {
            **os.environ,
            "FRAISEQL_ENV": "development",
            "FLIGHT_SESSION_SECRET": secrets.token_hex(32),
        }
        log = (d / "server.log").open("w")
        proc = subprocess.Popen(
            [str(b.server), "--config", str(d / "server.toml")], stdout=log, stderr=log, env=env
        )
        try:
            http = f"http://127.0.0.1:{http_port}"
            _wait_healthy(http, proc, d / "server.log")
            yield Endpoints(http, f"grpc://127.0.0.1:{flight_port}")
        finally:
            proc.terminate()
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            log.close()

    def close(self) -> None:
        for c in self._clients:
            c.close()


def _set_keys(text: str, keys: dict[str, Any]) -> str:
    """Set top-level `key = value` lines of a TOML text. A key it does not have is an
    error: a setting that silently does not apply would make the variant the main server."""
    head, sep, rest = text.partition("\n[")
    for k, v in keys.items():
        line = f"{k} = {json.dumps(v)}"
        head, n = re.subn(rf"(?m)^{re.escape(k)} = .*$", line, head)
        if n != 1:
            msg = f"the server template has no top-level `{k}`"
            raise RuntimeError(msg)
    return head + sep + rest


def _free_port() -> int:
    import socket  # noqa: PLC0415 - only here

    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _wait_healthy(http: str, proc: subprocess.Popen, log: Path) -> None:
    deadline = time.monotonic() + BOOT_WAIT
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            msg = f"a variant server exited during boot:\n{log.read_text()[-4000:]}"
            raise RuntimeError(msg)
        with contextlib.suppress(httpx.HTTPError):
            if httpx.get(f"{http}/health", timeout=2).status_code == 200:
                return
        time.sleep(0.1)
    msg = f"a variant server did not answer /health within {BOOT_WAIT}s"
    raise RuntimeError(msg)


Probe = Callable[[ProbeContext], list[ProbeCell]]


def run(ctx: ProbeContext, probes: list[Probe]) -> list[ProbeCell]:
    try:
        return [cell for p in probes for cell in p(ctx)]
    finally:
        ctx.close()
