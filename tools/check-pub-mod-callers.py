#!/usr/bin/env python3
"""The caller gate: every `pub mod` of a library is used by a binary, or is allowlisted.

`tools/check-module-reachability.py` proves every file is compiled. Compiled is not
used: a `pub` module in a library crate is exempt from rustc's `dead_code` lint,
because an embedder might call it, so a module whose last production caller is gone
stays green forever, kept alive by its own unit tests (#1640). REVIEW-2 found about
30k such lines: a second GraphQL parser, two Redis job queues, an OAuth client, a
duplicate RBAC layer.

The rule
--------
Every module on a library's public surface (declared `pub mod`, or re-exported from
with `pub use`, all the way up to the crate root) must be *used*
from a binary target (`src/main.rs`, `src/bin/*`, `[[bin]] path`), directly or through
other used modules, counting only non-test code. A module that may stay without a
binary caller has a line in `tools/pub-mod-callers-allowlist.txt`:

* `test-support` or `api`: some test, example or doctest must still use it, since a
  test-support module no test uses is dead too;
* `pending`: the reason cites the issue that wires or deletes it (`#N`).

An entry for a module a binary uses, or for one that no longer exists, fails, so
the commit that wires or deletes a module also drops its line.

"Used" is a reference graph, built the way rustc resolves names:

* Nodes are modules. Every target (lib, bins, tests, benches, examples) is walked
  from its root through `mod` declarations, inline `mod x { … }` and `#[path]`.
* Edges are the paths a module writes in its code (`a::b::Item`, `crate::x`,
  `super::y`, `fraiseql_core::z`) and its non-`pub` `use` trees. Each path is resolved
  segment by segment through child modules, items, `use` bindings, globs and the
  crate names a manifest declares. The module that owns the item a path lands on is
  *used*, and so is a module a path names outright (rustc rejects an unused import);
  the modules a path only passes through are *traversed*.
* Every definition `#[cfg]` could select counts: a feature-off stub beside the real
  re-export does not hide the re-export.
* A `pub use` re-export is not an edge. It is followed only when a path resolves
  through it, so a crate root re-exporting a dead module does not keep it alive.
* `impl` blocks are reached by method calls, which never name the module they live
  in. A module is used if it implements a used workspace trait for a used type, an
  external trait for a used type, or adds methods to a used type that some used
  module calls by name. An impl of a dead trait keeps nothing alive.
* `macro_rules!` invocations are edges to the module defining the macro. A module
  with an item-position macro call (`wasmtime::component::bindgen!`) may define
  names no text shows, so a lookup in it that finds nothing lands on it.
* Code under `#[cfg(test)]`, `#[test]` and `#[…::test]`, and the Rust blocks of doc
  comments, are test code: they count for the allowlist's test-use check only.

The unit is the module, so the gate is a lower bound on dead code: a module one
live function references stays green even if the rest of it is dead. Against the
symbols of the `--all-features` debug binaries it reported no module a binary
contains. `--why <module>` prints the chain of references that makes a module used.

Like the first gate it reads the filesystem, never git, because the preflight
container has no index.
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ROOT = Path(os.environ.get("PUB_MOD_CALLERS_ROOT", REPO))
ALLOWLIST = Path(os.environ.get("PUB_MOD_CALLERS_ALLOWLIST", REPO / "tools" / "pub-mod-callers-allowlist.txt"))

_spec = importlib.util.spec_from_file_location("reach", REPO / "tools" / "check-module-reachability.py")
reach = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(reach)

IDENT = r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*"
PATH_RE = re.compile(rf"(?<![\w$])(?:\$crate|{IDENT})(?:\s*::\s*{IDENT})+")
BARE_IDENT = re.compile(rf"(?<![\w$:])({IDENT})(?!\s*::)")
ITEM_MACRO = re.compile(rf"(?<![\w$])(?:{IDENT}\s*::\s*)*({IDENT})\s*!\s*[(\[{{]")
MOD_NAME = re.compile(rf"(?<![\w$])mod\s+{IDENT}")
CALLED = re.compile(rf"(?:\.|::)\s*({IDENT})")
MACRO_CALL = re.compile(rf"(?<![\w:$])({IDENT})\s*!\s*[(\[{{]")
MACRO_DEF = re.compile(rf"\bmacro_rules\s*!\s*({IDENT})")
DEF_RE = re.compile(
    rf"(?<![\w$])(?:(pub)(?:\s*\([^)]*\))?\s+)?"
    rf"(?:(?:const|async|unsafe|default|extern\s*\"[^\"]*\"|extern)\s+)*"
    rf"(fn|struct|enum|union|trait|type|const|static|mod)\s+({IDENT})"
)
USE_RE = re.compile(r"(?<![\w$])(?:(pub)(\s*\([^)]*\))?\s+)?use\s+")
IMPL_RE = re.compile(r"(?<![\w$])impl\b")
ATTR_STRING_PATH = re.compile(r'\b(?:with|serialize_with|deserialize_with|default|crate)\s*=\s*"((?:\w+::)+\w+)"')
TEST_ATTR = re.compile(r"^(?:test|\w+(?:::\w+)*::test|rstest|test_case\b.*|bench|cfg\s*\(\s*(?:test|all\s*\((?:(?!not\s*\().)*\btest\b.*\))\s*\))$", re.DOTALL)


def clean(name: str) -> str:
    return name.removeprefix("r#")


@dataclass
class Module:
    key: tuple
    file: Path
    line: int
    lines: int
    declared_pub: bool
    is_test: bool
    parent: tuple | None
    children: dict[str, tuple] = field(default_factory=dict)
    items: set[str] = field(default_factory=set)
    macros: set[str] = field(default_factory=set)
    bindings: dict[str, list[tuple[list[str], bool]]] = field(default_factory=dict)
    globs: list[tuple[list[str], bool]] = field(default_factory=list)
    prod_refs: list[list[str]] = field(default_factory=list)
    test_refs: list[list[str]] = field(default_factory=list)
    prod_macros: set[str] = field(default_factory=set)
    test_macros: set[str] = field(default_factory=set)
    impls: list[tuple[list[str], list[str] | None, set[str]]] = field(default_factory=list)
    called: set[str] = field(default_factory=set)
    item_macros: bool = False  # an item-position macro call may define any name


@dataclass
class Target:
    tid: str
    kind: str  # lib | bin | test
    ident: str
    root: Path
    externs: dict[str, str]  # identifier in code -> lib ident of a workspace crate


# ── text passes ─────────────────────────────────────────────────────────────────────


def matching(text: str, i: int, open_c: str, close_c: str) -> int:
    """Index just past the bracket matching the one at `i`."""
    depth, n = 0, len(text)
    while i < n:
        if text[i] == open_c:
            depth += 1
        elif text[i] == close_c:
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def item_end(text: str, i: int) -> int:
    """End of the item starting at `i`: its `;` or the brace closing its body."""
    depth, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif depth == 0 and c == ";":
            return i + 1
        elif depth == 0 and c == "{":
            return matching(text, i, "{", "}")
        i += 1
    return n


def test_spans(text: str) -> list[tuple[int, int]]:
    """The spans of every item marked as test-only."""
    spans: list[tuple[int, int]] = []
    inner = re.match(r"\s*(?:#!\s*\[[^\]]*\]\s*)*?#!\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", text)
    if inner:
        return [(0, len(text))]
    i, n = 0, len(text)
    while i < n:
        if text.startswith("#[", i) or (text[i] == "#" and re.match(r"#\s+\[", text[i:])):
            start = i
            j = text.index("[", i)
            end = matching(text, j, "[", "]")
            content = text[j + 1 : end - 1].strip()
            if TEST_ATTR.match(content):
                k = end
                while True:  # skip the attributes that follow
                    m = re.match(r"\s*#\s*\[", text[k:])
                    if not m:
                        break
                    k = matching(text, k + m.end() - 1, "[", "]")
                stop = item_end(text, k)
                spans.append((start, stop))
                i = stop
                continue
            i = end
        else:
            i += 1
    return spans


def ws(text: str) -> str:
    """`text` as blanks, newlines kept so line numbers survive."""
    if "\n" not in text:
        return " " * len(text)
    return "\n".join(" " * len(line) for line in text.split("\n"))


def splice(text: str, spans: list[tuple[int, int]], inside: bool) -> str:
    """Keep only the text inside `spans` (or only outside them), blanking the rest."""
    out, pos = [], 0
    for a, b in spans:
        out.append(ws(text[pos:a]) if inside else text[pos:a])
        out.append(text[a:b] if inside else ws(text[a:b]))
        pos = b
    out.append(ws(text[pos:]) if inside else text[pos:])
    return "".join(out)


def top_level(text: str) -> str:
    """Blank everything inside braces, leaving item headers at depth 0."""
    out, depth, pos = [], 0, 0
    for m in re.finditer(r"[{}]", text):
        piece = text[pos : m.start()]
        out.append(piece if depth == 0 else ws(piece))
        out.append(" ")
        depth += 1 if m.group() == "{" else -1
        pos = m.end()
    out.append(text[pos:] if depth == 0 else ws(text[pos:]))
    return "".join(out)


def parse_use_tree(text: str, prefix: list[str], out: list[tuple[list[str], str | None]]) -> None:
    """Expand a use tree into (path, binding) pairs; a binding of `*` is a glob."""
    text = text.strip()
    if not text:
        return
    if text.endswith("}") and "{" in text:
        brace = text.index("{")
        head = [clean(s.strip()) for s in text[:brace].split("::") if s.strip()]
        inner = text[brace + 1 : -1]
        depth, start = 0, 0
        for p, c in enumerate(inner):
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
            elif c == "," and depth == 0:
                parse_use_tree(inner[start:p], prefix + head, out)
                start = p + 1
        parse_use_tree(inner[start:], prefix + head, out)
        return
    alias = None
    m = re.match(rf"(.*?)\s+as\s+({IDENT})$", text, re.DOTALL)
    if m:
        text, alias = m.group(1), clean(m.group(2))
    segs = [clean(s.strip()) for s in text.split("::") if s.strip()]
    if segs == ["*"] or (segs and segs[-1] == "*"):
        out.append((prefix + segs[:-1], "*"))
        return
    if segs == ["self"]:
        path = prefix
        out.append((path, alias or (path[-1] if path else None)))
        return
    path = prefix + segs
    out.append((path, alias or (path[-1] if path else None)))


def is_declared_pub(text: str, mod_at: int) -> bool:
    """Whether the `mod` at `mod_at` is declared exactly `pub` (not `pub(crate)` and kin).

    Only the declaration's own text counts: back to the end of the previous item or
    attribute, so a `pub(crate) mod` on the line above cannot leak into this one.
    """
    start = max(text.rfind(c, 0, mod_at) for c in ";{}]") + 1
    return re.search(r"\bpub\b(?!\s*\()", text[start:mod_at]) is not None


def first_path(text: str) -> list[str] | None:
    text = re.sub(r"^\s*(?:&\s*(?:'\w+\s*)?(?:mut\s+)?|dyn\s+|unsafe\s+|!\s*)*", "", text)
    m = re.match(rf"(?:::)?\s*({IDENT}(?:\s*::\s*{IDENT})*)", text)
    if not m:
        return None
    return [clean(s.strip()) for s in m.group(1).split("::")]


def impl_headers(top: str, prod: str) -> list[tuple[list[str], list[str] | None, set[str]]]:
    """Every `impl` block: its self type, its trait (None for an inherent impl), its methods."""
    found = []
    for m in IMPL_RE.finditer(top):
        body = prod.find("{", m.end())
        methods: set[str] = set()
        if body != -1:
            inner = top_level(prod[body + 1 : matching(prod, body, "{", "}") - 1])
            methods = {clean(n) for n in re.findall(rf"\bfn\s+({IDENT})", inner)}
        rest = prod[m.end() : body if body != -1 else len(prod)]
        stripped = rest.lstrip()
        if stripped.startswith("<"):
            j = len(rest) - len(stripped)
            depth = 0
            for p in range(j, len(rest)):
                if rest[p] == "<":
                    depth += 1
                elif rest[p] == ">" and rest[p - 1] != "-":
                    depth -= 1
                    if depth == 0:
                        rest = rest[p + 1 :]
                        break
        header = re.split(r"\bwhere\b|\{|;", rest, maxsplit=1)[0]
        depth, cut = 0, None
        for p in range(len(header)):
            c = header[p]
            if c == "<":
                depth += 1
            elif c == ">" and header[p - 1] != "-":
                depth -= 1
            elif depth == 0 and re.match(r"\bfor\b", header[p:]) and (p == 0 or not header[p - 1].isalnum()):
                cut = p
                break
        if cut is None:
            self_ty, trait = first_path(header), None
        else:
            trait, self_ty = first_path(header[:cut]), first_path(header[cut + 3 :])
        if self_ty:
            found.append((self_ty, trait, methods))
    return found


DOC_LINE = re.compile(r"^\s*//[/!](?!/) ?(.*)$")
NOT_RUST = re.compile(r"\b(?:ignore|text|compile_fail|toml|json|sh|bash|console|sql|python|yaml|graphql|js|ts|javascript|typescript|http|ini|dockerfile)\b")


def doctests(raw: str) -> list[str]:
    """The Rust code blocks of every doc comment: rustdoc compiles them as tests."""
    blocks: list[str] = []
    current: list[str] | None = None
    for line in raw.splitlines():
        m = DOC_LINE.match(line)
        if not m:
            current = None
            continue
        text = m.group(1)
        fence = re.match(r"\s*(```+|~~~+)\s*(.*)$", text)
        if fence:
            if current is None:
                current = [] if not NOT_RUST.search(fence.group(2)) else None
                if current is not None:
                    blocks.append("")
                else:
                    current = ["skip"]
            else:
                if current != ["skip"]:
                    blocks[-1] = "\n".join(current)
                current = None
            continue
        if current is not None and current != ["skip"]:
            current.append(re.sub(r"^\s*# ?", "", text) if re.match(r"^\s*#(?:\s|$)", text) else text)
    return [b for b in blocks if b]


def strip_generics(path_text: str) -> list[str]:
    return [clean(s.strip()) for s in path_text.split("::")]


# ── the module graph ───────────────────────────────────────────────────────────────


class Graph:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.modules: dict[tuple, Module] = {}
        self.targets: dict[str, Target] = {}
        self.lib_root: dict[str, tuple] = {}  # lib ident -> root module key
        self.crate_macros: dict[str, dict[str, tuple]] = {}
        self.exported: dict[str, dict[str, tuple]] = {}  # #[macro_export] lands at the crate root
        self._memo: dict[tuple, tuple[list, list]] = {}
        self._dirs: list[Path] = []

    # building

    def load(self) -> None:
        workspace_deps = {}
        root_manifest = self.root / "Cargo.toml"
        if root_manifest.is_file():
            data = tomllib.loads(root_manifest.read_text())
            for k, v in data.get("workspace", {}).get("dependencies", {}).items():
                workspace_deps[k] = v.get("package", k) if isinstance(v, dict) else k
        crates = []
        for manifest in sorted(self.root.glob("crates/*/Cargo.toml")):
            data = tomllib.loads(manifest.read_text())
            pkg = data.get("package", {}).get("name")
            if pkg:
                crates.append((manifest.parent, data, pkg))
        lib_ident_of = {}
        for crate, data, pkg in crates:
            lib = data.get("lib", {})
            if lib.get("path") or (crate / "src/lib.rs").is_file():
                lib_ident_of[pkg] = lib.get("name", pkg).replace("-", "_")
        for crate, data, pkg in crates:
            externs = {}
            tables = [data.get("dependencies", {}), data.get("dev-dependencies", {})]
            for t in data.get("target", {}).values():
                tables += [t.get("dependencies", {}), t.get("dev-dependencies", {})]
            for table in tables:
                for key, val in table.items():
                    target_pkg = key
                    if isinstance(val, dict):
                        if val.get("workspace"):
                            target_pkg = workspace_deps.get(key, key)
                        target_pkg = val.get("package", target_pkg)
                    if target_pkg in lib_ident_of:
                        externs[key.replace("-", "_")] = lib_ident_of[target_pkg]
            own = lib_ident_of.get(pkg)
            lib = data.get("lib", {})
            if own:
                lib_path = crate / lib.get("path", "src/lib.rs")
                # Its doctests name the crate from outside, so its own name resolves too.
                self.add_target(Target(f"lib:{own}", "lib", own, lib_path.resolve(), dict(externs, **{own: own})))
            bins = {}
            if (crate / "src/main.rs").is_file():
                bins[pkg] = crate / "src/main.rs"
            for p in crate.glob("src/bin/*.rs"):
                bins[p.stem] = p
            for p in crate.glob("src/bin/*/main.rs"):
                bins[p.parent.name] = p
            for b in data.get("bin", []):
                if b.get("path"):
                    bins[b.get("name", pkg)] = crate / b["path"]
            with_own = dict(externs)
            if own:
                with_own[own] = own
            for name, path in sorted(bins.items()):
                self.add_target(Target(f"bin:{pkg}:{name}", "bin", name, path.resolve(), with_own))
            tests = []
            for pattern in ("tests/*.rs", "benches/*.rs", "examples/*.rs", "examples/*/main.rs", "tests/*/main.rs"):
                tests += crate.glob(pattern)
            for section in ("test", "bench", "example"):
                tests += [crate / t["path"] for t in data.get(section, []) if t.get("path")]
            for path in sorted({p.resolve() for p in tests}):
                if path.is_file():
                    rel = path.relative_to(crate.resolve())
                    self.add_target(Target(f"test:{pkg}:{rel}", "test", path.stem, path, with_own))

    def add_target(self, target: Target) -> None:
        self.targets[target.tid] = target
        key = (target.tid,)
        if target.kind == "lib":
            self.lib_root[target.ident] = key
        self.crate_macros.setdefault(target.tid, {})
        self.load_file(key, target.root, None, True, False, False, 1)

    def load_file(self, key: tuple, path: Path, parent: tuple | None, mod_rs: bool, declared_pub: bool, is_test: bool, line: int) -> None:
        raw = path.read_text(errors="replace")
        text = reach.strip(raw)
        spans = test_spans(text)
        mask = bytearray(len(text))
        for a, b in spans:
            mask[a:b] = b"\x01" * (b - a)
        views = (splice(text, spans, inside=False), splice(text, spans, inside=True))
        saved, self._dirs = self._dirs, []  # a file resolves its children from its own place
        self.load_region(key, path, raw, text, (mask, views), 0, len(text), parent, mod_rs, declared_pub, is_test, line, raw.count("\n") + 1)
        self._dirs = saved

    def load_region(self, key, path, raw, text, masks, start, end, parent, mod_rs, declared_pub, is_test, line, lines) -> None:
        mask, (prod_full, test_full) = masks
        mod = Module(key, path, line, lines, declared_pub, is_test, parent)
        self.modules[key] = mod
        if parent:
            self.modules[parent].children[key[-1]] = key
        file_dir = path.parent
        base = file_dir if mod_rs else file_dir / path.stem
        children: list[tuple[int, int]] = []
        # Inline child modules first: their bodies belong to them, not to us.
        i = start
        while i < end:
            m = reach.MOD_DECL.match(text, i) if text[i] == "m" and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")) else None
            if not m:
                i += 1
                continue
            name, term = clean(m.group(1)), m.group(2)
            pub = is_declared_pub(text, i)
            child_test = is_test or mask[i]
            child_key = key + (name,)
            attr = reach._path_attr_before(text, i)
            child_line = text.count("\n", 0, i) + 1
            if term == "{":
                body_end = matching(text, m.end() - 1, "{", "}")
                children.append((m.end() - 1, body_end))
                saved = self._push_dir(base, attr or name)
                self.load_region(child_key, path, raw, text, masks, m.end(), body_end - 1, key, True, pub, child_test,
                                 child_line, text.count("\n", m.end(), body_end) + 1)
                self._pop_dir(saved)
                i = body_end
                continue
            if attr:
                target = (self._path_dir(file_dir) / attr).resolve()
                if target.is_file():
                    self.load_file(child_key, target, key, True, pub, child_test, child_line)
            else:
                d = self._inline_dir(base)
                for cand in (d / f"{name}.rs", d / name / "mod.rs"):
                    if cand.is_file():
                        self.load_file(child_key, cand.resolve(), key, cand.name == "mod.rs", pub, child_test, child_line)
                        break
            i = m.end()
        def own(full: str) -> str:
            out, pos = [], start
            for a, b in children:
                out += [full[pos:a], ws(full[a:b])]
                pos = b
            out.append(full[pos:end])
            return "".join(out)

        prod = ws(own(text)) if is_test else own(prod_full)
        test = own(text) if is_test else own(test_full)
        self.collect(mod, prod, test, raw[start:end])

    # Inline modules nest their directory; a small stack keeps that without threading it.
    _dirs: list[Path]

    def _push_dir(self, base: Path, name: str):
        self._dirs.append((self._inline_dir(base)) / name)
        return len(self._dirs) - 1

    def _pop_dir(self, saved) -> None:
        del self._dirs[saved:]

    def _inline_dir(self, base: Path) -> Path:
        return self._dirs[-1] if self._dirs else base

    def _path_dir(self, file_dir: Path) -> Path:
        return self._dirs[-1] if self._dirs else file_dir

    def collect(self, mod: Module, prod: str, test: str, raw: str) -> None:
        top = top_level(prod)
        for m in DEF_RE.finditer(top):
            kind, name = m.group(2), clean(m.group(3))
            if kind != "mod":
                mod.items.add(name)
        for m in MACRO_DEF.finditer(prod):
            name = clean(m.group(1))
            mod.macros.add(name)
            tid = mod.key[0]
            self.crate_macros[tid].setdefault(name, mod.key)
            if re.search(r"#\s*\[\s*macro_export\s*\]\s*$", prod[max(0, m.start() - 60) : m.start()]):
                self.exported.setdefault(tid, {})[name] = mod.key
        prod_code = self.take_uses(mod, prod, record=True)
        test_code = self.take_uses(mod, test, record=False)
        for code, refs, macros in ((prod_code, mod.prod_refs, mod.prod_macros), (test_code, mod.test_refs, mod.test_macros)):
            code = MOD_NAME.sub(lambda m: ws(m.group()), code)  # declaring a module is not using it
            for m in PATH_RE.finditer(code):
                refs.append(strip_generics(m.group(0)))
            # A bare name reaches another module only through a glob or a re-export
            # (a private `use` is already an edge); resolve those names too.
            for name in set(BARE_IDENT.findall(PATH_RE.sub(" ", code))):
                name = clean(name)
                if mod.globs or any(reexport for _, reexport in mod.bindings.get(name, ())):
                    refs.append([name])
            for m in MACRO_CALL.finditer(code):
                if m.group(1) != "macro_rules":
                    macros.add(clean(m.group(1)))
        for code in doctests(raw):
            body = reach.strip(code)
            for m in USE_RE.finditer(body):
                pairs: list[tuple[list[str], str | None]] = []
                parse_use_tree(body[m.end() : item_end(body, m.end())].rstrip().rstrip(";").replace("\n", " "), [], pairs)
                mod.test_refs += [p for p, _ in pairs if p]
            mod.test_refs += [strip_generics(m.group(0)) for m in PATH_RE.finditer(body)]
        for m in ATTR_STRING_PATH.finditer(raw):
            mod.prod_refs.append(m.group(1).split("::"))
        mod.impls = impl_headers(top, prod)
        mod.item_macros = any(m.group(1) != "macro_rules" for m in ITEM_MACRO.finditer(top))
        mod.called = {clean(n) for n in CALLED.findall(prod_code)}

    def take_uses(self, mod: Module, text: str, record: bool) -> str:
        """Record `use` declarations and return the text with them blanked."""
        out = list(text)
        for m in USE_RE.finditer(text):
            end = item_end(text, m.end())
            body = text[m.end() : end].rstrip().rstrip(";")
            pairs: list[tuple[list[str], str | None]] = []
            parse_use_tree(body.replace("\n", " "), [], pairs)
            is_reexport = m.group(1) is not None and m.group(2) is None
            for path, binding in pairs:
                if path and path[0] == "":
                    path = path[1:]
                if not path:
                    continue
                if record or not is_reexport:
                    if binding == "*":
                        mod.globs.append((path, is_reexport and record))
                    elif binding:
                        mod.bindings.setdefault(binding, []).append((path, is_reexport and record))
                if not is_reexport:
                    (mod.prod_refs if record else mod.test_refs).append(path)
            for p in range(m.start(), end):
                out[p] = "\n" if text[p] == "\n" else " "
        return "".join(out)

    # resolution

    def target(self, key: tuple) -> Target:
        return self.targets[key[0]]

    def resolve(self, ctx: tuple, segs: list[str]) -> tuple[list[tuple[str, tuple]], list[tuple]]:
        """Resolve a path written in `ctx`.

        Returns the modules it touches, each tagged `owner` (an item the path names
        lives there) or `via` (the path passes through it), and the modules the whole
        path names when it names a module rather than an item.
        """
        segs = [s for s in segs if s]
        if not segs:
            return [], []
        memo_key = (ctx, tuple(segs))
        if memo_key in self._memo:
            return self._memo[memo_key]
        self._memo[memo_key] = ([], [])  # cycle guard
        touched: list[tuple[str, tuple]] = []
        head, rest = segs[0], segs[1:]
        if head in ("crate", "$crate"):
            current = [(ctx[0],)]
        elif head == "self":
            current = [ctx]
        elif head == "super":
            cur = self.modules[ctx].parent or ctx
            while rest and rest[0] == "super":
                cur = self.modules[cur].parent or cur
                rest = rest[1:]
            current = [cur]
        else:
            current = self.member(ctx, head, touched, scope=True)
        for seg in rest:
            for cur in current:
                touched.append(("via", cur))
            nxt: list[tuple] = []
            for cur in current:
                nxt += self.member(cur, seg, touched, scope=False)
            current = nxt
            if not current:
                break
        result = (touched, current)
        self._memo[memo_key] = result
        return result

    def member(self, cur: tuple, name: str, touched: list, scope: bool) -> list[tuple]:
        """Look `name` up in module `cur`; return the modules it names (empty for an item).

        With `scope`, `cur` is the module a path is written in, so the crate names its
        manifest declares are in scope too.
        """
        # `#[cfg]` lets one name have several definitions (a feature-off stub beside
        # the real re-export), so every candidate counts, not the first.
        mod = self.modules[cur]
        found: list[tuple] = []
        if len(cur) == 1 and name in self.exported.get(cur[0], {}):
            touched.append(("owner", self.exported[cur[0]][name]))
            return []
        if name in mod.children:
            found.append(mod.children[name])
        if name in mod.items or name in mod.macros:
            touched.append(("owner", cur))
        for path, _ in mod.bindings.get(name, ()):
            sub, ends = self.resolve(cur, path)
            touched.extend(sub)
            found += ends
        if found or name in mod.items or name in mod.macros or name in mod.bindings:
            return found
        for path, _ in mod.globs:
            sub, ends = self.resolve(cur, path)
            for gm in ends:
                before = len(touched)
                inner = self.member(gm, name, touched, scope=False)
                if inner or len(touched) > before:
                    touched.extend(sub)
                    touched.append(("via", gm))
                    return inner
        if scope:
            ident = self.target(cur).externs.get(name)
            if ident and ident in self.lib_root:
                return [self.lib_root[ident]]
        if not scope and mod.item_macros:
            # `bindgen!`, `include!` and their kind define names no text shows.
            touched.append(("owner", cur))
        return []

    # liveness

    def closure(self, with_tests: bool) -> tuple[set[tuple], set[tuple]]:
        used: set[tuple] = set()
        traversed: set[tuple] = set()
        pending: list[tuple] = []
        self.why: dict[tuple, str] = {}
        reason: list = ["root"]
        called: set[str] = set()

        def use(key: tuple) -> None:
            if key not in used:
                used.add(key)
                pending.append(key)
                called.update(self.modules[key].called)
                self.why[key] = reason[0]
            p = self.modules[key].parent
            while p:
                traversed.add(p)
                p = self.modules[p].parent

        def follow(ctx: tuple, refs, macros) -> None:
            for ref in refs:
                reason[0] = (ctx, "writes", ref)
                touched, ends = self.resolve(ctx, ref)
                for k in ends:  # a path naming a module uses it: rustc rejects an unused import
                    use(k)
                for kind, k in touched:
                    if kind == "owner":
                        use(k)
                    else:
                        traversed.add(k)
                        p = self.modules[k].parent
                        while p:
                            traversed.add(p)
                            p = self.modules[p].parent
            for name in macros:
                reason[0] = (ctx, "invokes", [name + "!"])
                owner = self.crate_macros[ctx[0]].get(name)
                if owner:
                    use(owner)

        for key, mod in self.modules.items():
            kind = self.targets[key[0]].kind
            if kind == "bin" or (with_tests and kind == "test"):
                use(key)
        if with_tests:
            for key, mod in self.modules.items():
                follow(key, mod.test_refs, mod.test_macros)
        while True:
            while pending:
                key = pending.pop()
                mod = self.modules[key]
                follow(key, mod.prod_refs, mod.prod_macros)
                if with_tests and mod.is_test:
                    follow(key, mod.test_refs, mod.test_macros)
            grew = False
            for key, mod in self.modules.items():
                if key in used:
                    continue
                for self_ty, trait, methods in mod.impls:
                    if self.impl_is_used(key, self_ty, trait, methods, used, called):
                        reason[0] = (key, "impl", (trait or []) + ["for"] + self_ty)
                        use(key)
                        grew = True
                        break
            if not grew:
                return used, traversed

    def impl_is_used(self, ctx, self_ty, trait, methods, used, called) -> bool:
        """Whether an `impl` block in an otherwise unused module is reached by method calls.

        * `impl Trait for Type`, both in the workspace: the trait must be used (an
          impl of a dead trait is dead), and the type too.
        * `impl ExternalTrait for Type`: the type must be used. Calls through the
          trait name neither module, so this is the conservative answer.
        * `impl Type { fn m … }`: the type must be used, and some used module must
          call one of the block's methods by name.
        """
        owners = {k for kind, k in self.resolve(ctx, self_ty)[0] if kind == "owner"}
        self_used = any(o in used and o != ctx for o in owners)
        if trait:
            t_owners = {k for kind, k in self.resolve(ctx, trait)[0] if kind == "owner"}
            if t_owners:
                return any(o in used and o != ctx for o in t_owners) and (self_used or not owners)
            return self_used
        return self_used and bool(methods & called)


# ── the check ──────────────────────────────────────────────────────────────────────


def dotted(graph: Graph, key: tuple) -> str:
    return "::".join((graph.targets[key[0]].ident,) + key[1:])


def public_path(graph: Graph, key: tuple) -> bool:
    """Part of the crate's public surface, all the way up to the crate root.

    A module is on it when it is declared `pub mod`, or when its parent re-exports
    something from it with `pub use`: `mod tracker; pub use tracker::Tracker;` is as
    public as `pub mod tracker;`, and as invisible to rustc's dead_code lint.
    """
    while len(key) > 1:
        mod = graph.modules[key]
        if mod.is_test or not (mod.declared_pub or reexported_from(graph, mod.parent, key)):
            return False
        key = mod.parent
    return True


def reexported_from(graph: Graph, parent: tuple, child: tuple) -> bool:
    """Whether a `pub use` in `parent` resolves into `child`."""
    mod = graph.modules[parent]
    paths = [p for entries in mod.bindings.values() for p, reexport in entries if reexport]
    paths += [p for p, reexport in mod.globs if reexport]
    for path in paths:
        touched, ends = graph.resolve(parent, path)
        if child in ends or any(k == child for _, k in touched):
            return True
    return False


def subtree_lines(graph: Graph, key: tuple) -> int:
    mod = graph.modules[key]
    total = mod.lines
    for child in mod.children.values():
        if graph.modules[child].file != mod.file:
            total += subtree_lines(graph, child)
    return total


KINDS = ("test-support", "api", "pending")


def read_allowlist(path: Path) -> tuple[dict[str, tuple[str, str]], list[str]]:
    """Entries `<module> <kind> <reason>`; returns them and the malformed lines."""
    entries: dict[str, tuple[str, str]] = {}
    errors: list[str] = []
    if not path.is_file():
        return entries, errors
    for n, line in enumerate(path.read_text().splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, kind, reason = (line.split(None, 2) + ["", ""])[:3]
        where = f"{path.name}:{n}"
        if kind not in KINDS:
            errors.append(f"✗ {where} {name}: kind must be one of {', '.join(KINDS)}, not {kind!r}")
        elif not reason:
            errors.append(f"✗ {where} {name}: no reason given")
        elif kind == "pending" and not re.search(r"#\d+", reason):
            errors.append(f"✗ {where} {name}: a pending entry cites the issue that wires or deletes it (#N)")
        elif name in entries:
            errors.append(f"✗ {where} {name}: listed twice")
        else:
            entries[name] = (kind, reason)
    return entries, errors


def explain(graph: Graph, name: str) -> int:
    """`--why <module>`: the chain of references that makes a module used."""
    by = {dotted(graph, k): k for k in graph.modules}
    key = by.get(name)
    if key is None:
        print(f"no such module: {name}", file=sys.stderr)
        return 1
    if key not in graph.why:
        print(f"{name}: no binary uses it")
        return 0
    seen = set()
    while key and key not in seen:
        seen.add(key)
        why = graph.why[key]
        if why == "root":
            print(f"{dotted(graph, key)}  <=  a binary target")
            break
        ctx, verb, path = why
        print(f"{dotted(graph, key)}  <=  {dotted(graph, ctx)} {verb} {'::'.join(path)}")
        key = ctx
    return 0


def main() -> int:
    if not any(ROOT.glob("crates/*/src/*.rs")):
        print(f"pub-mod-callers: FAIL — no crates/*/src under {ROOT}; refusing to pass vacuously", file=sys.stderr)
        return 1
    graph = Graph(ROOT)
    graph.load()
    if not any(t.kind == "bin" for t in graph.targets.values()):
        print("pub-mod-callers: FAIL — no binary target found; refusing to pass vacuously", file=sys.stderr)
        return 1
    used, traversed = graph.closure(with_tests=False)
    if len(sys.argv) == 3 and sys.argv[1] == "--why":
        return explain(graph, sys.argv[2])
    reached = used | traversed
    test_used, test_traversed = graph.closure(with_tests=True)
    test_reached = test_used | test_traversed
    allow, errors = read_allowlist(ALLOWLIST)
    by_name = {dotted(graph, k): k for k in graph.modules if graph.targets[k[0]].kind == "lib"}
    allowed: dict[tuple, str] = {}
    for name, (kind, _) in sorted(allow.items()):
        key = by_name.get(name)
        if key is None:
            errors.append(f"✗ allowlist entry {name}: no such library module; drop the entry")
        elif key in reached:
            errors.append(f"✗ allowlist entry {name}: a binary uses it; drop the entry")
        else:
            allowed[key] = kind

    def entry_for(key: tuple) -> tuple | None:
        while key:
            if key in allowed:
                return key
            key = graph.modules[key].parent
        return None

    total = 0
    for key in sorted(graph.modules, key=lambda k: dotted(graph, k)):
        if graph.targets[key[0]].kind != "lib" or len(key) == 1 or not public_path(graph, key):
            continue
        if key in reached:
            continue
        parent = graph.modules[key].parent
        if len(parent) > 1 and parent not in reached and entry_for(parent) is None:
            continue  # reported at its topmost unused ancestor
        lines = subtree_lines(graph, key)
        where = f"{dotted(graph, key)} ({graph.modules[key].file.relative_to(ROOT)}, {lines} lines)"
        entry = entry_for(key)
        if entry is None:
            total += lines
            errors.append(f"✗ {where}: no binary uses it")
        elif allowed[entry] != "pending" and key not in test_reached:
            total += lines
            errors.append(f"✗ {where}: allowlisted as {allowed[entry]}, but no test or example uses it either")
    if not errors:
        print(f"pub-mod-callers: OK — every public library module is used by a binary or allowlisted ({len(allowed)} entries)")
        return 0
    for line in errors:
        print(line)
    print(
        f"pub-mod-callers: FAIL — {len(errors)} problem(s), {total} lines. Wire each module into a binary, "
        "delete it, or allowlist it (test-support, api or pending #N) with a reason.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
