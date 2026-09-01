#!/usr/bin/env python3
"""Cross-SDK conformance harness: author → export → compile → observe.

Every official SDK's contract is one sentence: *the `schema.json` it emits, compiled by
the real `fraiseql compile`, must preserve what the author declared.* Nothing before
this harness tested that sentence.

The pre-existing `sdk-parity.yml` compared each SDK's `schema.json` against the Python
SDK's `schema.json`. It could not have caught any of the nine defects this suite closes,
for two structural reasons:

1. **It never ran the compiler.** `#849` (C# drops `relay`/`is_error`) and `#852` (PHP
   writes `invalidates` where the compiler reads `invalidates_views`) both produce
   schemas that compile cleanly and are silently wrong afterwards. Only comparing the
   *compiled* artifact shows the loss.
2. **Six of eleven "parity generators" hand-wrote their JSON** and never called the SDK
   they claimed to test — Java, C#, F#, Dart, Elixir and Ruby. A generator that
   constructs the expected bytes by hand passes whatever the SDK does.

So this harness runs the SDK's own public authoring API, feeds the result to the real
CLI, and compares *observations* (see `project.py`) rather than bytes.

Usage:

    python3 run.py --cli target/debug/fraiseql-cli          # every SDK in the manifest
    python3 run.py --cli … --sdk go --sdk php               # a subset
    python3 run.py --cli … --update                         # re-record expected*.json

Exit status is 0 only when every selected SDK conformed.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from project import CONSTRUCTS, project

HERE = Path(__file__).resolve().parent
SDK_ROOT = HERE.parent
FIXTURES = ("minimal", "full")

# Per-language dependency caches for the containerized fallback, so a second run does not
# re-download a Maven repository or a NuGet index.
CACHE_ROOT = Path(
    os.environ.get("FRAISEQL_CONFORMANCE_CACHE", Path.home() / ".cache" / "fraiseql-conformance")
)

# Top-level sections the compiler reads as arrays. A producer that marshals an empty
# collection to JSON `null` here fails the compile with `invalid type: null, expected a
# sequence` and no key name — the whole of `#850`, which made every shipped Go example
# uncompilable. Checked before the compile so the diagnostic names the key.
ARRAY_SECTIONS = (
    "types",
    "enums",
    "input_types",
    "interfaces",
    "unions",
    "queries",
    "mutations",
    "subscriptions",
    "fragments",
    "directives",
    "fact_tables",
    "aggregate_queries",
    "observers",
    "sources",
    "custom_scalars",
)


class ConformanceFailure(Exception):
    """A single, reportable reason an SDK did not conform."""


def load_manifest() -> dict:
    return json.loads((HERE / "manifest.json").read_text())


def compile_schema(cli: Path, schema: Path, out: Path) -> dict:
    """Run the real compiler and return the compiled schema.

    Raises `ConformanceFailure` on a non-zero exit, carrying the CLI's own diagnostic —
    which is the whole value of compiling rather than inspecting: the compiler names the
    offending key, and that message is what an SDK author would actually see.
    """
    result = subprocess.run(
        [str(cli), "compile", str(schema), "-o", str(out)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ConformanceFailure(
            f"`fraiseql compile` rejected the export:\n"
            f"{(result.stderr or result.stdout).strip()}"
        )
    return json.loads(out.read_text())


def check_no_null_sections(schema_json: dict) -> None:
    """#850: an absent section must be omitted or `[]`, never `null`."""
    nulls = [key for key in ARRAY_SECTIONS if key in schema_json and schema_json[key] is None]
    if nulls:
        raise ConformanceFailure(
            f"exported `null` for array section(s) {nulls}. An SDK with no entries in a "
            f"section must omit the key or emit `[]` — `null` is rejected by the compiler "
            f"with `invalid type: null, expected a sequence` and no key name."
        )


