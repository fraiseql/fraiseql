#!/usr/bin/env python3
"""Pin where a write's **selection set** comes from (#1331, #1352).

WHY THIS GATE EXISTS
--------------------
A mutation's selection set is not a formatting detail of the response. It is the
input to two security decisions at once:

  * `project_entity` filters the returned entity to it — and an **empty** slice
    means "no field filtering", so the whole stored entity is returned;
  * `selection_set_selects_gated_field` decides whether the #423 field authorizer
    runs at all — and it is false for an empty slice, so the authorizer takes
    **zero calls**.

So `&[]` is the *permissive* shape, not the neutral one. REST's anonymous write arm
passed it, and an unauthenticated caller was served policy-gated fields that an
authenticated caller is refused (#1352). One transport along, the gRPC arm derived
its own selection set and ended it in `unwrap_or_default()` — the same empty slice,
reached by a different spelling, for an unknown mutation or an all-object return
type. Neither is visible in a response: a transport that skips field authorization
answers every request reading no gated field exactly as one that runs it.

The other half is how the set is *carried*. `execute_mutation_with_security` used to
reach the engine by `format!`-ing a GraphQL document out of its arguments, and
`format!("{k}: {v}")` renders a `serde_json::Value` through `Display`, which emits
JSON — and JSON quotes object keys where GraphQL does not. Every nested body failed
to parse, so an authenticated REST write was refused a body the anonymous arm
accepted (#1331). Under the JSONB `data`-column model a nested object is the
ordinary body shape.

Both defects are one sentence: **a write path that has no selection set of its own
invented one.** The fix is that it asks `mutation_return_selections` instead, which
is scalars-only and never empty. This gate is what keeps a third transport from
inventing a fourth answer.

THREE RULES
-----------
1. No production call to an engine write entry passes an **empty selection set** in
   the selections position (matched positionally, so a legitimately-empty
   `inline_arguments` in the next slot is not confused with it).
2. A production file **outside the executor module** that calls one of those entries
   must obtain its selections from `mutation_return_selections`.
3. No production code in the engine runtime or in a transport route builds a GraphQL
   **operation document by string formatting**. Keyed on the operation keyword, so the
   anonymous-query shorthand (`{ field }`) is out of reach — deliberately: a write cannot
   use it, and `format!("{{ {x} }}")` is too common a shape to key on without false
   positives.

WHAT THIS GATE DELIBERATELY DOES NOT REACH
------------------------------------------
* `fraiseql-federation` is a GraphQL *client*: `HttpMutationClient::execute_mutation`
  must emit a document as text to a remote subgraph, and its own
  `execute_local_mutation` builds SQL and calls `execute_raw_query` without entering
  the engine at all. That path is outside every rule here, and outside
  check-mutation-dispatch-sites.sh's pattern too. Stated so a reader does not read
  silence as coverage.
* `fraiseql-cli`'s `doctor` builds argument-less probe documents out of schema-declared
  names, under `dry_run_mutations: true`. No caller value is formatted into it, so
  #1331's defect cannot occur there; it is outside rule 3's scope by construction
  rather than by allowlist, so there is no entry to go stale.

Mirrors tools/check-principal-producers.sh (#1336) and
tools/check-mutation-dispatch-sites.sh (#1327), including their staleness checks.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

# --------------------------------------------------------------------------------------
# The engine's write entries, and where the selection set sits in each argument list.
#
# Positional, not "any `&[]` in the call": `execute_mutation_detailed`'s LAST argument is
# `inline_arguments`, which is empty on every ordinary call. A gate that flagged an empty
# slice anywhere in the call would fire on `(…, &selections, &[])` — the correct code.
#
# `form` disambiguates namesakes. The engine entries are called as methods on an
# `Executor`; `execute_mutation_impl` is the chokepoint itself and is called as a path.
# `arity` is the second half of that disambiguation — see NAMESAKES.
#
#   name -> (form, arity, index of `selections`)
ENTRIES: dict[str, tuple[str, int, int]] = {
    "execute_mutation": ("method", 3, 2),
    "execute_mutation_as": ("method", 4, 3),
    "execute_mutation_detailed": ("method", 6, 4),
    "execute_mutation_impl": ("free", 7, 5),
}

# The one helper a transport with no selection set of its own may use.
PROVENANCE = "mutation_return_selections"

# Where the engine's own write entries live. Inside this directory a call may legitimately
# pass selections through from its own caller; outside it, rule 2 demands the helper.
ENGINE_DIR = "crates/fraiseql-core/src/runtime/executor/"

# Rule 3's scope: the engine runtime and the transports' route handlers — the request
# paths. A document built by formatting anywhere in here is #1331's shape.
DOCUMENT_SCOPE = (
    "crates/fraiseql-core/src/runtime/",
    "crates/fraiseql-server/src/routes/",
)

# Methods and macros that build a string, paired with a literal that opens a GraphQL
# operation. `format!` doubles its braces, so both `{` and `{{` must match.
DOCUMENT_BUILD = re.compile(
    r"""(?x)
    (?:format!|write!|writeln!|push_str|concat!|\+)             # a string is being built
    \s* \(? \s*
    r?\#? " \s*
    (?:mutation|query|subscription) \b                          # …opening an operation
    \s* (?:[A-Za-z_]\w*)?                                       # an optional operation name
    \s* (?: \( [^")]* \) )?                                    # optional variable definitions
    \s* \{                                                     # and its selection set
    """
)

# Every spelling of "no selections" that has appeared or could appear in the slot.
# `unwrap_or_default()` is here because that is how gRPC spelled it: the call site read
# `&selections` and the emptiness was one function away, so only an inline form is caught
# here — rule 2 is what reaches the other shape.
EMPTY_SELECTIONS = re.compile(
    r"""(?x)
    ^ (?:
          & ? \[ \s* \] (?: \s* \[ \s* \.\. \s* \] )?      # &[]  /  &[][..]
        | & ? vec! \s* \[ \s* \]                           # &vec![]
        | & ? Vec \s* (?: :: \s* < [^>]* > \s* )? :: \s* new \s* \( \s* \)   # &Vec::new()
        | & ? Default \s* :: \s* default \s* \( \s* \)     # &Default::default()
        | .* \. \s* unwrap_or_default \s* \( \s* \)        # …unwrap_or_default()
        | .* \. \s* unwrap_or \s* \( \s* & ? \[ \s* \] \s* \)
    ) $
    """
)

# Calls that share a name with an engine entry and are a different function.
#
# `HttpMutationClient::execute_mutation` posts a mutation to a REMOTE subgraph over HTTP.
# It has no local selection set, reaches no `Executor`, and takes six arguments where the
# engine entry takes three — which is what tells the two apart here.
#
# ⚠ Keyed by (file, name, arity), not by file alone: listing a file wholesale would excuse
# a real engine call added to it later. A name-keyed sweep that meets a namesake must
# report the ambiguity, never guess which one it found.
NAMESAKES: dict[tuple[str, str, int], str] = {
    (
        "crates/fraiseql-federation/src/saga_compensator.rs",
        "execute_mutation",
        6,
    ): "HttpMutationClient::execute_mutation — a remote subgraph call over HTTP",
    (
        "crates/fraiseql-federation/src/saga_executor/step.rs",
        "execute_mutation",
        6,
    ): "HttpMutationClient::execute_mutation — a remote subgraph call over HTTP",
}


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def blank_comments(src: str) -> str:
    """Replace every comment with spaces, preserving length so line numbers stay exact.

    A gate that counted prose would go red on the doc comment that explains it — which is
    how lint-async-trait once did, and this file's own header quotes both `format!("{k}:
    {v}")` and the operation literals rule 3 greps for.

    String-aware, because `"// not a comment"` is not one, and `"\\""` does not end its
    literal. Raw strings (`r#"…"#`) are handled to their matching hash count.
    """
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        ch = src[i]
        if ch == '"' or (ch == "r" and src[i : i + 2] in ('r"', "r#")):
            if ch == "r":
                j = i + 1
                hashes = 0
                while j < n and src[j] == "#":
                    hashes += 1
                    j += 1
                if j >= n or src[j] != '"':
                    i += 1
                    continue
                close = '"' + "#" * hashes
                end = src.find(close, j + 1)
                i = n if end == -1 else end + len(close)
                continue
            i += 1
            while i < n:
                if src[i] == "\\":
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        if ch == "'":
            # A lifetime (`'a`) is not a char literal; a char literal closes within 4 chars.
            end = src.find("'", i + 1)
            if end != -1 and end - i <= 4:
                i = end + 1
            else:
                i += 1
            continue
        if src[i : i + 2] == "//":
            while i < n and src[i] != "\n":
                out[i] = " "
                i += 1
            continue
        if src[i : i + 2] == "/*":
            depth, start = 1, i
            i += 2
            while i < n and depth:
                if src[i : i + 2] == "/*":
                    depth += 1
                    i += 2
                elif src[i : i + 2] == "*/":
                    depth -= 1
                    i += 2
                else:
                    i += 1
            for k in range(start, min(i, n)):
                if out[k] != "\n":
                    out[k] = " "
            continue
        i += 1
    return "".join(out)


CFG_TEST_MOD = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]")


def blank_inline_tests(code: str) -> str:
    """Blank every `#[cfg(test)] mod … { … }` body, preserving length.

    Excluding test FILES is not enough: `runtime/executor/security.rs` carries two inline
    `#[cfg(test)]` modules, and one of them passes `&[]` to `execute_mutation_as` on
    purpose. Counting those as production made this gate's first run a false positive.
    """
    out = list(code)
    for m in CFG_TEST_MOD.finditer(code):
        open_brace = code.find("{", m.end())
        if open_brace == -1:
            continue
        # Only a `mod … {` body is a test module; `#[cfg(test)] use …;` has no brace before
        # the next `;`, and `#[cfg(test)] mod tests;` names a file excluded by name.
        between = code[m.end() : open_brace]
        if ";" in between or "mod" not in between:
            continue
        depth, i, n = 0, open_brace, len(code)
        while i < n:
            if code[i] == "{":
                depth += 1
            elif code[i] == "}":
                depth -= 1
                if depth == 0:
                    i += 1
                    break
            i += 1
        for k in range(m.start(), min(i, n)):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


def split_args(code: str, open_paren: int) -> tuple[list[str], int] | None:
    """Top-level arguments of the call whose `(` is at `open_paren`, and its closing index.

    Returns None if the parentheses never balance — reported as a gate failure rather than
    skipped, because a silently-skipped call site is a hole shaped exactly like the defect.
    """
    depth = 0
    args: list[str] = []
    current: list[str] = []
    i, n = open_paren, len(code)
    while i < n:
        ch = code[i]
        if ch in "([{":
            depth += 1
            if depth == 1 and ch == "(":
                i += 1
                continue
        elif ch in ")]}":
            depth -= 1
            if depth == 0:
                arg = "".join(current).strip()
                if arg or args:
                    args.append(arg)
                # A multi-line Rust call ends `…,\n)`. The text after that last comma is
                # not an argument, and counting it made every engine entry look like a
                # namesake with one argument too many.
                if len(args) > 1 and args[-1] == "":
                    args.pop()
                return args, i
        elif ch == "," and depth == 1:
            args.append("".join(current).strip())
            i += 1
            current = []
            continue
        current.append(ch)
        i += 1
    return None


def line_of(code: str, index: int) -> int:
    return code.count("\n", 0, index) + 1


def production_files(root: Path) -> list[Path]:
    files = []
    for crate_src in sorted(root.glob("crates/*/src")):
        for path in sorted(crate_src.rglob("*.rs")):
            rel = path.relative_to(root).as_posix()
            if "/tests/" in rel:
                continue
            if path.name == "tests.rs" or path.name.endswith("_tests.rs"):
                continue
            if path.name == "test_support.rs":
                continue
            files.append(path)
    return files


class Failure(Exception):
    pass


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=None,
        help="tree to check; defaults to the git toplevel. Used by the self-test in "
        "tools/tests/write_selection_sources_test.sh to prove each rule goes red.",
    )
    args = parser.parse_args()
    root = args.root if args.root is not None else repo_root()

    empty_slot: list[str] = []          # rule 1
    no_provenance: list[str] = []       # rule 2
    built_documents: list[str] = []     # rule 3
    unlisted_namesakes: list[str] = []
    unbalanced: list[str] = []

    checked_calls = 0
    external_callers: set[str] = set()
    namesakes_seen: set[tuple[str, str, int]] = set()
    document_scope_files: dict[str, int] = dict.fromkeys(DOCUMENT_SCOPE, 0)

    for path in production_files(root):
        rel = path.relative_to(root).as_posix()
        code = blank_inline_tests(blank_comments(path.read_text(encoding="utf-8")))

        # ── rules 1 & 2 ────────────────────────────────────────────────────────────────
        for name, (form, arity, sel_index) in ENTRIES.items():
            prefix = r"\.\s*" if form == "method" else r"(?<![\w.])"
            for m in re.finditer(prefix + re.escape(name) + r"\s*\(", code):
                open_paren = code.index("(", m.end() - 1)
                split = split_args(code, open_paren)
                if split is None:
                    unbalanced.append(f"  {rel}:{line_of(code, m.start())}  {name}(…")
                    continue
                call_args, _ = split
                line = line_of(code, m.start())

                if len(call_args) != arity:
                    key = (rel, name, len(call_args))
                    namesakes_seen.add(key)
                    if key not in NAMESAKES:
                        unlisted_namesakes.append(
                            f"  {rel}:{line}  {name}/{len(call_args)} "
                            f"(the engine entry takes {arity})"
                        )
                    continue

                checked_calls += 1
                if not rel.startswith(ENGINE_DIR):
                    external_callers.add(rel)

                slot = " ".join(call_args[sel_index].split())
                if EMPTY_SELECTIONS.match(slot):
                    empty_slot.append(f"  {rel}:{line}  {name}(…, {slot}, …)")

        if not rel.startswith(ENGINE_DIR) and rel in external_callers:
            if not re.search(r"\b" + PROVENANCE + r"\b", code):
                no_provenance.append(f"  {rel}")

        # ── rule 3 ─────────────────────────────────────────────────────────────────────
        in_scope = [prefix for prefix in DOCUMENT_SCOPE if rel.startswith(prefix)]
        if in_scope:
            for prefix in in_scope:
                document_scope_files[prefix] += 1
            for m in DOCUMENT_BUILD.finditer(code):
                snippet = " ".join(code[m.start() : m.start() + 60].split())
                built_documents.append(f"  {rel}:{line_of(code, m.start())}  {snippet}…")

    failed = False

    if unbalanced:
        print(
            "ERROR: could not read the argument list of a write-entry call:", file=sys.stderr
        )
        print("\n".join(unbalanced), file=sys.stderr)
        print(
            "\nA call site this gate cannot parse is a call site it does not check.\n"
            "Fix the parser in tools/check-write-selection-sources.py rather than\n"
            "letting it skip.",
            file=sys.stderr,
        )
        failed = True

    if unlisted_namesakes:
        print(
            "ERROR: a call names an engine write entry with an argument count it does "
            "not take:",
            file=sys.stderr,
        )
        print("\n".join(unlisted_namesakes), file=sys.stderr)
        print(
            "\nEither the entry's signature changed — update ENTRIES, and re-derive which\n"
            "argument is `selections` — or this is a different function that happens to\n"
            "share the name, in which case add it to NAMESAKES with the reason.\n"
            "Guessing which one it is, is how a name-keyed sweep reports the wrong verdict.",
            file=sys.stderr,
        )
        failed = True

    if empty_slot:
        print("ERROR: a write dispatches with an EMPTY selection set (#1352):", file=sys.stderr)
        print("\n".join(empty_slot), file=sys.stderr)
        print(
            "\nAn empty selection set is the PERMISSIVE shape, not the neutral one:\n"
            "  project_entity returns the whole entity unfiltered, and\n"
            "  selection_set_selects_gated_field is false, so the #423 field authorizer\n"
            "  short-circuits with zero calls.\n"
            "That is how an unauthenticated REST write was served policy-gated fields an\n"
            "authenticated one is refused.\n\n"
            f"Derive the set instead:\n"
            f"  let selections = fraiseql_core::runtime::{PROVENANCE}(schema, mutation_name);\n"
            "It is scalars-only and never empty, by construction and by its own test.",
            file=sys.stderr,
        )
        failed = True

    if no_provenance:
        print(
            "ERROR: a transport calls an engine write entry and does not derive its "
            "selection set from the one helper (#1331/#1352):",
            file=sys.stderr,
        )
        print("\n".join(no_provenance), file=sys.stderr)
        print(
            f"\nCall fraiseql_core::runtime::{PROVENANCE}(schema, mutation_name).\n"
            "The gRPC arm had its own copy of that derivation and ended it in\n"
            "`unwrap_or_default()` — an empty set for an unknown mutation or an\n"
            "all-object return type, which is the permissive shape one transport along.\n"
            "A second implementation of this rule is the defect, not the spelling.",
            file=sys.stderr,
        )
        failed = True

    if built_documents:
        print(
            "ERROR: a GraphQL operation document is built by string formatting on a "
            "request path (#1331):",
            file=sys.stderr,
        )
        print("\n".join(built_documents), file=sys.stderr)
        print(
            "\n`format!(\"{k}: {v}\")` renders a serde_json::Value through Display, which\n"
            "emits JSON — and JSON quotes object keys where GraphQL does not. Every body\n"
            "carrying a nested object produced a document the parser refused, so an\n"
            "authenticated REST write was refused a body the anonymous arm accepted.\n\n"
            "Bind the arguments as VALUES through the executor instead:\n"
            "  Executor::execute_mutation_with_security  (structured args + a principal)\n"
            "  Executor::execute_mutation_as             (…plus your own selection set)",
            file=sys.stderr,
        )
        failed = True

    if failed:
        return 1

    # ── the gate's own non-vacuity ──────────────────────────────────────────────────────
    # Each of these is a way for every rule above to keep passing while checking nothing.
    try:
        try:
            entry_defs = (
                root / "crates/fraiseql-core/src/runtime/executor/mutation.rs"
            ).read_text(encoding="utf-8")
            runner_defs = (
                root / "crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs"
            ).read_text(encoding="utf-8")
        except FileNotFoundError as missing:
            raise Failure(
                f"{missing.filename} does not exist.\n"
                "The engine's write entries moved; this gate cannot confirm it is still\n"
                "looking at them, so it refuses rather than passing over a tree it does\n"
                "not recognise."
            ) from missing
        for name in ENTRIES:
            if not re.search(r"\bfn\s+" + re.escape(name) + r"\b", entry_defs + runner_defs):
                raise Failure(
                    f"the write entry `{name}` no longer exists.\n"
                    "Either it was renamed — update ENTRIES, and re-derive which argument\n"
                    "is `selections` — or this gate is now blind to that path."
                )
        if not re.search(r"\bfn\s+" + PROVENANCE + r"\b", entry_defs):
            raise Failure(
                f"the selection-set helper `{PROVENANCE}` no longer exists.\n"
                "Rule 2 accepts a transport only because that helper is where the rule\n"
                "lives; without it the rule has no subject."
            )
        if checked_calls == 0:
            raise Failure(
                "no write-entry call site was examined.\n"
                "The call pattern matched nothing, so rules 1 and 2 passed over an empty set."
            )
        if not external_callers:
            raise Failure(
                "no production file outside the executor module calls a write entry.\n"
                "Rule 2 is pinning nothing: either the last transport caller went away\n"
                "(and this check should be re-derived) or the sweep went blind."
            )
        # Per prefix, not in total: `crates/fraiseql-core/src/runtime/` necessarily has
        # files (the entries live under it), so a combined count would stay non-zero while
        # `crates/fraiseql-server/src/routes/` — every transport in the tree — had moved
        # out from under rule 3 unnoticed.
        for prefix, seen in document_scope_files.items():
            if seen == 0:
                raise Failure(
                    f"rule 3 scanned no files under {prefix}.\n"
                    "That scope moved or was renamed, and rule 3 is vacuous over it."
                )
        for key, why in NAMESAKES.items():
            rel, name, arity = key
            if not (root / rel).exists():
                raise Failure(f"NAMESAKES entry {rel} does not exist — remove it.")
            if key not in namesakes_seen:
                raise Failure(
                    f"NAMESAKES entry {rel} no longer calls `{name}` with {arity} "
                    f"arguments ({why}).\n"
                    "Remove it so the file is gated again: an entry that excuses nothing\n"
                    "would excuse a REAL engine call added to that same file."
                )
    except Failure as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    print(
        f"OK: {checked_calls} write-entry call site(s) carry a derived selection set "
        f"({len(external_callers)} outside the engine), and no request path builds a "
        f"GraphQL document by formatting ({sum(document_scope_files.values())} files "
        "in scope)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
