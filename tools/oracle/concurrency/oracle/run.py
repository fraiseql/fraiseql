"""One oracle run: the fixture database, compiled schemas and server processes."""

from __future__ import annotations

import contextlib
import json
import os
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

import httpx

from oracle import db, schema
from oracle.issuer import AUDIENCE, Issuer, free_port

if TYPE_CHECKING:
    from collections.abc import Generator

HERE = Path(__file__).resolve().parent.parent
FIXTURE = HERE / "fixture"


class SetupError(RuntimeError):
    """The run could not be set up: nothing was judged."""


@dataclass
class Run:
    cli: Path
    server: Path
    work: Path
    admin_url: str
    db_name: str = "oracle_concurrency"
    primary_url: str = ""
    replica_url: str = ""
    issuer: Issuer = field(default_factory=Issuer)
    db_url: str = ""
    _compiled: dict[tuple[int | None, bool], Path] = field(default_factory=dict)

    def setup_database(self, admin_url: str | None = None, name: str | None = None) -> str:
        """Creates and loads a fixture database; returns its URL."""
        url = db.create(admin_url or self.admin_url, name or self.db_name)
        db.load(url, FIXTURE / "db.sql")
        log = self.work / f"setup-{name or self.db_name}.log"
        with log.open("w") as out:
            rc = subprocess.run(
                [str(self.cli), "setup"],
                cwd=self.work,
                env={**os.environ, "DATABASE_URL": url},
                stdout=out,
                stderr=subprocess.STDOUT,
                check=False,
            ).returncode
        if rc != 0:
            msg = f"fraiseql-cli setup failed:\n{log.read_text()[-3000:]}"
            raise SetupError(msg)
        db.load(url, FIXTURE / "fn.sql")
        return url

    def compiled(self, ttl: int | None, *, observers: bool = False) -> Path:
        """The compiled schema with every query at `ttl` (None: no TTL).

        `observers` turns on the observer runtime, which feeds subscriptions from the
        change log.
        """
        if (ttl, observers) not in self._compiled:
            tag = ("none" if ttl is None else str(ttl)) + (".observers" if observers else "")
            types = self.work / f"schema.{tag}.json"
            types.write_text(json.dumps(schema.build(ttl), indent=1))
            project = FIXTURE / "fraiseql.toml"
            if observers:
                project = self.work / "fraiseql.observers.toml"
                project.write_text(
                    (FIXTURE / "fraiseql.toml").read_text()
                    + '\n[observers]\nenabled = true\nbackend = "postgres"\n'
                )
            out = self.work / f"compiled.{tag}.json"
            proc = subprocess.run(
                [
                    str(self.cli),
                    "compile",
                    str(project),
                    "--types",
                    str(types),
                    "-o",
                    str(out),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            if proc.returncode != 0:
                msg = f"fraiseql-cli compile failed:\n{proc.stdout[-2000:]}{proc.stderr[-2000:]}"
                raise SetupError(msg)
            self._compiled[ttl, observers] = out
        return self._compiled[ttl, observers]

    @contextlib.contextmanager
    def serve(
        self,
        name: str,
        *,
        cache: bool = True,
        ttl: int | None = 0,
        top: str = "",
        database_url: str | None = None,
        observers: bool = False,
    ) -> Generator[str]:
        """Starts a server; yields its base URL and stops it on exit."""
        port = free_port()
        config = self.work / f"server.{name}.toml"
        config.write_text(
            "\n".join(
                [
                    f'bind_addr = "127.0.0.1:{port}"',
                    f'schema_path = "{self.compiled(ttl, observers=observers)}"',
                    f'database_url = "{database_url or self.db_url}"',
                    f"cache_enabled = {'true' if cache else 'false'}",
                    "subscriptions_enabled = true",
                    top,
                    "",
                    "[auth]",
                    f'issuer = "{self.issuer.url}"',
                    f'audience = "{AUDIENCE}"',
                    f'jwks_uri = "{self.issuer.url}/jwks.json"',
                    'allowed_algorithms = ["RS256"]',
                    "required = false",
                    "",
                    "[rate_limiting]",
                    "enabled = false",
                    "",
                ]
            )
        )
        log = self.work / f"server.{name}.log"
        with log.open("w") as out:
            proc = subprocess.Popen(
                [str(self.server), "--config", str(config)],
                env={**os.environ, "FRAISEQL_ENV": "development", "RUST_LOG": "warn"},
                stdout=out,
                stderr=subprocess.STDOUT,
            )
        base = f"http://127.0.0.1:{port}"
        try:
            _wait_healthy(base, proc, log)
            yield base
        finally:
            proc.terminate()
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def _wait_healthy(base: str, proc: subprocess.Popen[bytes], log: Path) -> None:
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            msg = f"fraiseql-server exited during boot:\n{log.read_text()[-4000:]}"
            raise SetupError(msg)
        with contextlib.suppress(httpx.HTTPError):
            if httpx.get(base + "/health", timeout=1).status_code == 200:
                return
        time.sleep(0.1)
    msg = f"fraiseql-server did not become healthy in 60s:\n{log.read_text()[-4000:]}"
    raise SetupError(msg)
