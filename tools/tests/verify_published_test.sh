#!/usr/bin/env bash
# verify_published_test.sh — tools/verify-published.sh against a stubbed registry.
#
# The script runs for real only after a release has published, so without this the
# first run of its logic would be a release. A stub `curl` on PATH answers every
# probe and logs the URL, so each case can say exactly what was asked for:
#
#   - the probes are the workspace's publishable crates, read from the manifests,
#     plus PyPI, npm and the Rust SDK — each exactly once when all are visible;
#   - a crate added to a workspace is probed with no list edited;
#   - a crate that never becomes visible fails the run after VERIFY_ROUNDS rounds,
#     and is named; one that turns up in a later round passes;
#   - a workspace publishing nothing, or a missing version, fails.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
script="$repo_root/tools/verify-published.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0

# The stub: 404 for a URL listed in $STUB_MISSING, and for one in $STUB_LATE the first
# time it is asked for; 200 otherwise. Every URL asked for is appended to $STUB_LOG.
mkdir -p "$tmp/bin"
cat >"$tmp/bin/curl" <<'STUB'
#!/usr/bin/env bash
url="${!#}"
echo "$url" >>"$STUB_LOG"
if grep -qxF -- "$url" "$STUB_MISSING" 2>/dev/null; then
    printf 404
elif grep -qxF -- "$url" "$STUB_LATE" 2>/dev/null && [ "$(grep -cxF -- "$url" "$STUB_LOG")" -eq 1 ]; then
    printf 404
else
    printf 200
fi
STUB
chmod +x "$tmp/bin/curl"

crate_url() { echo "https://crates.io/api/v1/crates/$1/9.9.9"; }

# The probes a release of the workspace at <root> must make, sorted.
expected_probes() {
    {
        python3 "$repo_root/tools/publish-order.py" --set --root "$1" | while IFS= read -r c; do crate_url "$c"; done
        echo "https://pypi.org/pypi/fraiseql/9.9.9/json"
        echo "https://registry.npmjs.org/fraiseql/9.9.9"
        crate_url fraiseql-rust
    } | sort
}

# run <label> [env assignments…] -- <args…>: run the script with the stub, fresh logs.
run() {
    : >"$tmp/log"
    : >"$tmp/missing"
    : >"$tmp/late"
    [ -n "${MISSING:-}" ] && printf '%s\n' $MISSING >"$tmp/missing"
    [ -n "${LATE:-}" ] && printf '%s\n' $LATE >"$tmp/late"
    PATH="$tmp/bin:$PATH" STUB_LOG="$tmp/log" STUB_MISSING="$tmp/missing" STUB_LATE="$tmp/late" \
        VERIFY_SLEEP=0 GITHUB_STEP_SUMMARY="$tmp/summary" "$@" >"$tmp/out" 2>&1
}

check() {
    cases=$((cases + 1))
    if [ "$1" = ok ]; then echo "  ok   $2"; else
        echo "  FAIL $2"
        sed 's/^/       /' "$tmp/out"
        failures=$((failures + 1))
    fi
}

# A two-crate workspace, so a case can add a crate to it.
fixture() {
    rm -rf "$tmp/ws"
    mkdir -p "$tmp/ws"
    printf '[workspace]\nmembers = ["crates/*"]\n' >"$tmp/ws/Cargo.toml"
    for c in "$@"; do
        mkdir -p "$tmp/ws/crates/$c"
        printf '[package]\nname = "%s"\nversion = "0.1.0"\n' "$c" >"$tmp/ws/crates/$c/Cargo.toml"
    done
}

echo "verify-published.sh:"

# --- this workspace, everything visible ---------------------------------------------
MISSING="" LATE="" run bash "$script" 9.9.9
rc=$?
probes="$(sort "$tmp/log")"
[ "$rc" -eq 0 ] && [ "$probes" = "$(expected_probes "$repo_root")" ] && ok=ok || ok=no
check "$ok" "all visible: exactly this workspace's crates, PyPI, npm and fraiseql-rust, once each"
grep -qxF "$(crate_url fraiseql-time)" "$tmp/log" && ok=ok || ok=no
check "$ok" "...including fraiseql-time, which the old hand-kept list had to be edited for"