def force_container(sdk: str) -> bool:
    """Whether `FRAISEQL_CONFORMANCE_FORCE_CONTAINER` selects this SDK.

    `1`/`all` forces every SDK; otherwise a comma-separated list of SDK names. Needed
    because "the tool is on PATH" is not the same as "the tool works": a box with a JRE
    has `mvn` and cannot compile, and the resulting `No compiler is provided` reads like
    an SDK defect rather than a local-environment one.
    """
    setting = os.environ.get("FRAISEQL_CONFORMANCE_FORCE_CONTAINER", "").strip()
    if not setting:
        return False
    if setting in {"1", "all"}:
        return True
    return sdk in {name.strip() for name in setting.split(",")}


def containerized(spec: dict, fixture: str, out: Path, workdir: Path) -> list[str]:
    """Wrap an SDK's export command in `docker run` against its declared image.

    Used when the SDK's toolchain is not installed locally. CI installs the real
    toolchains, so this is a developer convenience — but it is what makes "run the whole
    suite" a single command on a machine that does not have eleven language runtimes,
    and a suite nobody can run locally is a suite that only ever fails in CI.
    """
    return [
        "docker", "run", "--rm",
        "-v", f"{workdir}:/sdk",
        "-v", f"{out.parent}:/out",
        "-v", f"{CACHE_ROOT}:/conformance-cache",
        "-w", "/sdk",
        "-e", f"FRAISEQL_CONFORMANCE_FIXTURE={fixture}",
        "-e", f"FRAISEQL_CONFORMANCE_OUT=/out/{out.name}",
        *[arg for var in spec.get("container_env", []) for arg in ("-e", var)],
        "--user", f"{os.getuid()}:{os.getgid()}",
        spec["container"],
        *spec["export"],
    ]


