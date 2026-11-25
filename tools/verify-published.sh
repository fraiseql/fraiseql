#!/usr/bin/env bash
# verify-published.sh <version> — every artifact a release publishes is visible at <version>.
#
# The crates are the workspace's publishable crates, read from the manifests by
# `tools/publish-order.py --set` (no toolchain needed), so a new crate is verified
# without editing a list. Beside them: the Python package on PyPI, the TypeScript
# package on npm, and the Rust SDK `fraiseql-rust`, which is not a workspace member.
#
# A collect-all check: it probes every artifact, retries the missing ones to absorb
# registry indexing lag, and fails once at the end naming each artifact that never
# became visible. A missing artifact is never soft-passed.
#
# Environment:
#   GITHUB_STEP_SUMMARY  where the per-artifact summary goes (default: /dev/null)
#   VERIFY_ROUNDS        probe rounds before failing (default 12)
#   VERIFY_SLEEP         seconds between rounds (default 15)
#   VERIFY_ROOT          the workspace whose crates are verified (default: this one)
set -uo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "usage: $0 <version>" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
rounds="${VERIFY_ROUNDS:-12}"
pause="${VERIFY_SLEEP:-15}"

if ! crates="$(python3 "$repo_root/tools/publish-order.py" --set --root "${VERIFY_ROOT:-$repo_root}")"; then
    echo "::error::could not read the publishable crates from the workspace"
    exit 1
fi
if [ -z "$crates" ]; then
    echo "::error::the workspace names no publishable crates; nothing would be verified"
    exit 1
fi

echo "## 📦 Package Verification" >>"$summary"
echo "" >>"$summary"

# One "label|url" item per array element.
pending=()
while IFS= read -r c; do
    pending+=("crates.io: $c v$VERSION|https://crates.io/api/v1/crates/$c/$VERSION")
done <<<"$crates"
pending+=("PyPI: fraiseql v$VERSION|https://pypi.org/pypi/fraiseql/$VERSION/json")
pending+=("npm: fraiseql v$VERSION|https://registry.npmjs.org/fraiseql/$VERSION")
pending+=("crates.io: fraiseql-rust v$VERSION|https://crates.io/api/v1/crates/fraiseql-rust/$VERSION")

for round in $(seq 1 "$rounds"); do
    still=()
    for item in "${pending[@]}"; do
        label="${item%%|*}"
        url="${item##*|}"
        code="$(curl -s -o /dev/null -w '%{http_code}' -H 'User-Agent: fraiseql-ci' "$url")"
        if [ "$code" = "200" ]; then
            echo "✅ $label" >>"$summary"
        else
            still+=("$item")
        fi
    done
    pending=("${still[@]}")
    [ "${#pending[@]}" -eq 0 ] && break
    if [ "$round" -lt "$rounds" ]; then
        echo "… ${#pending[@]} not yet visible; retry in ${pause}s (round $round/$rounds)"
        sleep "$pause"
    fi
done

if [ "${#pending[@]}" -ne 0 ]; then
    echo "" >>"$summary"
    echo "### ❌ Never became visible" >>"$summary"
    for item in "${pending[@]}"; do
        echo "- ${item%%|*}" >>"$summary"
        echo "::error::never became visible: ${item%%|*}"
    done
    echo "::error::one or more published artifacts never became visible after $rounds round(s) — see the job summary"
    exit 1
fi
echo "All $(wc -l <<<"$crates") crates, the Rust SDK (crates.io), PyPI and npm are published and visible at v$VERSION."