# --- a crate added to a workspace is probed with no list edited ----------------------
fixture alpha beta
MISSING="" LATE="" run env VERIFY_ROOT="$tmp/ws" bash "$script" 9.9.9
grep -c '^https://crates.io/api/v1/crates/' "$tmp/log" >"$tmp/n"
[ "$(cat "$tmp/n")" = 3 ] && grep -qxF "$(crate_url beta)" "$tmp/log" && ok=ok || ok=no
check "$ok" "a two-crate workspace: its two crates and the Rust SDK are the crates.io probes"
fixture alpha beta gamma
MISSING="" LATE="" run env VERIFY_ROOT="$tmp/ws" bash "$script" 9.9.9
grep -qxF "$(crate_url gamma)" "$tmp/log" && ok=ok || ok=no
check "$ok" "a crate added to that workspace is probed too"
printf 'publish = false\n' >>"$tmp/ws/crates/gamma/Cargo.toml"
MISSING="" LATE="" run env VERIFY_ROOT="$tmp/ws" bash "$script" 9.9.9
grep -qxF "$(crate_url gamma)" "$tmp/log" && ok=no || ok=ok
check "$ok" "a crate saying publish = false is not"

# --- a crate that never becomes visible ----------------------------------------------
MISSING="$(crate_url fraiseql-time)" LATE="" run env VERIFY_ROUNDS=3 bash "$script" 9.9.9
rc=$?
[ "$rc" -eq 1 ] && grep -qF "never became visible: crates.io: fraiseql-time v9.9.9" "$tmp/out" && ok=ok || ok=no
check "$ok" "a crate never visible fails the run and is named"
[ "$(grep -cxF "$(crate_url fraiseql-time)" "$tmp/log")" = 3 ] && ok=ok || ok=no
check "$ok" "...after exactly VERIFY_ROUNDS probes of it"
[ "$(grep -cxF "$(crate_url fraiseql-core)" "$tmp/log")" = 1 ] && ok=ok || ok=no
check "$ok" "...while a visible crate is probed once"
grep -qF -- "- crates.io: fraiseql-time v9.9.9" "$tmp/summary" && ok=ok || ok=no
check "$ok" "...and the job summary lists it"

MISSING="https://pypi.org/pypi/fraiseql/9.9.9/json" LATE="" run env VERIFY_ROUNDS=2 bash "$script" 9.9.9
[ $? -eq 1 ] && grep -qF "never became visible: PyPI: fraiseql v9.9.9" "$tmp/out" && ok=ok || ok=no
check "$ok" "a PyPI package never visible fails the run too"

# --- a crate that turns up in a later round --------------------------------------------
MISSING="" LATE="$(crate_url fraiseql-server)" run env VERIFY_ROUNDS=3 bash "$script" 9.9.9
rc=$?
[ "$rc" -eq 0 ] && [ "$(grep -cxF "$(crate_url fraiseql-server)" "$tmp/log")" = 2 ] && ok=ok || ok=no
check "$ok" "a crate visible on the second round passes, after a second probe"

# --- nothing to verify ------------------------------------------------------------------
fixture alpha
printf 'publish = false\n' >>"$tmp/ws/crates/alpha/Cargo.toml"
MISSING="" LATE="" run env VERIFY_ROOT="$tmp/ws" bash "$script" 9.9.9
rc=$?
[ "$rc" -eq 1 ] && [ ! -s "$tmp/log" ] && ok=ok || ok=no
check "$ok" "a workspace publishing no crate fails, and probes nothing"

MISSING="" LATE="" run bash "$script"
[ $? -eq 2 ] && [ ! -s "$tmp/log" ] && ok=ok || ok=no
check "$ok" "no version is a usage error"

echo
echo "$cases cases, $failures failed"
[ "$failures" -eq 0 ] || exit 1
echo "OK: tools/verify-published.sh verifies the workspace's derived crates and fails on any it never sees."
