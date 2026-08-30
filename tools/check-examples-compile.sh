#!/usr/bin/env bash
# check-examples-compile.sh — every authoring artifact a shipped example carries must
# compile with the CLI in this tree.
#
# Background (Phase 09, #1052 and #1168): `examples/streaming/schema.json` is a JSON
# object keyed by type name where the compiler reads a sequence, so it has never been
# compilable by any version of this CLI; and `examples/async-jobs-subgraph/…/fraiseql.toml`
# sets `[database] pool_size`, which no version of the config schema accepts, so the file
# is refused at parse time. Both shipped for months. Nothing compiled them, because
# nothing in CI compiled any example at all.
#
# 25 example directories carry exactly six tracked authoring artifacts between them, so
# this gate is cheap. Its cost is one CLI binary, which is why it lives in a leg that has
# a Rust toolchain rather than in ShellGates.
#
# ⚠ `[domain_discovery]` resolves its root against the process's working directory, so
# each artifact is compiled FROM ITS OWN DIRECTORY. Compiling from the repo root silently
# discovers a different (or empty) domain and can pass on a file that a reader following
# the README cannot compile.
#
# ⚠ This gate never skips. A missing binary is a failure, not a pass: the whole point of
# the phase that produced it is that a check which quietly does nothing reads as green.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Accept a prebuilt binary (the CI leg builds one anyway); otherwise use whichever
# profile is on disk. Building here is deliberate: `cargo build` for a leg that already
# has the artifact is wasted minutes, and a silent `command -v fraiseql` could pick up an
# INSTALLED release from a different version — the one thing this gate must not measure.
FRAISEQL_BIN="${FRAISEQL_BIN:-}"
if [ -z "$FRAISEQL_BIN" ]; then
    for candidate in target/release/fraiseql target/debug/fraiseql; do
        if [ -x "$candidate" ]; then FRAISEQL_BIN="$REPO_ROOT/$candidate"; break; fi
    done
fi
if [ -z "$FRAISEQL_BIN" ] || [ ! -x "$FRAISEQL_BIN" ]; then
    echo "✗ no fraiseql binary to compile with."
    echo "    Build one:  cargo build -p fraiseql --bin fraiseql --features cli"
    echo "    Or point FRAISEQL_BIN at it. This gate does not skip."
    exit 1
fi
echo "→ compiling every example authoring artifact with $("$FRAISEQL_BIN" --version 2>/dev/null || echo "$FRAISEQL_BIN")"

# find, not `git ls-files`: the Dagger legs ignore `.git` and `git init` an empty repo,
# so an ls-files loop iterates over nothing and the gate cannot fail.
artifacts=$(find examples -name 'fraiseql.toml' -o -name 'schema.json' | sort)
if [ -z "$artifacts" ]; then
    echo "✗ found no example authoring artifacts at all — the search is wrong, not the tree."
    exit 1
fi

# The authoring step is part of the chain, so the gate runs it. `examples/…/schema.py`
# declares the types; running it writes types.json (TOML workflow) or schema.json
# (legacy). Compiling only the committed artifact would have missed the defect that
# motivated this: five example schema.py files still `from fraiseql import type, key`,
# and `key` has not existed in the SDK for a major version — every one of them raises
# ImportError before it can emit anything.
#
# Everything happens in a pristine COPY of the example directory. The gate must never
# mutate the tree it is checking: an authoring step writes its output next to itself,
# and a gate that leaves the working tree dirty is one a developer learns to skip.
SDK_SRC="$REPO_ROOT/sdks/official/fraiseql-python/src"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

python_ok=1
if ! PYTHONPATH="$SDK_SRC" python3 -c 'import fraiseql' 2>/dev/null; then
    python_ok=0
fi

# ---------------------------------------------------------------------------
# Known-broken, each with the issue that owns it.
#
# These directories are v1-era rot that predates this gate, filed rather than
# repaired so the gate could land without being red on trunk from day one — a red
# gate nobody can turn green teaches the next reader to skip it (the lesson of
# check-feature-chains.sh, #1055/#990).
#
# The list is checked in BOTH directions. An entry that starts PASSING is a failure,
# because the exemption is now a lie and the only thing keeping it in the file is
# that nobody looked. That is what stops this list from becoming permanent.
# ---------------------------------------------------------------------------
declare -A KNOWN_BROKEN=(
    ["examples/ltree-hierarchical-data/organization-chart/schema.py"]="#1191"
    ["examples/ltree-hierarchical-data/product-catalog/schema.py"]="#1191"
)
declare -A SAW_BROKEN=()

# Record a failure. A path on the known-broken list is reported and tolerated;
# anything else fails the gate.
record_failure() {
    local path="$1"
    if [ -n "${KNOWN_BROKEN[$path]:-}" ]; then
        SAW_BROKEN["$path"]=1
        return 0
    fi
    found=1
}

