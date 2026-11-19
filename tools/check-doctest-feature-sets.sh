#!/usr/bin/env bash
# Run each workspace library's doctests on its own, with its default features, and the
# umbrella `fraiseql` crate's once per feature its documentation examples are gated on.
#
# The workspace doctest pass runs with `--all-features`, so an example that needs a
# feature compiles there and fails for anyone who builds the crate with its defaults.
# `cargo test --workspace --doc` with the defaults does not catch it either: cargo
# unifies features across the members it builds, so a feature one member turns on
# reaches every other. fraiseql-core's `Executor` examples named an adapter behind
# `wire-backend`, and passed that run because another member enables it. Hence one
# `cargo test -p <crate> --doc` per library.
#
# The umbrella's documentation includes docs/*.md files under `cfg_attr(feature = …)`.
# Each such feature gets a run of its own, so a doc gated on one feature that also
# needs another fails here rather than only without `--all-features`.
#
# A library with no doctest is skipped: its run compiles the crate's whole dependency
# graph and tests nothing. tools/check-doctest-skip.py holds that list and refuses it
# when a skipped crate's source holds anything rustdoc could compile.
#
# Every set runs, and each failure is reported before the script fails.
set -uo pipefail

# No git: the CI container mounts the source without `.git`.
cd "$(dirname "$0")/.."

mapfile -t libraries < <(
    cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if any(k in ("lib", "proc-macro") for t in p["targets"] for k in t["kind"]):
        print(p["name"])
' | sort
)

# A list read from the metadata can come back short when its shape changes; a gate over
# no library would pass on anything.
if ((${#libraries[@]} < 20)); then
    echo "check-doctest-feature-sets: read ${#libraries[@]} workspace libraries from" \
        "cargo metadata; expected at least 20." >&2
    exit 2
fi

# The features the umbrella's included docs are gated on, read from its source. Every
# `include_str!` there must read as one of the two forms, plain or gated on one feature,
# so an include written another way (wrapped, or gated on `all(…)`) stops the script
# instead of losing its run.
umbrella=crates/fraiseql/src/lib.rs
gated_re='#\[cfg_attr\(feature = "[a-z0-9-]+", doc = include_str!'
n_all=$(grep -c 'include_str!' "$umbrella")
n_plain=$(grep -cE '#\[doc = include_str!' "$umbrella")
n_gated=$(grep -cE "$gated_re" "$umbrella")
if ((n_all != n_plain + n_gated)); then
    echo "check-doctest-feature-sets: $umbrella has $n_all include_str! lines, of which" \
        "$n_plain read as #[doc = …] and $n_gated as #[cfg_attr(feature = \"…\", doc = …)];" \
        "the rest are in a form this script cannot read." >&2
    exit 2
fi
mapfile -t umbrella_features < <(grep -oE "$gated_re" "$umbrella" | cut -d'"' -f2 | sort -u)

skip_out=$(python3 tools/check-doctest-skip.py)
skip_rc=$?
if ((skip_rc)); then
    exit "$skip_rc"
fi
mapfile -t skipped <<<"$skip_out"
# A skipped name that is no workspace library is a list that outlived its crate.
for name in "${skipped[@]}"; do
    if [[ " ${libraries[*]} " != *" $name "* ]]; then
        echo "check-doctest-feature-sets: $name is skipped by tools/check-doctest-skip.py" \
            "and is not a workspace library." >&2
        exit 2
    fi
done

sets=()
for lib in "${libraries[@]}"; do
    if [[ " ${skipped[*]} " != *" $lib "* ]]; then
        sets+=("$lib")
    fi
done
for feature in "${umbrella_features[@]}"; do
    sets+=("fraiseql --features $feature")
done

failed=()
for set in "${sets[@]}"; do
    read -r -a args <<<"$set"
    echo "=== cargo test --doc -p ${set} ==="
    if ! cargo test --doc -p "${args[@]}" -- --test-threads=6; then
        failed+=("$set")
    fi
done

if ((${#failed[@]})); then
    echo "check-doctest-feature-sets: doctests fail for ${#failed[@]} of ${#sets[@]} sets:" >&2
    printf '  -p %s\n' "${failed[@]}" >&2
    exit 1
fi
echo "check-doctest-feature-sets: ${#sets[@]} sets pass ($((${#libraries[@]} - ${#skipped[@]}))" \
    "libraries with their defaults, ${#skipped[@]} with no doctest skipped, fraiseql with:" \
    "${umbrella_features[*]:-no gated doc})"
