"""Fixture database lifecycle, so the runner needs no PostgreSQL client tools.

python -m oracle.db wait   <admin-url>
python -m oracle.db create <admin-url> <name>    prints the new database's URL
python -m oracle.db load   <url> <file.sql>
python -m oracle.db drop   <admin-url> <name>
"""

from __future__ import annotations

import re
import sys
import time
import urllib.parse
from pathlib import Path

import psycopg
from psycopg import sql

NAME = re.compile(r"^[a-z_][a-z0-9_]{0,62}$")


def with_database(url: str, name: str) -> str:
    parts = urllib.parse.urlsplit(url)
    return urllib.parse.urlunsplit(parts._replace(path=f"/{name}"))


def _accepts(admin: str) -> bool:
    try:
        with psycopg.connect(admin, connect_timeout=2) as c:
            c.execute("select 1")
    except psycopg.OperationalError:
        return False
    return True


def wait(admin: str) -> None:
    for _ in range(120):
        if _accepts(admin):
            return
        time.sleep(0.5)
    msg = "the database did not accept connections within 60s"
    raise RuntimeError(msg)


def drop(admin: str, name: str) -> None:
    with psycopg.connect(admin, autocommit=True) as c:
        c.execute(sql.SQL("drop database if exists {} with (force)").format(sql.Identifier(name)))


def create(admin: str, name: str) -> str:
    drop(admin, name)
    with psycopg.connect(admin, autocommit=True) as c:
        c.execute(sql.SQL("create database {}").format(sql.Identifier(name)))
    return with_database(admin, name)


def load(url: str, file: Path) -> None:
    with psycopg.connect(url, autocommit=True) as c:
        c.execute(file.read_text())  # ty: ignore[no-matching-overload]


def main(argv: list[str]) -> None:
    cmd, *args = argv
    if cmd == "wait":
        wait(args[0])
        return
    if cmd == "load":
        load(args[0], Path(args[1]))
        return
    admin, name = args
    if not NAME.match(name):
        msg = f"bad database name: {name}"
        raise ValueError(msg)
    if cmd == "drop":
        drop(admin, name)
    elif cmd == "create":
        print(create(admin, name))
    else:
        msg = f"unknown command: {cmd}"
        raise ValueError(msg)


if __name__ == "__main__":
    main(sys.argv[1:])
