#!/usr/bin/env python3
"""Pin first-party auth's tables to the `auth` schema.

WHY THIS GATE EXISTS
--------------------
First-party auth owns its tables in a schema of its own, `auth`, so that a separate
database role can own exactly that schema and nothing else. The DDL is one place; the
queries that read and write those tables are a few hundred string literals spread over
two crates, their tests and a conformance tool. An inventory test that runs the DDL
proves where the tables are *created*. It cannot see a query that still *reads* the old
location: that query fails only at runtime, and only on the path that runs it.

THREE RULES
-----------
1. No code string literal anywhere under `crates/` or `tools/` names an auth table in a
   schema it no longer lives in: qualified (`core.tb_user`, `_system.sessions`, …), or as
   a catalog lookup that filters on the old schema beside the table's name
   (`WHERE schemaname = 'core' AND tablename = 'tb_user'`).
2. Inside `crates/fraiseql-auth`, no code string literal names an auth table
   unqualified in a SQL table position (`FROM tb_user`, `INSERT INTO tb_otp_code`, …).
   An unqualified name resolves through `search_path`, which is the caller's, not auth's.
   Scoped to the auth crate on purpose: elsewhere `tb_user` is the conventional name of
   an *application* table, and the engine's own tests create one.
3. Every auth table is created in `auth` by a literal in `crates/fraiseql-auth/src`. This
   is the staleness check: if the DDL moves or is renamed, the gate refuses the tree
   rather than passing over a tree it can no longer see.

POSITIONAL, NOT TEXTUAL
-----------------------
Only the contents of string literals in code are matched. Comments, doc comments,
Markdown and the CHANGELOG are prose and may name the old location while explaining the
move. `_system.session_state` and the engine-owned `core.tb_api_key` /
`core.tb_admin_token` are not auth tables and stay where they are; a prefix
(`core.tb_user_secret`) is a different table.

WHAT THIS GATE DELIBERATELY DOES NOT REACH
------------------------------------------
* Go sources (`.dagger/`) name these tables only in comments.
* SQL text assembled at runtime from fragments (`format!("{schema}.{table}")`). No auth
  store does this today; the inventory test is the backstop for DDL built that way.
"""

from __future__ import annotations

import argparse
import io
import re
import subprocess
import sys
import tokenize
from pathlib import Path

AUTH_SCHEMA = "auth"

# The tables first-party auth owns. `session_state` is agent memory, not identity, and
# stays with the engine.
AUTH_TABLES = (
    "tb_user",
    "tb_auth_identity",
    "tb_password_credential",
    "tb_password_reset_token",
    "tb_email_verification_token",
    "tb_otp_code",
    "tb_otp_send_budget",
    "tb_mfa_enrollment",
    "tb_mfa_challenge",
    "tb_saml_replay",
    "tb_saml_idp",
    "tb_scim_group",
    "tb_scim_group_member",
    "tb_scim_token",
    "sessions",
)

FORMER_SCHEMAS = ("core", "_system")

_NAMES = "|".join(sorted(AUTH_TABLES, key=len, reverse=True))
_FORMER = "|".join(FORMER_SCHEMAS)

# Rule 1: `core.tb_user`, `"_system".sessions`, `core . tb_user`.
FORMER_QUALIFIED = re.compile(
    rf'(?<![\w.])"?(?:{_FORMER})"?\s*\.\s*"?({_NAMES})"?(?![\w])'
)

# Rule 1, the catalog form: `WHERE schemaname = 'core' AND tablename = 'tb_user'` names the
# old schema and the table apart, so no qualified name appears. Matched only in a literal
# that also names an auth table: the engine's own tables are looked up the same way.
FORMER_CATALOG_PREDICATE = re.compile(
    rf"\b(?:schemaname|nspname|table_schema|schema_name)\s*=\s*'(?:{_FORMER})'"
)
AUTH_TABLE_WORD = re.compile(rf"(?<![\w.])({_NAMES})(?![\w])")

# Rule 2: a table position followed by an unqualified auth table name. Uppercase only:
# the auth crate writes SQL keywords in uppercase, and lowercase prose such as
# "failed to update sessions" in an error message is not SQL.
UNQUALIFIED = re.compile(
    rf"\b(?:FROM|INTO|UPDATE|JOIN|REFERENCES|TABLE|ON|TRUNCATE|EXISTS|ONLY)\s+"
    rf'"?({_NAMES})"?(?![\w.])'
)

# Rule 3.
CREATED_IN_AUTH = re.compile(
    rf"CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS\s+{AUTH_SCHEMA}\.({_NAMES})(?![\w])"
)

