#!/usr/bin/env python3
"""Every config table reachable from `ServerConfig` or `TomlSchema` refuses unknown keys.

WHY THIS GATE EXISTS
--------------------
#839 put `#[serde(deny_unknown_fields)]` on `ServerConfig`. serde does **not** propagate
that into nested structs, so for three releases every `[section]` whose own struct lacked
it accepted a mistyped key and discarded it silently. `[rate_limiting] enabeld = true`
booted the section on its defaults — and since `enabled` defaults to `true`, the operator
saw exactly the behaviour they intended while having configured nothing. `[auth]
require_jti`, `[tls] require_client_cert` and `[rate_limiting] enabled` are security
switches that sat at their defaults when misspelled.

The set of sections is **discovered** from `ServerConfig`'s fields rather than listed,
because a list is the same silence one layer up: a new section would join without anyone
noticing it was unchecked. The discovered count is asserted for the same reason — if the
walk starts finding fewer structs (a renamed field, a parser regression), that is a gate
quietly measuring less, not a tree that got simpler.

WHY IT RESOLVES WITHIN THE RUNTIME CRATES
-----------------------------------------
`fraiseql-cli` keeps a parallel set of config types for the TOML authoring surface.
`ValidationConfig` and `RateLimitConfig` exist in both, and the CLI's copies already
carried the attribute while the runtime ones did not. A name-keyed sweep answers about
whichever file it walked first — which is how #1337's own table and this gate's first
draft each mis-classified the same two structs, in opposite directions. An ambiguous name
is reported, never guessed.

WHY A HAND-WRITTEN `Deserialize` NAMES ITS TEST
-----------------------------------------------
The derive check reads attributes. A struct whose `impl Deserialize` is written by hand has
none to read, so the walk counted it and checked nothing: an impl that drops the keys it does
not recognise would pass. What such an impl does with an unknown key is code, not an
attribute, so each one reached is listed in `tools/config-deny-unknown-hand-written.txt`
beside the test that feeds it an unknown key and requires the refusal. The gate checks that
the test exists and runs (`#[test]`, not `#[ignore]`), that every hand-written impl the walk
reaches is listed, and that every listed struct is still one: an entry that outlives its
struct is a pin on nothing.

WHY A SECOND WALK FROM `TomlSchema`
-----------------------------------
`fraiseql.toml` is read into `TomlSchema`, through types of `fraiseql-cli`, `fraiseql-core`
and `fraiseql-db`. Two tables it reaches compiled a key they do not take and dropped it
(#2051): `VectorConfig` had no `deny_unknown_fields`, and `SessionVariableMapping` flattened
its source, which serde cannot deny through. The second walk starts at `TomlSchema` and
requires every table with keys to deny unknown ones, flags a `#[serde(flatten)]` as the
reason it cannot, and follows `#[serde(from = "X")]` to the type the table is read through.
A table whose keys the author chooses is listed in `tools/config-deny-unknown-open-tables.txt`
by type and file, with the reason; an entry for a table that now denies unknown keys, or
that the walk no longer reaches, fails.

Several crates define a config type of the same name (`TypeDefinition`, `ObserversConfig`),
so this walk reads a name as rustc would: by its written crate path, else by the `use`
that brings it in, else in the crate that names it. A name still left with several
definitions is reported, never guessed. `--only=server` / `--only=toml` run one walk.
"""

from __future__ import annotations

import collections
import dataclasses
import os
import pathlib
import re
import sys

# `fraiseql-cli` is excluded: see the module docstring. Its config types are a different
# surface that happens to share names.
EXCLUDED_CRATES = {"fraiseql-cli"}

# The walk starts here and follows struct-typed fields.
ROOT_STRUCT = os.environ.get("CONFIG_DENY_UNKNOWN_ROOT_STRUCT", "ServerConfig")

