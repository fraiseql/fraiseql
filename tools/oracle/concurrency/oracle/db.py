"""Fixture database lifecycle and direct reads, so the runner needs no PostgreSQL client.

python -m oracle.db wait   <admin-url>
python -m oracle.db drop   <admin-url> <name>
"""

from __future__ import annotations

import re
import sys
import time
import urllib.parse
from typing import TYPE_CHECKING, Any

import psycopg
from psycopg import sql

if TYPE_CHECKING:
    from pathlib import Path

    from typing_extensions import LiteralString

NAME = re.compile(r"^[a-z_][a-z0-9_]{0,62}$")


def with_database(url: str, name: str) -> str:
    parts = urllib.parse.urlsplit(url)
    return urllib.parse.urlunsplit(parts._replace(path=f"/{name}"))


def _accepts(url: str) -> bool:
    try:
        with psycopg.connect(url, connect_timeout=2) as c:
            c.execute("select 1")
    except psycopg.OperationalError:
        return False
    return True


def wait(url: str) -> None:
    for _ in range(120):
        if _accepts(url):
            return
        time.sleep(0.5)
    msg = f"{url} did not accept connections within 60s"
    raise RuntimeError(msg)


def _checked(name: str) -> str:
    if not NAME.match(name):
        msg = f"bad database name: {name}"
        raise ValueError(msg)
    return name


def drop(admin: str, name: str) -> None:
    with psycopg.connect(admin, autocommit=True) as c:
        c.execute(
            sql.SQL("drop database if exists {} with (force)").format(
                sql.Identifier(_checked(name))
            )
        )


def create(admin: str, name: str) -> str:
    drop(admin, name)
    with psycopg.connect(admin, autocommit=True) as c:
        c.execute(sql.SQL("create database {}").format(sql.Identifier(_checked(name))))
    return with_database(admin, name)


def load(url: str, file: Path) -> None:
    with psycopg.connect(url, autocommit=True) as c:
        c.execute(file.read_text())  # ty: ignore[no-matching-overload]


def scalar(url: str, query: LiteralString, *params: Any) -> Any:
    """The first column of the first row of `query`, or None when there is no row."""
    with psycopg.connect(url, autocommit=True) as c:
        row = c.execute(query, params).fetchone()
    return None if row is None else row[0]


def execute(url: str, query: LiteralString, *params: Any) -> None:
    with psycopg.connect(url, autocommit=True) as c:
        c.execute(query, params)


def main(argv: list[str]) -> None:
    cmd, *args = argv
    if cmd == "wait":
        wait(args[0])
    elif cmd == "drop":
        drop(args[0], args[1])
    else:
        msg = f"unknown command: {cmd}"
        raise ValueError(msg)


if __name__ == "__main__":
    main(sys.argv[1:])