SCANNED_SUFFIXES = (".rs", ".py")
ESCAPE = re.compile(r'\\(["\\])')
SELF = Path(__file__).resolve()


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def rust_string_literals(src: str) -> list[tuple[int, str]]:
    """Every string literal's (offset of its first content char, contents).

    Skips comments (line, block, nested block) and char literals, so a quote inside a
    comment or `'"'` does not open a string. Handles escapes and raw strings to their
    matching hash count; byte strings are the same shapes behind a `b`.
    """
    out: list[tuple[int, str]] = []
    i, n = 0, len(src)
    while i < n:
        ch = src[i]
        if src.startswith("//", i):
            nl = src.find("\n", i)
            i = n if nl == -1 else nl
            continue
        if src.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        prev_is_ident = i > 0 and (src[i - 1].isalnum() or src[i - 1] == "_")
        raw_at = i + 1 if src.startswith("br", i) else i
        if (
            not prev_is_ident
            and src[raw_at : raw_at + 1] == "r"
            and src[raw_at + 1 : raw_at + 2] in ('"', "#")
        ):
            j = raw_at + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                end = src.find(close, j + 1)
                end = n if end == -1 else end
                out.append((j + 1, src[j + 1 : end]))
                i = end + len(close)
                continue
        if ch == '"':
            start = i + 1
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    break
                i += 1
            # A quoted SQL identifier is written `\"core\"` in a plain literal. Escapes
            # are unescaped; that never removes a newline, so a match's line is the
            # literal's first line plus the newlines before it in the unescaped text.
            out.append((start, ESCAPE.sub(lambda m: m.group(1), src[start:i])))
            i += 1
            continue
        if ch == "'":
            # A lifetime (`'a`) is not a char literal; a char literal closes within 4 chars
            # (`'"'`, `'\n'`, `'\''`).
            if src.startswith("\\'", i + 1):
                i += 4
                continue
            end = src.find("'", i + 1)
            i = end + 1 if end != -1 and end - i <= 4 else i + 1
            continue
        i += 1
    return out


def python_string_literals(src: str) -> list[tuple[int, str]]:
    """Every string (and f-string text part) with its offset; comments are not strings."""
    line_offsets = [0]
    for line in src.splitlines(keepends=True):
        line_offsets.append(line_offsets[-1] + len(line))
    out: list[tuple[int, str]] = []
    fstring_middle = getattr(tokenize, "FSTRING_MIDDLE", None)
    for tok in tokenize.generate_tokens(io.StringIO(src).readline):
        if tok.type == tokenize.STRING or (fstring_middle and tok.type == fstring_middle):
            row, col = tok.start
            out.append((line_offsets[row - 1] + col, tok.string))
    return out


def literals(path: Path) -> list[tuple[int, str]]:
    src = path.read_text(encoding="utf-8")
    if path.suffix == ".py":
        return python_string_literals(src)
    return rust_string_literals(src)


def line_of(src: str, literal_offset: int, text: str, index: int) -> int:
    """Line of `text[index]`, `text` being the (unescaped) literal starting there."""
    return src.count("\n", 0, literal_offset) + text.count("\n", 0, index) + 1


def scanned_files(root: Path) -> list[Path]:
    files = []
    for top in ("crates", "tools"):
        base = root / top
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if path.suffix not in SCANNED_SUFFIXES or not path.is_file():
                continue
            if "/target/" in path.as_posix() or path.resolve() == SELF:
                continue
            files.append(path)
    return files


def check(root: Path) -> list[str]:
    failures: list[str] = []
    created: set[str] = set()
    auth_crate = root / "crates" / "fraiseql-auth"
    auth_src = auth_crate / "src"

    for path in scanned_files(root):
        rel = path.relative_to(root).as_posix()
        src = path.read_text(encoding="utf-8")
        in_auth_crate = auth_crate in path.parents
        for offset, text in literals(path):
            for m in FORMER_QUALIFIED.finditer(text):
                failures.append(
                    f"{rel}:{line_of(src, offset, text, m.start())}: `{m.group(0)}` names an "
                    f"auth table outside schema `{AUTH_SCHEMA}`; use "
                    f"`{AUTH_SCHEMA}.{m.group(1)}`"
                )
            table = AUTH_TABLE_WORD.search(text)
            if table:
                for m in FORMER_CATALOG_PREDICATE.finditer(text):
                    failures.append(
                        f"{rel}:{line_of(src, offset, text, m.start())}: `{m.group(0)}` "
                        f"looks up auth table `{table.group(1)}` in a schema it no longer "
                        f"lives in; use `'{AUTH_SCHEMA}'`"
                    )
            if in_auth_crate:
                for m in UNQUALIFIED.finditer(text):
                    failures.append(
                        f"{rel}:{line_of(src, offset, text, m.start())}: `{m.group(0)}` names "
                        f"an auth table unqualified; use `{AUTH_SCHEMA}.{m.group(1)}`"
                    )
            if auth_src in path.parents:
                created.update(m.group(1) for m in CREATED_IN_AUTH.finditer(text))

    failures.extend(
        f"crates/fraiseql-auth/src: no literal creates `{AUTH_SCHEMA}.{name}` "
        f"(`CREATE TABLE IF NOT EXISTS {AUTH_SCHEMA}.{name}`); the gate cannot see "
        "the auth DDL"
        for name in AUTH_TABLES
        if name not in created
    )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="tree to check (default: this repository)",
    )
    args = parser.parse_args()
    root = (args.root or repo_root()).resolve()

    failures = check(root)
    if failures:
        print(f"✗ auth tables must live in schema `{AUTH_SCHEMA}`:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(f"✓ auth tables: {len(AUTH_TABLES)} created in `{AUTH_SCHEMA}`, no stale reference")
    return 0


if __name__ == "__main__":
    sys.exit(main())