# Each hand-written `Deserialize` the walk reaches, with the test that pins its refusal of an
# unknown key: `<struct> <test file> <test fn>` per line.
HAND_WRITTEN = pathlib.Path(
    os.environ.get(
        "CONFIG_DENY_UNKNOWN_HAND_WRITTEN", "tools/config-deny-unknown-hand-written.txt"
    )
)

# The discovered-struct count, asserted so the walk cannot quietly measure less.
# Bump it deliberately, with the section that was added.
EXPECTED_REACHABLE = int(os.environ.get("CONFIG_DENY_UNKNOWN_EXPECTED", "54"))
# +1: [application_pool] (#2098) sizes the application database pool.
# +1: [function_dispatch] (#1628) leases and sweeps the function dispatch ledger.
# +1 (#2033): `[rate_limiting]` is `ServerRateLimitingConfig` over the compiled
# `RateLimitingSecurityConfig`, replacing `RateLimitConfig`. Its `Deserialize` is written by
# hand, and listed with its pinning test in `HAND_WRITTEN`.
# +1 (TOML walk): `ConnectionCap`, a `[subscriptions]` connection bound, is a number or the
# string "unlimited", never a table, so it has no key to be unknown.
# +2 (TOML walk, #1607): `[accent_insensitive]` is `AccentInsensitiveConfig`, which refuses
# unknown keys, and its `function` is `UnaccentFunction`, read from a string, never a table.

# The second walk starts at the type `fraiseql.toml` is read into, across the crates that
# define the types it reaches.
TOML_ROOT = os.environ.get("CONFIG_DENY_UNKNOWN_TOML_ROOT", "TomlSchema")
TOML_CRATES = set(
    os.environ.get(
        "CONFIG_DENY_UNKNOWN_TOML_CRATES", "fraiseql-cli,fraiseql-core,fraiseql-db"
    ).split(",")
)
TOML_EXPECTED_REACHABLE = int(os.environ.get("CONFIG_DENY_UNKNOWN_TOML_EXPECTED", "98"))

# Each table `TomlSchema` reaches that is open on purpose, its keys chosen by the author:
# `<type> <file> <reason>` per line.
OPEN_TABLES = pathlib.Path(
    os.environ.get("CONFIG_DENY_UNKNOWN_OPEN_TABLES", "tools/config-deny-unknown-open-tables.txt")
)

# Types that are not config structs of ours.
LEAF_TYPES = {
    "String", "Option", "Vec", "HashMap", "BTreeMap", "Duration", "PathBuf", "Arc",
    "Value", "SocketAddr", "IpAddr", "Result", "Box", "Cow", "DateTime", "Utc",
    "NonZeroUsize", "Secret",
}

STRUCT_RE = re.compile(
    r"((?:^[ \t]*#\[[^\n]*\]\n)*)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)", re.M
)
FIELD_RE = re.compile(r":\s*(.+?),\s*$")
REASON_RE = re.compile(r"^\s*//\s*Reason\b", re.M)
# `impl<'de> Deserialize<'de> for Name`, with any generics and any path to the trait.
HAND_WRITTEN_RE = re.compile(
    r"\bimpl\s*<[^>]*'de[^>]*>\s*(?:[\w:]+::)?Deserialize\s*<\s*'de\s*>\s+for\s+(\w+)"
)


def struct_body(source: str, start: int) -> str:
    """The brace-balanced body beginning at the first `{` after `start`."""
    open_at = source.find("{", start)
    if open_at == -1:
        return ""
    depth, i = 0, open_at
    while i < len(source):
        if source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
            if depth == 0:
                return source[open_at : i + 1]
        i += 1
    return source[open_at:]


def runtime_sources() -> list[tuple[pathlib.Path, str]]:
    """Every non-test source file of the runtime crates, with its text."""
    out = []
    for path in sorted(pathlib.Path("crates").rglob("*.rs")):
        if path.parts[1] in EXCLUDED_CRATES:
            continue
        if "/tests/" in str(path) or path.name.endswith("tests.rs"):
            continue
        out.append((path, path.read_text(errors="replace")))
    return out


