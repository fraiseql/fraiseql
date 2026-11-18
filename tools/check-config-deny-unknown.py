#!/usr/bin/env python3
"""Every config struct reachable from `ServerConfig` refuses unknown keys (#1337).

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
"""

from __future__ import annotations

import collections
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
EXPECTED_REACHABLE = int(os.environ.get("CONFIG_DENY_UNKNOWN_EXPECTED", "53"))
# +1: [function_dispatch] (#1628) leases and sweeps the function dispatch ledger.
# +1 (#2033): `[rate_limiting]` is `ServerRateLimitingConfig` over the compiled
# `RateLimitingSecurityConfig`, replacing `RateLimitConfig`. Its `Deserialize` is written by
# hand, and listed with its pinning test in `HAND_WRITTEN`.

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


def main() -> int:
    sources = runtime_sources()
    defs = index_structs(sources)
    hand_written = index_hand_written(sources)
    pins, malformed = read_pins()
    if ROOT_STRUCT not in defs:
        print(f"ERROR: {ROOT_STRUCT} not found — this gate is measuring nothing.")
        return 1

    seen: set[str] = set()
    violations: list[str] = []
    ambiguous: list[str] = []
    exempt: list[str] = []
    unpinned: list[str] = [
        f"{line}  — malformed line in {HAND_WRITTEN}: expected `<struct> <test file> <test fn>`"
        for line in malformed
    ]

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
        return 1

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
        return 1

    for name in sorted(pins):
        if name not in seen or name not in hand_written:
            unpinned.append(
                f"{name:30s} {HAND_WRITTEN}  — listed, but the walk reaches no hand-written "
                f"`Deserialize` of that name: remove the entry"
            )

    if unpinned:
        print("ERROR: a hand-written `Deserialize` reachable from ServerConfig is not pinned:")
        for line in unpinned:
            print(f"  {line}")
        print()
        print("This gate cannot read what a hand-written impl does with an unknown key. List")
        print(f"each one in {HAND_WRITTEN} as `<struct> <test file> <test fn>`, naming a")
        print("test that feeds it an unknown key and requires the refusal.")
        return 1

    if len(seen) != EXPECTED_REACHABLE:
        print(
            f"ERROR: the walk reached {len(seen)} structs, expected {EXPECTED_REACHABLE}."
        )
        print()
        print("More means a new section joined — check it and bump EXPECTED_REACHABLE.")
        print("FEWER means this gate is now measuring less than it was, which looks")
        print("identical to a clean tree. That is the failure this count exists to catch.")
        return 1

    print(
        f"OK: {len(seen)} config structs reachable from {ROOT_STRUCT}; "
        f"every deserialized one refuses unknown keys "
        f"({len(exempt)} exempt with a stated reason, {len(pins)} written by hand and "
        f"pinned by a test)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
