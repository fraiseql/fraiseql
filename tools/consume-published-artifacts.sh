#!/usr/bin/env bash
# Consume published artifacts the way a stranger consumes them (#1222).
#
# `release.yml`'s `verify-release` asks each registry whether a URL returns HTTP 200 and
# asks the GitHub release whether an asset NAME is present. That is a presence check. It
# cannot see a crate that resolves but does not compile, a wheel that installs but cannot
# import, a package that installs but cannot be required, or a tarball that downloads but
# whose binary will not run — which is the class v2.13.1's crates.io CDN-lag partial
# publish fell into, and the class a `--dry-run` is structurally unable to see.
#
# So this installs, links, imports, extracts and RUNS. Every tier is a real consumption
# from the real registry, in a scratch directory outside the workspace, with nothing from
# this checkout on the path that could satisfy an import by accident.
#
#   crate-build    a fresh crate `cargo add fraiseql@=<v>` + `cargo build`, default and
#                  `--features server` — this transitively compiles most of the 18
#                  published crates from the REGISTRY copy, not the workspace one
#   crate-install  `cargo install fraiseql-cli fraiseql-server --version <v>`, then run
#                  each installed binary
#   pypi           a venv, `pip install fraiseql==<v>`, then `import fraiseql`
#   npm            a scratch package, `npm install fraiseql@<v>`, then require/import it
#   assets         download EVERY published release asset, verify it extracts and carries
#                  the binaries it claims, and RUN the ones this host can execute
#
# ⚠ This is release-time, not pre-merge: it consumes what has been published, so it can
# only run once a version exists. `--version` defaults to the workspace version; pass a
# released one to rehearse (`--version 2.14.1` is how this gate was proved before the
# artifacts it gates existed).
#
# CDN lag is absorbed by a bounded retry whose COUNT AND TOTAL WAIT are logged — a gate
# that silently retried would hide exactly the partial-publish it exists to catch.
#
# Usage:
#   tools/consume-published-artifacts.sh [--version X.Y.Z] [--tier NAME]... [--list]
#
# Exit: 0 all selected tiers consumed; 1 a tier failed; 2 the run could not be set up.
#
# ⚠ Exit 1 and exit 2 are deliberately distinct. A missing toolchain is "cannot answer",
# not "the artifact is broken", and a harness that asked only "non-zero?" would score the
# two alike.

set -uo pipefail

die() { echo "error: $*" >&2; exit 2; }

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || die "not in a git repository"

version=""
list_only=0
declare -a wanted=()

ALL_TIERS=(crate-build crate-install pypi npm assets)

while [ $# -gt 0 ]; do
	case "$1" in
	--version)
		shift
		[ $# -gt 0 ] || die "--version needs a value"
		version="$1"
		;;
	--version=*) version="${1#--version=}" ;;
	--tier)
		shift
		[ $# -gt 0 ] || die "--tier needs a value"
		wanted+=("$1")
		;;
	--tier=*) wanted+=("${1#--tier=}") ;;
	--list) list_only=1 ;;
	-h | --help)
		sed -n '2,42p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*) die "unknown argument '$1' (see --help)" ;;
	esac
	shift
done

if [ -z "$version" ]; then
	version="$(grep -m1 '^version = ' "$repo_root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
	[ -n "$version" ] || die "could not read the workspace version from Cargo.toml"
fi

# Resolve the selection up front, so an unknown --tier fails fast rather than running
# nothing and exiting 0.
declare -a tiers=()
if [ "${#wanted[@]}" -eq 0 ]; then
	tiers=("${ALL_TIERS[@]}")
else
	for w in "${wanted[@]}"; do
		found=0
		for t in "${ALL_TIERS[@]}"; do [ "$t" = "$w" ] && found=1 && break; done
		[ "$found" -eq 1 ] || die "unknown tier '$w' (known: ${ALL_TIERS[*]})"
		tiers+=("$w")
	done
fi

echo "=== consume published artifacts: version=$version tiers=${#tiers[@]}/${#ALL_TIERS[@]} ==="
if [ "${#tiers[@]}" -lt "${#ALL_TIERS[@]}" ]; then
	echo "=== NARROWED: this run does NOT consume every tier ==="
fi

if [ "$list_only" -eq 1 ]; then
	printf '%s\n' "${tiers[@]}"
	exit 0
fi

# Scratch root OUTSIDE the workspace: a build that resolved a path dependency back into
# this checkout would prove nothing about what was published.
work="$(mktemp -d -t fraiseql-consume-XXXXXX)"
trap 'rm -rf "$work"' EXIT
echo "scratch: $work"

# ── CDN-lag retry ────────────────────────────────────────────────────────────
# Bounded, and it LOGS its rounds and total wait. A silent retry would hide the
# partial-publish this gate exists to catch.
RETRY_ROUNDS="${CONSUME_RETRY_ROUNDS:-10}"
RETRY_SLEEP="${CONSUME_RETRY_SLEEP:-15}"