def index_structs(
    sources: list[tuple[pathlib.Path, str]],
) -> dict[str, list[tuple[pathlib.Path, str, str, str]]]:
    """name -> [(path, attribute block, body, the ~12 lines above the attributes)]."""
    out: dict[str, list[tuple[pathlib.Path, str, str, str]]] = collections.defaultdict(list)
    for path, source in sources:
        for match in STRUCT_RE.finditer(source):
            attrs, name = match.group(1), match.group(2)
            preamble = source[max(0, match.start() - 900) : match.start()]
            out[name].append((path, attrs, struct_body(source, match.end()), preamble))
    return out


def index_hand_written(sources: list[tuple[pathlib.Path, str]]) -> dict[str, pathlib.Path]:
    """name -> the file holding its hand-written `impl Deserialize`."""
    return {
        match.group(1): path
        for path, source in sources
        for match in HAND_WRITTEN_RE.finditer(source)
    }


def read_pins() -> tuple[dict[str, tuple[str, str]], list[str]]:
    """struct -> (test file, test fn), from `HAND_WRITTEN`; and its malformed lines."""
    pins: dict[str, tuple[str, str]] = {}
    malformed: list[str] = []
    if not HAND_WRITTEN.is_file():
        return pins, malformed
    for line in HAND_WRITTEN.read_text().splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        fields = line.split()
        if len(fields) != 3:
            malformed.append(line)
            continue
        pins[fields[0]] = (fields[1], fields[2])
    return pins, malformed


def pin_problem(test_file: str, test_fn: str) -> str | None:
    """Why `test_fn` in `test_file` does not pin anything, or None when it is a running test."""
    path = pathlib.Path(test_file)
    if not path.is_file():
        return f"its test file {test_file} does not exist"
    source = path.read_text(errors="replace")
    match = re.search(
        r"((?:^[ \t]*#\[[^\n]*\]\n|^[ \t]*///[^\n]*\n)*)^[ \t]*(?:pub\s+)?(?:async\s+)?fn\s+"
        + re.escape(test_fn)
        + r"\s*\(",
        source,
        re.M,
    )
    if match is None:
        return f"its test `{test_fn}` is not in {test_file}"
    attrs = match.group(1)
    if not re.search(r"#\[(?:tokio::)?test\b", attrs):
        return f"`{test_fn}` in {test_file} is not a `#[test]`"
    if "#[ignore" in attrs:
        return f"`{test_fn}` in {test_file} is `#[ignore]`d, so it pins nothing"
    return None


def field_type_names(body: str) -> list[str]:
    names = []
    for line in body.split("\n"):
        line = line.split("//")[0]
        match = FIELD_RE.search(line)
        if match:
            names.extend(re.findall(r"\b([A-Z]\w+)\b", match.group(1)))
    return names