def run_exporter(sdk: str, spec: dict, fixture: str, out: Path) -> dict:
    """Invoke an SDK's conformance exporter and read back the schema it wrote."""
    workdir = SDK_ROOT / spec["dir"]
    env = {
        **os.environ,
        "FRAISEQL_CONFORMANCE_FIXTURE": fixture,
        "FRAISEQL_CONFORMANCE_OUT": str(out),
    }
    command = spec["export"]
    unavailable = spec.get("requires") and not shutil.which(spec["requires"])
    if spec.get("container") and (unavailable or force_container(sdk)):
        command = containerized(spec, fixture, out, workdir)

    result = subprocess.run(
        command,
        cwd=workdir,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise ConformanceFailure(
            f"exporter `{' '.join(spec['export'])}` failed in {workdir} "
            f"(exit {result.returncode}):\n{(result.stdout + result.stderr).strip()[-4000:]}"
        )
    if not out.exists():
        raise ConformanceFailure(
            f"exporter `{' '.join(spec['export'])}` exited 0 but wrote nothing to "
            f"$FRAISEQL_CONFORMANCE_OUT. Exiting 0 without producing a schema is the "
            f"fabricated-success shape this suite exists to catch."
        )
    return json.loads(out.read_text())


def diff_observations(
    expected: dict, actual: dict, unsupported: dict[str, str], compiled: dict | None = None
) -> list[str]:
    """Compare per construct, skipping (and reporting) declared gaps."""
    problems = []
    for construct in CONSTRUCTS:
        if construct in unsupported:
            continue
        want = expected.get(construct)
        got = actual.get(construct)
        if want != got:
            problems.append(
                f"  construct `{construct}`:\n"
                f"    expected: {json.dumps(want, sort_keys=True)}\n"
                f"    actual:   {json.dumps(got, sort_keys=True)}"
                + casing_hint(construct, want, got, compiled)
            )
    return problems


def casing_hint(construct: str, want: object, got: object, compiled: dict | None) -> str:
    """Name the near-miss when an operation was published under a different convention.

    `project()` indexes the operation constructs by the names the canonical fixture
    declares, so an SDK that publishes `tenant_orders` where the fixture expects
    `tenantOrders` produces an observation with the key simply *absent* — and the diff
    above then reads as "this SDK registered no such query", which is the wrong thing to
    go and look for. The projection stays strict on purpose; the hint is read off the raw
    compiled schema instead, so naming the near-miss cannot weaken the comparison (#1255).
    """
    section = {
        "queries": "queries",
        "query_arguments": "queries",
        "query_inject_params": "queries",
        "query_cache_ttl": "queries",
        "query_requires_role": "queries",
        "query_requires_actor": "queries",
        "mutations": "mutations",
        "mutation_arguments": "mutations",
        "mutation_invalidates_views": "mutations",
        "mutation_invalidates_fact_tables": "mutations",
        "mutation_requires_role": "mutations",
        "mutation_requires_actor": "mutations",
        "subscriptions": "subscriptions",
    }.get(construct)
    if section is None or not isinstance(compiled, dict) or not isinstance(want, dict):
        return ""
    published = [
        item["name"]
        for item in compiled.get(section) or []
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    ]
    got_keys = set(got) if isinstance(got, dict) else set()

    def fold(name: str) -> str:
        return name.replace("_", "").lower()

    by_fold = {fold(n): n for n in published}
    pairs = [
        (expected_name, by_fold[fold(expected_name)])
        for expected_name in want
        if expected_name not in got_keys
        and fold(expected_name) in by_fold
        and by_fold[fold(expected_name)] != expected_name
    ]
    if not pairs:
        return ""
    joined = ", ".join(f"`{a}` published as `{b}`" for a, b in sorted(pairs))
    return f"\n    note:     same name, different convention — {joined}"


def exercised(value: object) -> bool:
    """Whether the canonical fixture actually declares anything for this construct.

    The `minimal` fixture leaves most constructs empty on purpose, and an empty
    expectation matches an empty result for a reason that has nothing to do with support.
    Only a construct the fixture populates can tell a declared gap from a real one.
    """
    if isinstance(value, dict):
        # `type_relay` is a dict of sub-observations that is never empty; it counts as
        # exercised only when something in it is actually set.
        return any(bool(item) for item in value.values())
    return bool(value)


def check_undeclared_support(expected: dict, actual: dict, unsupported: dict[str, str]) -> list[str]:
    """Report a construct declared unsupported that in fact works.

    A stale `unsupported` entry is a published falsehood — the SDK support matrix is
    generated from these declarations — and it silently un-gates a construct that is
    working today and could regress tomorrow.
    """
    return [
        f"  construct `{construct}` is declared unsupported ({reason!r}) but the export "
        f"satisfies it. Remove the declaration so the construct is gated."
        for construct, reason in unsupported.items()
        if construct in CONSTRUCTS
        and exercised(expected.get(construct))
        and expected.get(construct) == actual.get(construct)
    ]


def check_sdk(sdk: str, spec: dict, cli: Path, expected: dict[str, dict]) -> list[str]:
    """Run both fixtures through one SDK. Returns a list of failure reports."""
    unsupported = spec.get("unsupported", {})
    unknown = set(unsupported) - set(CONSTRUCTS)
    if unknown:
        return [
            f"manifest declares unsupported construct(s) {sorted(unknown)} that are not in "
            f"project.CONSTRUCTS — a typo here silently disables a real gate."
        ]

    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = Path(tmp)
        for fixture in FIXTURES:
            schema_path = tmpdir / f"{sdk}.{fixture}.json"
            compiled_path = tmpdir / f"{sdk}.{fixture}.compiled.json"
            try:
                exported = run_exporter(sdk, spec, fixture, schema_path)
                check_no_null_sections(exported)
                compiled = compile_schema(cli, schema_path, compiled_path)
            except ConformanceFailure as exc:
                failures.append(f"[{fixture}] {exc}")
                continue

            problems = diff_observations(expected[fixture], project(compiled), unsupported, compiled)
            problems += check_undeclared_support(expected[fixture], project(compiled), unsupported)
            if problems:
                failures.append(f"[{fixture}] observations differ:\n" + "\n".join(problems))
    return failures


def reference_expectations(cli: Path, update: bool) -> dict[str, dict]:
    """Compile the canonical reference fixtures and check (or record) their projection.

    This runs before any SDK and is a gate in its own right: the reference `schema.json`
    files are the normative worked example in `docs/architecture/intermediate-schema.md`,
    and a compiler change that alters what an author's declaration compiles to shows up
    here first, once, instead of as eleven simultaneous SDK failures.
    """
    expectations = {}
    with tempfile.TemporaryDirectory() as tmp:
        for fixture in FIXTURES:
            reference = HERE / "reference" / f"{fixture}.json"
            compiled = Path(tmp) / f"{fixture}.compiled.json"
            observations = project(compile_schema(cli, reference, compiled))
            expected_path = HERE / f"expected.{fixture}.json"
            if update:
                expected_path.write_text(json.dumps(observations, indent=2, sort_keys=True) + "\n")
            recorded = json.loads(expected_path.read_text())
            if recorded != observations:
                raise SystemExit(
                    f"FAIL reference/{fixture}.json no longer compiles to "
                    f"expected.{fixture}.json.\n"
                    f"  expected: {json.dumps(recorded, sort_keys=True)}\n"
                    f"  actual:   {json.dumps(observations, sort_keys=True)}\n"
                    f"If the compiler change is intended, re-record with --update and "
                    f"review the diff."
                )
            expectations[fixture] = observations

    # A construct the `full` fixture does not populate gates nothing: its expectation is
    # empty, every SDK's result is empty, and they match — so it passes for all eleven
    # while `exercised()` also reports it unexercised, which suppresses the stale-gap
    # check. Two constructs would silently be in that state without this, and the failure
    # is invisible by construction: it looks exactly like universal support.
    unexercised = [c for c in CONSTRUCTS if not exercised(expectations["full"].get(c))]
    if unexercised:
        raise SystemExit(
            f"FAIL the `full` reference fixture declares nothing for construct(s) "
            f"{unexercised}. A construct the fixture does not exercise passes for every "
            f"SDK regardless of support — add it to reference/full.json, or remove it "
            f"from project.CONSTRUCTS."
        )
    return expectations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, help="path to the fraiseql-cli binary")
    parser.add_argument("--sdk", action="append", default=[], help="limit to these SDKs")
    parser.add_argument("--update", action="store_true", help="re-record expected*.json")
    parser.add_argument(
        "--require-all",
        action="store_true",
        help="treat an unrunnable SDK as a failure instead of a skip (use this in CI: a "
        "skipped SDK reads exactly like a passing one in a log)",
    )
    args = parser.parse_args()

    cli = Path(args.cli).resolve()
    if not cli.is_file():
        raise SystemExit(f"FAIL no CLI binary at {cli} — build it with `cargo build -p fraiseql-cli`")

    manifest = load_manifest()
    expected = reference_expectations(cli, args.update)
    print(f"ok   reference fixtures compile to the recorded observations ({len(CONSTRUCTS)} constructs)")

    selected = args.sdk or list(manifest["sdks"])
    unknown = set(selected) - set(manifest["sdks"])
    if unknown:
        raise SystemExit(f"FAIL unknown SDK(s): {sorted(unknown)}")

    CACHE_ROOT.mkdir(parents=True, exist_ok=True)
    failed = []
    for sdk in selected:
        spec = manifest["sdks"][sdk]
        runnable = not spec.get("requires") or shutil.which(spec["requires"])
        if not runnable and spec.get("container") and shutil.which("docker"):
            runnable = True
        if not runnable:
            reason = f"`{spec['requires']}` is not on PATH"
            if spec.get("container"):
                reason += " and docker is unavailable for the container fallback"
            if args.require_all:
                failed.append(sdk)
                print(f"FAIL {sdk}: {reason}")
            else:
                print(f"SKIP {sdk}: {reason}")
            continue
        failures = check_sdk(sdk, spec, cli, expected)
        gaps = spec.get("unsupported", {})
        if failures:
            failed.append(sdk)
            print(f"FAIL {sdk}")
            for failure in failures:
                print("     " + failure.replace("\n", "\n     "))
        else:
            note = f" ({len(gaps)} declared gap(s))" if gaps else ""
            print(f"ok   {sdk}{note}")

    if failed:
        print(f"\n{len(failed)} SDK(s) failed conformance: {', '.join(failed)}")
        return 1
    print(f"\nall {len(selected)} selected SDK(s) conform")
    return 0


if __name__ == "__main__":
    sys.exit(main())