retrying() {
	local what="$1"
	shift
	local waited=0
	for round in $(seq 1 "$RETRY_ROUNDS"); do
		if "$@"; then
			[ "$waited" -gt 0 ] && echo "    (visible after ${round} round(s), ${waited}s of CDN lag)"
			return 0
		fi
		if [ "$round" -lt "$RETRY_ROUNDS" ]; then
			echo "    $what not consumable yet; retry in ${RETRY_SLEEP}s (round $round/$RETRY_ROUNDS)"
			sleep "$RETRY_SLEEP"
			waited=$((waited + RETRY_SLEEP))
		fi
	done
	echo "    $what NEVER became consumable after $RETRY_ROUNDS rounds (${waited}s total)" >&2
	return 1
}

need() { command -v "$1" >/dev/null 2>&1 || die "tier needs '$1' on PATH, and it is absent"; }

# ── Tiers ────────────────────────────────────────────────────────────────────

tier_crate_build() {
	need cargo
	local dir="$work/crate-build"
	cargo new --quiet --bin "$dir" >/dev/null 2>&1 || return 1
	(
		cd "$dir" || exit 1
		retrying "crates.io fraiseql@=$version" cargo add "fraiseql@=$version" --quiet || exit 1
		echo "    building (default features)"
		cargo build --quiet || exit 1
		echo "    building (--features server)"
		cargo add "fraiseql@=$version" --features server --quiet || exit 1
		cargo build --quiet || exit 1
	) || return 1
}

tier_crate_install() {
	need cargo
	local root="$work/cargo-install"
	mkdir -p "$root"
	retrying "crates.io fraiseql-cli/fraiseql-server@$version" \
		cargo install fraiseql-cli fraiseql-server --version "$version" --root "$root" --quiet || return 1
	# ⚠ `fraiseql-cli`, not `fraiseql`. The SAME TOOL has two names depending on how you
	# obtain it: `cargo install fraiseql-cli` gives you `fraiseql-cli` (the crate's
	# `[[bin]] name`), while the release tarballs ship it as `fraiseql` because
	# release.yml:335 copies it under that name when packaging. Do not "correct" either
	# side to match the other — each is right for its own delivery path, and this gate is
	# the only place both are asserted, so this is where the discrepancy is recorded.
	local rc=0
	for bin in fraiseql-cli fraiseql-server; do
		if [ -x "$root/bin/$bin" ]; then
			# Run it. An installed binary that cannot start is the class a presence
			# check cannot see.
			if "$root/bin/$bin" --version >/dev/null 2>&1 || "$root/bin/$bin" --help >/dev/null 2>&1; then
				echo "    ran $bin"
			else
				echo "    $bin installed but neither --version nor --help succeeded" >&2
				rc=1
			fi
		else
			echo "    $bin was not installed" >&2
			rc=1
		fi
	done
	return $rc
}

tier_pypi() {
	need python3
	local dir="$work/pypi"
	mkdir -p "$dir"
	python3 -m venv "$dir/venv" >/dev/null 2>&1 || die "python3 -m venv failed (is python3-venv installed?)"
	retrying "PyPI fraiseql==$version" \
		"$dir/venv/bin/pip" install --quiet --disable-pip-version-check "fraiseql==$version" || return 1
	# Import from a directory with nothing of ours in it, so the workspace copy cannot
	# satisfy the import and make a broken wheel look fine.
	(cd "$dir" && "$dir/venv/bin/python" -c "import fraiseql; print('    imported fraiseql', getattr(fraiseql, '__version__', '(no __version__)'))") || return 1
}

tier_npm() {
	need npm
	local dir="$work/npm"
	mkdir -p "$dir"
	(
		cd "$dir" || exit 1
		npm init -y >/dev/null 2>&1 || exit 1
		retrying "npm fraiseql@$version" npm install --silent --no-audit --no-fund "fraiseql@$version" || exit 1
		# Resolve and load it. `npm install` succeeding says the tarball downloaded;
		# only requiring it says the package has a usable entry point.
		node -e "const m=require('fraiseql'); if(!m) throw new Error('fraiseql required to a falsy value'); console.log('    required fraiseql, ' + Object.keys(m).length + ' export(s)')" \
			|| node --input-type=module -e "import('fraiseql').then(m => console.log('    imported fraiseql (ESM), ' + Object.keys(m).length + ' export(s)'))" \
			|| exit 1
	) || return 1
}

# Every asset release.yml publishes. Kept in the same order as its EXPECTED list; the
# `lint-delivery-coverage` ledger is what holds the two to each other.
ASSETS=(
	fraiseql-x86_64-unknown-linux-gnu.tar.gz
	fraiseql-x86_64-unknown-linux-musl.tar.gz
	fraiseql-aarch64-unknown-linux-gnu.tar.gz
	fraiseql-x86_64-apple-darwin.tar.gz
	fraiseql-aarch64-apple-darwin.tar.gz
	fraiseql-x86_64-pc-windows-msvc.zip
	fraiseql-full-x86_64-unknown-linux-gnu.tar.gz
	fraiseql-full-aarch64-unknown-linux-gnu.tar.gz
	fraiseql-full-x86_64-apple-darwin.tar.gz
	fraiseql-full-aarch64-apple-darwin.tar.gz
	fraiseql-full-x86_64-pc-windows-msvc.zip
)