def server_walk(pins: dict[str, tuple[str, str]]) -> tuple[int, set[str]]:
    """The walk from `ServerConfig`: its exit code, and the hand-written structs it reached."""
    sources = runtime_sources()
    defs = index_structs(sources)
    hand_written = index_hand_written(sources)
    if ROOT_STRUCT not in defs:
        print(f"ERROR: {ROOT_STRUCT} not found — this gate is measuring nothing.")
        return 1, set()

    seen: set[str] = set()
    violations: list[str] = []
    ambiguous: list[str] = []
    exempt: list[str] = []
    unpinned: list[str] = []

    queue = collections.deque([ROOT_STRUCT])
    while queue:
        name = queue.popleft()
        if name in seen or name in LEAF_TYPES or name not in defs:
            continue
        seen.add(name)
        candidates = defs[name]
        if len(candidates) > 1:
            ambiguous.append(
                f"{name}: " + ", ".join(str(p) for p, _, _, _ in candidates)
            )
            continue
        path, attrs, body, preamble = candidates[0]

        # Only a struct that is actually deserialized has an unknown-key surface.
        # `RateLimitOverrides` is reachable from `ServerConfig` and derives neither
        # `Serialize` nor `Deserialize` — it is built from CLI flags.
        if name in hand_written:
            pin = pins.get(name)
            if pin is None:
                unpinned.append(
                    f"{name:30s} {hand_written[name]}  — its `Deserialize` is written by "
                    "hand and no test is listed for it"
                )
            elif problem := pin_problem(*pin):
                unpinned.append(f"{name:30s} {hand_written[name]}  — {problem}")
        elif "Deserialize" in attrs:
            if "deny_unknown_fields" not in attrs:
                if REASON_RE.search(preamble):
                    exempt.append(f"{name} ({path})")
                else:
                    violations.append(f"{name:30s} {path}")
            if "flatten" in body and "deny_unknown_fields" in attrs:
                violations.append(
                    f"{name:30s} {path}  — `deny_unknown_fields` does not compose with "
                    "`#[serde(flatten)]`; serde silently drops the flattened keys"
                )

        for type_name in field_type_names(body):
            if type_name not in seen:
                queue.append(type_name)

    if ambiguous:
        print("ERROR: a config struct name resolves to several runtime definitions (#1337):")
        for line in ambiguous:
            print(f"  {line}")
        print()
        print("This gate refuses to guess, because guessing is how #1337's own table and")
        print("this gate's first draft each mis-classified the same two structs. Rename")
        print("one, or teach the walk to resolve by module path.")
        return 1, set()

    if violations:
        print("ERROR: config structs reachable from ServerConfig accept unknown keys (#1337):")
        for line in violations:
            print(f"  {line}")
        print()
        print("serde does not propagate `deny_unknown_fields` into nested structs, so each")
        print("section struct needs its own. Without it a mistyped key is discarded and the")
        print("setting stays at its default, silently — including security switches.")
        print()
        print("Add `#[serde(deny_unknown_fields)]`, or — if the struct legitimately carries")
        print("keys another producer owns — a `// Reason:` comment above it saying which,")
        print("as `[observers]` does for the keys the compiler writes.")
        return 1, set()

    reached = {name for name in seen if name in hand_written}
    if unpinned:
        print("ERROR: a hand-written `Deserialize` reachable from ServerConfig is not pinned:")
        for line in unpinned:
            print(f"  {line}")
        print()
        print("This gate cannot read what a hand-written impl does with an unknown key. List")
        print(f"each one in {HAND_WRITTEN} as `<struct> <test file> <test fn>`, naming a")
        print("test that feeds it an unknown key and requires the refusal.")
        return 1, reached

    if len(seen) != EXPECTED_REACHABLE:
        print(
            f"ERROR: the walk reached {len(seen)} structs, expected {EXPECTED_REACHABLE}."
        )
        print()
        print("More means a new section joined — check it and bump EXPECTED_REACHABLE.")
        print("FEWER means this gate is now measuring less than it was, which looks")
        print("identical to a clean tree. That is the failure this count exists to catch.")
        return 1, reached

    print(
        f"OK: {len(seen)} config structs reachable from {ROOT_STRUCT}; "
        f"every deserialized one refuses unknown keys "
        f"({len(exempt)} exempt with a stated reason, {len(reached)} written by hand and "
        f"pinned by a test)"
    )
    return 0, reached



# ── the walk from `TomlSchema` ───────────────────────────────────────────────────────────


@dataclasses.dataclass(frozen=True)
class Item:
    """A struct or enum definition: where it is, its attributes and its body."""

    path: pathlib.Path
    kind: str
    name: str
    attrs: str
    body: str

    @property
    def crate(self) -> str:
        return self.path.parts[1]


