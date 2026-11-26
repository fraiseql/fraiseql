#!/usr/bin/env bash
# Document fraiseql-server once per feature that gates one of its modules, with
# `-D warnings` (#2089).
#
# The two workspace rustdoc passes cannot see a link from one optional subsystem to
# another: `--all-features` resolves every target, and the default build compiles
# neither end. `--features functions-runtime` alone compiled modules whose docs linked
# `crate::sources`, `fraiseql_observers::RedisCacheInvalidator` and
# `crate::inbound::email::SmtpMailboxTransport`, none of which that feature enables.
#
# The feature list is read from the source: every feature named in a `cfg` on a `mod`
# declaration under crates/fraiseql-server/src, less the default features (the default
# pass covers those). A module gated behind a new feature joins this gate with no edit.
#
# fraiseql-functions is documented once per runtime (`runtime-*`) feature alone, with no
# default features: each runtime's docs may link only into what that runtime builds, and
# `--all-features` enables every runtime at once. Its runtime features are read from its
# manifest, so a new runtime joins this gate with no edit.
#
# Every set is documented, and each failure is reported before the script fails.
set -uo pipefail

# No git: the CI container mounts the source without `.git`.
cd "$(dirname "$0")/.."
manifest=crates/fraiseql-server/Cargo.toml

defaults=$(sed -n 's/^default *= *\[\(.*\)\]/\1/p' "$manifest" | tr -d '" ' | tr ',' '\n')
mapfile -t features < <(
    grep -rh --include='*.rs' -B2 -E '^\s*(pub(\(crate\))? )?mod [a-z_]+;' crates/fraiseql-server/src \
        | grep -oE 'feature = "[a-z0-9-]+"' | cut -d'"' -f2 | sort -u \
        | grep -vxF -f <(printf '%s\n' "$defaults")
)

# A list read from the source can come back empty when the source changes shape; a
# gate over no feature set would pass on anything.
if ((${#features[@]} < 10)); then
    echo "check-rustdoc-feature-sets: read ${#features[@]} module-gating features from" \
        "crates/fraiseql-server/src; expected at least 10. Has the cfg-on-mod layout changed?" >&2
    exit 2
fi

failed=()
for feature in "${features[@]}"; do
    echo "=== rustdoc -D warnings: fraiseql-server --features $feature ==="
    if ! cargo rustdoc -p fraiseql-server --features "$feature" -- -D warnings; then
        failed+=("$feature")
    fi
done

functions_manifest=crates/fraiseql-functions/Cargo.toml
mapfile -t runtimes < <(sed -n 's/^\(runtime-[a-z0-9-]*\) *=.*/\1/p' "$functions_manifest")
if ((${#runtimes[@]} < 2)); then
    echo "check-rustdoc-feature-sets: read ${#runtimes[@]} runtime features from" \
        "$functions_manifest; expected at least 2. Has its [features] table changed shape?" >&2
    exit 2
fi
functions_failed=()
for runtime in "${runtimes[@]}"; do
    echo "=== rustdoc -D warnings: fraiseql-functions --no-default-features --features $runtime ==="
    if ! cargo rustdoc -p fraiseql-functions --no-default-features --features "$runtime" \
        -- -D warnings; then
        functions_failed+=("$runtime")
    fi
done

if ((${#failed[@]})); then
    echo "check-rustdoc-feature-sets: fraiseql-server docs fail with --features: ${failed[*]}" >&2
fi
if ((${#functions_failed[@]})); then
    echo "check-rustdoc-feature-sets: fraiseql-functions docs fail with --features:" \
        "${functions_failed[*]} (no default features)" >&2
fi
if ((${#failed[@]} + ${#functions_failed[@]})); then
    exit 1
fi
echo "check-rustdoc-feature-sets: ${#features[@]} fraiseql-server and ${#runtimes[@]}" \
    "fraiseql-functions feature sets documented cleanly"
