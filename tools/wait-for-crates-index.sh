#!/usr/bin/env bash
# Wait until each <crate>@<version> is resolvable for the NEXT release tier's
# `cargo publish`.
#
# `cargo publish` resolves dependency versions from the SPARSE INDEX
# (index.crates.io), which lags the crates.io API by tens of seconds. The original
# tier-waits polled only the API (`/api/v1/crates/X/$VERSION` -> 200), so a tier
# could proceed before the index had propagated — exactly the v2.5.0 partial
# publish, where Tier-4 fraiseql-core failed with "failed to select a version for
# fraiseql-wire ^2.5.0" although wire was already API-visible. Poll BOTH: the sparse
# index is the load-bearing check (it gates the break), the API is belt-and-
# suspenders (reported, never gating).
#
# Usage: wait-for-crates-index.sh <version> <crate> [<crate> ...]
#
# Tunables (env):
#   CRATES_INDEX_MAX_ATTEMPTS  poll attempts per crate (default 40)
#   CRATES_INDEX_SLEEP_SECS    delay between attempts  (default 10)
# Default budget ~= 6.5 min/crate (the index usually lags only tens of seconds; the
# budget is generous so propagation almost always wins). On timeout this HARD-FAILS
# (exit 1) rather than warn-and-proceed: the v2.13.0 federation publish failed
# mid-sequence precisely because the old wait warned and proceeded, letting a later
# tier's `cargo publish` resolve against a not-yet-indexed sibling. The bounded retry
# above IS the absorb-propagation-lag mechanism; a timeout after that budget is a real
# stall, so we stop the release rather than march into a partial publish.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tools/lib/release_helpers.sh
source "$SCRIPT_DIR/lib/release_helpers.sh"

if [ "$#" -lt 2 ]; then
    echo "usage: $(basename "$0") <version> <crate> [<crate> ...]" >&2
    exit 2
fi

version="$1"
shift
max_attempts="${CRATES_INDEX_MAX_ATTEMPTS:-40}"
sleep_secs="${CRATES_INDEX_SLEEP_SECS:-10}"
ua='User-Agent: fraiseql-ci'

for crate in "$@"; do
    idx_url="$(index_url_for "$crate")"
    echo "Waiting for $crate@$version to be indexed on crates.io ($idx_url)..."
    indexed=0
    for _ in $(seq 1 "$max_attempts"); do
        # Load-bearing: the sparse index is what `cargo publish` resolves from.
        body="$(curl -s -H "$ua" "$idx_url" || true)"
        if index_body_has_version "$body" "$version"; then
            # Belt-and-suspenders: report the API status too, but do not gate on it.
            api_code="$(curl -s -o /dev/null -w '%{http_code}' -H "$ua" \
                "https://crates.io/api/v1/crates/$crate/$version" || true)"
            echo "✅ $crate@$version in sparse index (crates.io API HTTP $api_code)"
            indexed=1
            break
        fi
        sleep "$sleep_secs"
    done
    if [ "$indexed" -ne 1 ]; then
        echo "::error::$crate@$version not visible in the sparse index ($idx_url) after $((max_attempts * sleep_secs))s — refusing to proceed. Publishing the next tier now risks resolving a dependency against a stale index (the v2.13.0 federation mid-sequence failure). Wait for propagation and re-run, or raise CRATES_INDEX_MAX_ATTEMPTS / CRATES_INDEX_SLEEP_SECS." >&2
        exit 1
    fi
done