ITEM_RE = re.compile(
    r"((?:^[ \t]*(?:#\[|//)[^\n]*\n)*)"
    r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(struct|enum)\s+(\w+)",
    re.M,
)
USE_RE = re.compile(r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);", re.M)
FROM_RE = re.compile(r"\b(?:try_)?from\s*=\s*\"(\w+)\"")
TYPE_RE = re.compile(r"((?:\w+::)*)\b([A-Z]\w*)\b")
# `name:`, not the `::` of a path.
FIELD_NAME_RE = re.compile(r"(?<![:\w])\w+\s*:(?!:)")


def toml_sources() -> list[tuple[pathlib.Path, str]]:
    """Every non-test source file of the crates `fraiseql.toml` is read through."""
    out = []
    for path in sorted(pathlib.Path("crates").rglob("*.rs")):
        if path.parts[1] not in TOML_CRATES:
            continue
        if "/tests/" in str(path) or path.name.endswith("tests.rs"):
            continue
        out.append((path, path.read_text(errors="replace")))
    return out


def index_items(sources: list[tuple[pathlib.Path, str]]) -> dict[str, list[Item]]:
    out: dict[str, list[Item]] = collections.defaultdict(list)
    for path, source in sources:
        for match in ITEM_RE.finditer(source):
            attrs = "\n".join(
                line for line in match.group(1).split("\n") if line.strip().startswith("#[")
            )
            # A tuple or unit struct ends at its `;`, before any brace.
            rest = source[match.end() :]
            brace, semi = rest.find("{"), rest.find(";")
            body = "" if semi != -1 and (brace == -1 or semi < brace) else struct_body(
                source, match.end()
            )
            out[match.group(3)].append(Item(path, match.group(2), match.group(3), attrs, body))
    return out


def crate_of_root(root: str, here: str) -> str | None:
    """The crate a path root names, seen from crate `here`."""
    if root in ("crate", "super", "self"):
        return here
    if root.startswith("fraiseql_"):
        return root.replace("_", "-")
    return None


def definitions_of(
    name: str, prefix: str, item: Item, uses: dict[pathlib.Path, list[str]],
    defs: dict[str, list[Item]],
) -> list[Item]:
    """The definitions `name`, written in `item` after the path `prefix`, can stand for.

    Several crates define a config type of the same name, so a bare name is read as rustc
    would: by its written path, else by the `use` that brings it in, else in its own crate.
    """
    candidates = defs.get(name, [])
    if len(candidates) <= 1:
        return candidates
    crate = crate_of_root(prefix.split("::")[0], item.crate) if prefix else None
    if crate is None:
        for use in uses[item.path]:
            if re.search(rf"\b{re.escape(name)}\b", use):
                crate = crate_of_root(use.strip().split("::")[0].lstrip("{").strip(), item.crate)
                break
    if crate is None:
        crate = item.crate
    in_crate = [candidate for candidate in candidates if candidate.crate == crate]
    return in_crate or candidates


def code(body: str) -> str:
    """`body` without its comments."""
    return "\n".join(line.split("//")[0] for line in body.split("\n"))


SERDE_FLATTEN_RE = re.compile(r"#\[serde\([^\]]*\bflatten\b")


def has_keys(item: Item) -> bool:
    """Whether a value of `item` is a table with keys: a struct with named fields, or an enum
    with a struct variant. A unit enum is a string; a tuple struct is its inner value."""
    inner = code(item.body)[1:-1]
    if item.kind == "struct":
        return bool(item.body.startswith("{") and re.search(r"^\s*(?:pub\S*\s+)?\w+\s*:", inner, re.M))
    return "{" in inner


def referenced_types(item: Item) -> list[tuple[str, str]]:
    """`(path prefix, name)` of every type named in `item`'s fields and variants."""
    out = []
    for line in code(item.body)[1:-1].split("\n"):
        if line.strip().startswith("#["):
            continue
        # A named field: its type is the rest of the line, every field of a one-line
        # struct variant included (`Jwt { name: String, claim: Claim }`).
        field = FIELD_NAME_RE.search(line)
        if field:
            out.extend(TYPE_RE.findall(line[field.end() :]))
        # A struct field's type wrapped onto its own line.
        elif item.kind == "struct":
            out.extend(TYPE_RE.findall(line))
        # A tuple variant: the types between its parentheses. A bare word is a variant.
        elif item.kind == "enum":
            for inner in re.findall(r"\b[A-Z]\w*\s*\(([^)]*)\)", line):
                out.extend(TYPE_RE.findall(inner))
    return out


