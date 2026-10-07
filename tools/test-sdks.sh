#!/usr/bin/env bash
# Run every official SDK's own test suite and linters: the local `make test-sdks`.
#
#   tools/test-sdks.sh [sdk ...]        # default: all eleven
#
# Each suite is tools/sdk-suite.sh <sdk>, the same script sdk-suites.yml runs in CI.
# An SDK whose toolchain is missing here runs in the container image
# sdks/official/conformance/manifest.json names for it (this box has no ruby, elixir or
# dart, and php without composer). FRAISEQL_SDK_FORCE_CONTAINER="java,php" forces the
# container for SDKs whose host toolchain is present but unusable.
#
# Every SDK is reported by name, and the run fails if any failed OR could not run: a
# skipped suite reads exactly like a passing one (#1346). FRAISEQL_SDK_ALLOW_SKIP lists
# SDKs whose absence is accepted, and the summary still names them.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$root/sdks/official/conformance/manifest.json"
cache="${FRAISEQL_SDK_CACHE:-/srv/bench/tmp-fraiseql/sdk-cache}"

# The commands each suite needs on the host, beyond a shell.
tools_for() {
    case "$1" in
    python) echo uv ;;
    typescript) echo npm npx ;;
    go) echo go ;;
    php) echo php composer ;;
    java) echo mvn javac ;;
    csharp | fsharp) echo dotnet ;;
    elixir) echo mix ;;
    ruby) echo ruby gem ;;
    dart) echo dart ;;
    rust) echo cargo ;;
    *) return 1 ;;
    esac
}

image_for() {
    python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sdks"][sys.argv[2]].get("container") or "")' \
        "$manifest" "$1"
}

listed() { [[ ",${2:-}," == *",$1,"* ]]; }

if [ $# -gt 0 ]; then
    sdks=("$@")
else
    mapfile -t sdks < <(python3 -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1]))["sdks"]))' "$manifest")
fi

declare -A verdict
for sdk in "${sdks[@]}"; do
    tools="$(tools_for "$sdk")" || { echo "unknown SDK: $sdk" >&2; verdict[$sdk]="unknown"; continue; }
    native=1
    for t in $tools; do command -v "$t" >/dev/null || native=0; done
    listed "$sdk" "${FRAISEQL_SDK_FORCE_CONTAINER:-}" && native=0

    echo
    echo "=== $sdk ==="
    if [ "$native" = 1 ]; then
        bash "$root/tools/sdk-suite.sh" "$sdk"
        rc=$?
        how="native"
    else
        image="$(image_for "$sdk")"
        if [ -z "$image" ] || ! command -v docker >/dev/null; then
            echo "SKIP $sdk: needs $tools and no container fallback is available"
            verdict[$sdk]="skipped (no $tools)"
            continue
        fi
        mkdir -p "$cache/$sdk"
        # Root inside so a bash-less image can install one; ownership of everything the
        # suite wrote goes back to the caller, so no root-owned build output is left in
        # the tree to break the next host build.
        docker run --rm -v "$root:/src" -v "$cache/$sdk:/sdk-home" -e HOME=/sdk-home -w /src \
            "$image" sh -c '
                command -v bash >/dev/null || { apk add --no-cache bash >/dev/null 2>&1 || (apt-get update -qq && apt-get install -y -qq bash >/dev/null); }
                bash tools/sdk-suite.sh "$1"; rc=$?
                chown -R "$2" /src/sdks/official /sdk-home 2>/dev/null
                exit $rc' _ "$sdk" "$(id -u):$(id -g)"
        rc=$?
        how="container $image"
    fi
    if [ "$rc" = 0 ]; then verdict[$sdk]="passed ($how)"; else verdict[$sdk]="FAILED ($how, rc=$rc)"; fi
done

echo
echo "=== SDK suites ==="
status=0
for sdk in "${sdks[@]}"; do
    v="${verdict[$sdk]:-not run}"
    printf '  %-11s %s\n' "$sdk" "$v"
    case "$v" in
    passed*) ;;
    skipped*) listed "$sdk" "${FRAISEQL_SDK_ALLOW_SKIP:-}" || status=1 ;;
    *) status=1 ;;
    esac
done
exit $status
