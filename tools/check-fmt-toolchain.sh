#!/usr/bin/env bash
# check-fmt-toolchain.sh — the nightly rustfmt is pinned, and pinned in one place.
#
# Three things run rustfmt: `make fmt` / `make fmt-check` locally, the Dagger
# Fmt gate, and the `rustup toolchain install` that puts the nightly into
# rustBase. When any of them says a bare `nightly`, the formatting the gate
# enforces is whatever nightly published that morning — which is how dev came to
# be red on 2026-09-25 with no commit responsible for it. So this gate asserts:
#
#   1. tools/fmt-toolchain.txt's LAST line names a DATED nightly, not floating
#      `nightly`. Last line, because that is what the Makefile reads (`tail -1`):
#      a `#` cannot appear in a Make `$(shell ...)` without escaping games.
#   2. .dagger/main.go's fmtNightly const is that same value.
#   3. No rustfmt invocation anywhere passes a bare `+nightly`.
#
# Overrides, for testing:
#   FMT_TOOLCHAIN_FILE=path   read the pin from here instead
set -euo pipefail

file="${FMT_TOOLCHAIN_FILE:-tools/fmt-toolchain.txt}"
status=0

if [[ ! -f "$file" ]]; then
	echo "ERROR: $file is missing — the fmt toolchain pin has no source of truth"
	exit 1
fi

pin="$(tail -1 "$file" | tr -d '[:space:]')"

if [[ ! "$pin" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
	echo "ERROR: $file's last line is '$pin', not a dated nightly (nightly-YYYY-MM-DD)."
	echo "       A floating 'nightly' makes the Fmt gate depend on the date CI runs."
	status=1
fi

const="$(sed -n 's/^[[:space:]]*fmtNightly[[:space:]]*=[[:space:]]*"\(.*\)".*/\1/p' .dagger/main.go | head -1)"
if [[ "$const" != "$pin" ]]; then
	echo "ERROR: .dagger/main.go fmtNightly = '$const' but $file says '$pin'."
	echo "       The Dagger gate would enforce a different rustfmt than 'make fmt' applies."
	status=1
fi

# A bare `+nightly` in an invocation. Comment lines are skipped — the Makefile and
# main.go both explain the rule in prose, and prose is not an invocation.
while IFS= read -r hit; do
	echo "ERROR: bare '+nightly' — use the pin from $file: $hit"
	status=1
done < <(grep -rn -- '+nightly[^-]' Makefile .dagger/*.go 2>/dev/null \
	| grep -vE '^[^:]+:[0-9]+:[[:space:]]*(#|//)' || true)

if [[ $status -eq 0 ]]; then
	echo "OK: nightly rustfmt pinned to $pin in $file, .dagger/main.go and the Makefile"
fi
exit $status