def read_open_tables() -> tuple[dict[tuple[str, str], str], list[str]]:
    """`(type, file) -> reason`, from `OPEN_TABLES`; and its malformed lines."""
    entries: dict[tuple[str, str], str] = {}
    malformed: list[str] = []
    if not OPEN_TABLES.is_file():
        return entries, malformed
    for line in OPEN_TABLES.read_text().splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        fields = line.split(None, 2)
        if len(fields) != 3:
            malformed.append(line)
            continue
        entries[(fields[0], fields[1])] = fields[2]
    return entries, malformed


def toml_walk(pins: dict[str, tuple[str, str]]) -> tuple[int, set[str]]:
    """The walk from `TomlSchema`: its exit code, and the hand-written types it reached."""
    sources = toml_sources()
    defs = index_items(sources)
    uses = {path: USE_RE.findall(source) for path, source in sources}
    hand_written = index_hand_written(sources)
    open_tables, malformed = read_open_tables()
    roots = [item for item in defs.get(TOML_ROOT, []) if item.crate == "fraiseql-cli"] or defs.get(
        TOML_ROOT, []
    )
    if len(roots) != 1:
        print(f"ERROR: {TOML_ROOT} not found once — this gate is measuring nothing.")
        return 1, set()

    seen: set[tuple[str, str]] = set()
    violations: list[str] = [
        f"{line}  — malformed line in {OPEN_TABLES}: expected `<type> <file> <reason>`"
        for line in malformed
    ]
    ambiguous: list[str] = []
    unpinned: list[str] = []
    listed_open: set[tuple[str, str]] = set()
    reached_hand_written: set[str] = set()

    queue: collections.deque[Item] = collections.deque(roots)
    while queue:
        item = queue.popleft()
        key = (item.name, str(item.path))
        if key in seen:
            continue
        seen.add(key)

        def follow(prefix: str, name: str, item: Item = item) -> None:
            if name in LEAF_TYPES:
                return
            found = definitions_of(name, prefix, item, uses, defs)
            if len(found) > 1:
                ambiguous.append(
                    f"{name} (named in {item.path}): " + ", ".join(str(f.path) for f in found)
                )
            elif found:
                queue.append(found[0])

        # A type read through another (`#[serde(from = "X")]`) has the keys of `X`.
        read_through = FROM_RE.search(item.attrs)
        if read_through:
            follow("", read_through.group(1))
            continue

        if not has_keys(item):
            # A string or a newtype: no key to be unknown. Its own values are its parser's.
            pass
        elif item.name in hand_written and hand_written[item.name] == item.path:
            reached_hand_written.add(item.name)
            pin = pins.get(item.name)
            if pin is None:
                unpinned.append(
                    f"{item.name:30s} {item.path}  — its `Deserialize` is written by hand and "
                    "no test is listed for it"
                )
            elif problem := pin_problem(*pin):
                unpinned.append(f"{item.name:30s} {item.path}  — {problem}")
        elif "Deserialize" in item.attrs:
            denies = "deny_unknown_fields" in item.attrs
            flattens = bool(SERDE_FLATTEN_RE.search(code(item.body)))
            if key in open_tables:
                listed_open.add(key)
                if denies:
                    violations.append(
                        f"{item.name:30s} {item.path}  — listed open in {OPEN_TABLES}, but it "
                        "denies unknown keys: remove the entry"
                    )
            elif not denies:
                why = (
                    "`#[serde(flatten)]` and no `deny_unknown_fields`: serde cannot deny "
                    "unknown keys through a flatten, so a key no table takes is dropped"
                    if flattens
                    else "no `deny_unknown_fields`: a key it does not take is dropped"
                )
                violations.append(f"{item.name:30s} {item.path}  — {why}")
            if denies and flattens:
                violations.append(
                    f"{item.name:30s} {item.path}  — `deny_unknown_fields` does not compose "
                    "with `#[serde(flatten)]`; serde silently drops the flattened keys"
                )

        for prefix, name in referenced_types(item):
            follow(prefix, name)

    for key in sorted(set(open_tables) - listed_open):
        violations.append(
            f"{key[0]:30s} {key[1]}  — listed open in {OPEN_TABLES}, but the walk from "
            f"{TOML_ROOT} reaches no table of that name in that file: remove the entry"
        )

    if ambiguous:
        print(f"ERROR: a type {TOML_ROOT} reaches names several definitions:")
        for line in sorted(set(ambiguous)):
            print(f"  {line}")
        print()
        print("Write the type with its crate path, or `use` it, so it names one definition.")
        return 1, reached_hand_written

    if violations:
        print(f"ERROR: a table {TOML_ROOT} reaches accepts a key it does not take:")
        for line in violations:
            print(f"  {line}")
        print()
        print("serde does not propagate `deny_unknown_fields` into nested types, and cannot")
        print("deny through a `#[serde(flatten)]`: such a table compiles a mistyped key and")
        print("drops it. Add `#[serde(deny_unknown_fields)]` (for a flatten, read the type")
        print("through a closed one, `#[serde(from = ...)]`), or, for a table whose keys the")
        print(f"author chooses, list it in {OPEN_TABLES} as `<type> <file> <reason>`.")
        return 1, reached_hand_written

    if unpinned:
        print(f"ERROR: a hand-written `Deserialize` reachable from {TOML_ROOT} is not pinned:")
        for line in unpinned:
            print(f"  {line}")
        print()
        print(f"List each one in {HAND_WRITTEN} as `<struct> <test file> <test fn>`.")
        return 1, reached_hand_written

    if len(seen) != TOML_EXPECTED_REACHABLE:
        print(
            f"ERROR: the walk from {TOML_ROOT} reached {len(seen)} types, expected "
            f"{TOML_EXPECTED_REACHABLE}."
        )
        print()
        print("More means a new table joined — check it and bump TOML_EXPECTED_REACHABLE.")
        print("FEWER means this gate is now measuring less than it was.")
        return 1, reached_hand_written

    print(
        f"OK: {len(seen)} types reachable from {TOML_ROOT}; every table refuses unknown "
        f"keys ({len(listed_open)} open, listed with the reason)"
    )
    return 0, reached_hand_written


def main() -> int:
    only = sys.argv[1] if len(sys.argv) > 1 else ""
    if only not in ("", "--only=server", "--only=toml"):
        print(f"usage: {sys.argv[0]} [--only=server|--only=toml]")
        return 2
    pins, malformed = read_pins()
    if malformed:
        print(f"ERROR: malformed lines in {HAND_WRITTEN}:")
        for line in malformed:
            print(f"  {line}  — expected `<struct> <test file> <test fn>`")
        return 1

    rc, reached = 0, set()
    if only != "--only=toml":
        walk_rc, walk_reached = server_walk(pins)
        rc, reached = max(rc, walk_rc), reached | walk_reached
    if only != "--only=server":
        walk_rc, walk_reached = toml_walk(pins)
        rc, reached = max(rc, walk_rc), reached | walk_reached
    if rc:
        return rc

    stale = sorted(set(pins) - reached)
    if stale:
        print("ERROR: a listed hand-written `Deserialize` is not reached as one:")
        for name in stale:
            print(
                f"  {name:30s} {HAND_WRITTEN}  — listed, but the walk reaches no hand-written "
                f"`Deserialize` of that name: remove the entry"
            )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