found=0
count=0
authored=0
while IFS= read -r artifact; do
    [ -n "$artifact" ] || continue
    count=$((count + 1))
    dir="$(dirname "$artifact")"
    base="$(basename "$artifact")"

    work="$WORKDIR/$(printf '%s' "$dir" | tr '/' '_')"
    rm -rf "$work"
    cp -r "$dir" "$work"

    # 1. Authoring, when this example has an authoring source.
    types_arg=()
    if [ -f "$work/schema.py" ]; then
        if [ "$python_ok" -ne 1 ]; then
            echo "  FAIL $dir/schema.py — cannot import the in-repo authoring SDK."
            echo "         This gate does not skip. Provide python3 and the SDK's"
            echo "         dependencies (httpx) in whatever runs it."
            record_failure "$dir/schema.py"
            continue
        fi
        authored=$((authored + 1))
        if ! out=$(cd "$work" && PYTHONPATH="$SDK_SRC" python3 schema.py 2>&1); then
            echo "  FAIL $dir/schema.py does not run"
            printf '%s\n' "$out" | tail -5 | sed 's/^/         /'
            record_failure "$dir/schema.py"
            continue
        fi
        [ -f "$work/types.json" ] && types_arg=(--types types.json)
    fi

    # 2. Compilation, from the example's own directory — `[domain_discovery]`
    #    resolves root_dir against the CWD, so compiling from the repo root can
    #    discover a different (or empty) domain and pass on a file a reader cannot
    #    compile.
    if out=$(cd "$work" && "$FRAISEQL_BIN" compile "$base" "${types_arg[@]}" --check 2>&1); then
        echo "  ok   $artifact${types_arg[*]:+ (+ types.json from schema.py)}"
    else
        echo "  FAIL $artifact${types_arg[*]:+ (+ types.json from schema.py)}"
        printf '%s\n' "$out" | sed 's/^/         /'
        record_failure "$artifact"
    fi
done <<< "$artifacts"

# Every authoring source, including those in examples that ship no compilable
# artifact of their own — a schema.py that cannot run is broken whether or not
# anything compiles its output.
while IFS= read -r script; do
    dir="$(dirname "$script")"
    [ -f "$dir/fraiseql.toml" ] || [ -f "$dir/schema.json" ] && continue
    count=$((count + 1))
    if [ "$python_ok" -ne 1 ]; then
        echo "  FAIL $script — cannot import the in-repo authoring SDK (see above)."
        record_failure "$script"
        continue
    fi
    work="$WORKDIR/orphan_$(printf '%s' "$dir" | tr '/' '_')"
    rm -rf "$work"; cp -r "$dir" "$work"
    if out=$(cd "$work" && PYTHONPATH="$SDK_SRC" python3 schema.py 2>&1); then
        echo "  ok   $script (authoring only)"
        authored=$((authored + 1))
    else
        echo "  FAIL $script does not run"
        printf '%s\n' "$out" | tail -5 | sed 's/^/         /'
        record_failure "$script"
    fi
done < <(find examples -name 'schema.py' | sort)

# The other direction: an exemption that no longer describes anything is a claim the
# tree has stopped making. Fail on it, so closing #1190 or #1191 also deletes its row
# here rather than leaving a permanent hole.
stale=()
for path in "${!KNOWN_BROKEN[@]}"; do
    if [ -z "${SAW_BROKEN[$path]:-}" ]; then
        stale+=("$path (${KNOWN_BROKEN[$path]})")
    fi
done

if [ "${#SAW_BROKEN[@]}" -gt 0 ]; then
    echo
    echo "  ${#SAW_BROKEN[@]} known-broken example(s) tolerated, each owned by an open issue:"
    # Sorted: bash iterates an associative array in hash order, and a gate whose
    # output differs run to run cannot be diffed against a previous run.
    for path in "${!SAW_BROKEN[@]}"; do
        echo "    $path  ${KNOWN_BROKEN[$path]}"
    done | sort
fi

if [ "${#stale[@]}" -gt 0 ]; then
    echo
    echo "✗ these are on the known-broken list but now COMPILE:"
    printf '    %s\n' "${stale[@]}" | sort
    echo "  Delete the row from KNOWN_BROKEN in this file and close the issue."
    echo "  An exemption nobody removes is how a gate stops meaning anything."
    exit 1
fi

if [ "$found" -ne 0 ]; then
    echo
    echo "examples/ compile gate FAILED — an example ships an artifact the CLI cannot read,"
    echo "or an authoring source that does not run."
    exit 1
fi
echo "OK: $count example artifacts compile ($authored authored from schema.py first,"
echo "    ${#SAW_BROKEN[@]} known-broken and tracked)."
