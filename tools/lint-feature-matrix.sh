#!/usr/bin/env bash
# Run the feature-check matrix natively, exactly as `Dagger — feature matrix` runs it.
#
# Why this exists (#1227): the matrix leg is `push: branches: [dev]`, so it cannot gate
# a branch, and `make preflight` is structurally unable to find what it finds —
# preflight's clippy pass is `--all-features` (where a feature-OFF arm is not compiled
# at all) and its narrow-feature pass is `cargo check`, which runs no clippy lints.
# Their intersection — clippy under anything other than `--all-features` — had no gate
# anywhere before a push. `94e7b5558` went to `dev` with `make preflight` exit 0 and
# reddened 4 of 47 combos on one `clippy::collection_is_never_read`.
#
# The combo list is NOT duplicated here. It is derived from `.dagger/feature-combos.go`
# by tools/feature-combos.py, which refuses to emit a short list: a literal it cannot
# parse, or a field it does not model, is a hard error. So this runner cannot silently
# cover fewer combos than the leg declares — the property the issue asked for.
#
# Usage:
#   tools/lint-feature-matrix.sh                     # all declared combos (the default)
#   tools/lint-feature-matrix.sh --clippy-only       # only the combos the leg clippies
#   tools/lint-feature-matrix.sh --combo=NAME [...]  # named combos
#   tools/lint-feature-matrix.sh --list              # print the derived invocations
#
# Any narrowing is stated on stdout, every run, next to the declared total — a subset
# run is never mistaken for a full one.
#
# Cost: a cold run compiles 47 feature sets and is slow. Warm runs are cheap because
# cargo fingerprints each feature set separately. This is deliberately NOT in
# `make preflight`: a target that takes an hour is a target nobody runs, which would
# weaken every other gate preflight carries. Run it before pushing anything under a
# `#[cfg(feature = …)]`.

set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"

selection="all"
list_only=0
declare -a wanted=()

for arg in "$@"; do
	case "$arg" in
	--list) list_only=1 ;;
	--clippy-only) selection="clippy" ;;
	--combo=*)
		selection="named"
		wanted+=("${arg#--combo=}")
		;;
	-h | --help)
		sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "error: unknown argument '$arg' (see --help)" >&2
		exit 2
		;;
	esac
done

# Derive the matrix. A parse failure is fatal: running a subset we did not intend is
# exactly the failure mode this gate exists to prevent.
if ! derived="$(python3 tools/feature-combos.py)"; then
	echo "error: could not derive the feature matrix from .dagger/feature-combos.go" >&2
	exit 2
fi

declare -a names=() cmds=()
while IFS=$'\t' read -r name cmd; do
	[ -n "$name" ] || continue
	names+=("$name")
	cmds+=("$cmd")
done <<<"$derived"

declared=${#names[@]}
if [ "$declared" -eq 0 ]; then
	echo "error: derived 0 combos — refusing to report a green matrix" >&2
	exit 2
fi

# Resolve the selection to indices, so an unknown --combo= name fails fast with the
# known list rather than running nothing and exiting 0.
declare -a selected=()
case "$selection" in
all)
	for i in "${!names[@]}"; do selected+=("$i"); done
	;;
clippy)
	for i in "${!names[@]}"; do
		case "${cmds[$i]}" in *"cargo clippy "*) selected+=("$i") ;; esac
	done
	;;
named)
	for want in "${wanted[@]}"; do
		found=0
		for i in "${!names[@]}"; do
			if [ "${names[$i]}" = "$want" ]; then
				selected+=("$i")
				found=1
				break
			fi
		done
		if [ "$found" -eq 0 ]; then
			echo "error: unknown combo '$want'" >&2
			printf 'known: %s\n' "$(
				IFS=,
				echo "${names[*]}"
			)" >&2
			exit 2
		fi
	done
	;;
esac

echo "=== feature matrix: declared=$declared selected=${#selected[@]} (selection: $selection) ==="
if [ "${#selected[@]}" -lt "$declared" ]; then
	echo "=== NARROWED: this run does NOT cover the whole matrix ==="
fi
echo "### toolchain: $(rustc --version)"

if [ "$list_only" -eq 1 ]; then
	for i in "${selected[@]}"; do
		printf '%s\t%s\n' "${names[$i]}" "${cmds[$i]}"
	done
	exit 0
fi

# fail-fast is OFF, matching the leg's `fail-fast: false`: one run reports the whole
# matrix rather than stopping at the first bad combo.
declare -a failed=()
for i in "${selected[@]}"; do
	name="${names[$i]}"
	cmd="${cmds[$i]}"
	echo
	echo "### feature-check: $name"
	echo "### $cmd"
	start=$SECONDS
	# `< /dev/null` so a child can never consume this script's stdin, and `|| rc=$?`
	# rather than `set -e` so the loop observes the failure instead of aborting on it.
	rc=0
	# shellcheck disable=SC2086  # cmd is a derived argv with no shell metacharacters
	$cmd </dev/null || rc=$?
	elapsed=$((SECONDS - start))
	if [ "$rc" -eq 0 ]; then
		echo "=== COMBO-RESULT $name: OK === (${elapsed}s)"
	else
		echo "=== COMBO-RESULT $name: FAIL === (${elapsed}s, exit $rc)"
		failed+=("$name")
	fi
done

echo
if [ "${#failed[@]}" -gt 0 ]; then
	echo "❌ ${#failed[@]} of ${#selected[@]} combo(s) FAILED:"
	printf '   - %s\n' "${failed[@]}"
	exit 1
fi

echo "✅ ${#selected[@]} of $declared declared combo(s) passed."
if [ "${#selected[@]}" -lt "$declared" ]; then
	echo "   (narrowed run — the remaining $((declared - ${#selected[@]})) were not built)"
fi