# Which assets this host can actually execute. The rest are still downloaded and
# extracted and their binaries inspected — "cannot run here" is not "unchecked".
host_runnable() {
	case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) [[ "$1" == *x86_64-unknown-linux-gnu* ]] ;;
	Linux-aarch64) [[ "$1" == *aarch64-unknown-linux-gnu* ]] ;;
	Darwin-x86_64) [[ "$1" == *x86_64-apple-darwin* ]] ;;
	Darwin-arm64) [[ "$1" == *aarch64-apple-darwin* ]] ;;
	*) return 1 ;;
	esac
}

tier_assets() {
	need curl
	need tar
	local tag="v$version"
	local dir="$work/assets"
	mkdir -p "$dir"
	local rc=0 ran=0 extracted=0
	for asset in "${ASSETS[@]}"; do
		local url="https://github.com/fraiseql/fraiseql/releases/download/$tag/$asset"
		local out="$dir/$asset"
		if ! retrying "release asset $asset" curl -fsSL -o "$out" "$url"; then
			echo "    ⚠ $asset is not present on release $tag." >&2
			echo "      At release time this is a MISSING ARTIFACT and the failure is real." >&2
			echo "      When rehearsing against an OLDER version it may simply postdate it —" >&2
			echo "      e.g. fraiseql-full-aarch64-unknown-linux-gnu.tar.gz joined the set in" >&2
			echo "      #649 (2026-08-05), after v2.14.1 (2026-07-24), so v2.14.1 has 10 of the" >&2
			echo "      11 assets this list declares. Check the tag before reading it as a defect." >&2
			rc=1
			continue
		fi
		local into="$dir/x-$asset"
		mkdir -p "$into"
		case "$asset" in
		*.tar.gz) tar -xzf "$out" -C "$into" || { echo "    $asset does not extract" >&2; rc=1; continue; } ;;
		*.zip)
			if command -v unzip >/dev/null 2>&1; then
				unzip -qq "$out" -d "$into" || { echo "    $asset does not extract" >&2; rc=1; continue; }
			else
				echo "    skipping extraction of $asset: no unzip on PATH" >&2
				continue
			fi
			;;
		esac
		extracted=$((extracted + 1))

		# Measured against the published v2.14.1 tarballs, not assumed: a LEAN asset
		# carries `fraiseql` (the CLI) alone; a `-full` asset carries `fraiseql` and
		# `fraiseql-server`. Getting this backwards is exactly the kind of claim a
		# presence check never has to make and so never gets wrong out loud.
		local -a expect=(fraiseql)
		[[ "$asset" == *-full-* ]] && expect+=(fraiseql-server)
		for bin in "${expect[@]}"; do
			local found
			found="$(find "$into" -type f \( -name "$bin" -o -name "$bin.exe" \) | head -1)"
			if [ -z "$found" ]; then
				echo "    $asset does not contain $bin" >&2
				rc=1
				continue
			fi
			if host_runnable "$asset"; then
				chmod +x "$found"
				if "$found" --version >/dev/null 2>&1 || "$found" --help >/dev/null 2>&1; then
					ran=$((ran + 1))
				else
					echo "    $asset: $bin is present but does not run on this host" >&2
					rc=1
				fi
			fi
		done
	done
	echo "    ${#ASSETS[@]} asset(s) declared, $extracted extracted, $ran binary run(s) on $(uname -s)-$(uname -m)"
	# Say what was NOT executed here, rather than letting the tier read as full coverage.
	echo "    (binaries for other platforms are downloaded and inspected, not executed)"
	return $rc
}

# ── Run ──────────────────────────────────────────────────────────────────────

declare -a failed=()
for tier in "${tiers[@]}"; do
	echo
	echo "### tier: $tier"
	start=$SECONDS
	if "tier_${tier//-/_}"; then
		echo "=== TIER-RESULT $tier: OK === ($((SECONDS - start))s)"
	else
		echo "=== TIER-RESULT $tier: FAIL === ($((SECONDS - start))s)"
		failed+=("$tier")
	fi
done

echo
if [ "${#failed[@]}" -gt 0 ]; then
	echo "❌ ${#failed[@]} of ${#tiers[@]} tier(s) FAILED:"
	printf '   - %s\n' "${failed[@]}"
	exit 1
fi
echo "✅ ${#tiers[@]} of ${#ALL_TIERS[@]} tier(s) consumed v$version as a stranger would."
if [ "${#tiers[@]}" -lt "${#ALL_TIERS[@]}" ]; then
	echo "   (narrowed run — $(( ${#ALL_TIERS[@]} - ${#tiers[@]} )) tier(s) were not run)"
fi
