#!/usr/bin/env bash
# check-r-examples-parse.sh — every R file under examples/ parses.
#
# Background (#1260): `examples/r/fraiseql_client.R` is shipped, documented and
# advertised in `examples/README.md`, and nothing in this repository ran it,
# linted it, or parsed it. There was no R toolchain in any leg and no `Rscript`
# anywhere in the tree:
#
#   * `check-examples-compile.sh` compiles example *authoring artifacts*
#     (schema.json / fraiseql.toml), not R;
#   * `check-examples-integrity.sh` is a static gate with no toolchain;
#   * `check-example-crates-compile.sh` (#1200) covers standalone Cargo projects.
#
# #1200 rewrote this client to perform the Flight handshake and attach the
# `authorization` header — on a machine with no R installed. So the rewrite went
# out unverified in precisely the dimension an editor who cannot run R is most
# likely to break, and the file sat in the same position `examples/rust/
# flight_client` was in before that gate: something a reader is invited to
# `source()` that could stop working, or never have worked, with nothing to
# notice.
#
# What this does NOT establish: parsing is not running. It says nothing about
# whether the handshake works, whether the header is accepted, or whether the
# result decodes — that is Level 3 in #1260 and needs a live Flight server with a
# real OIDC validator. What it buys is that the class of defect this file was
# most exposed to can no longer land silently.
#
# Discovery is at runtime, never a hard-coded list: a new R example is covered the
# day it lands, and an empty discovery is a failure rather than a silent pass.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

SCAN_ROOT="${R_EXAMPLES_SCAN_ROOT:-examples}"
R_IMAGE="${R_IMAGE:-r-base:4.4.1}"

if [ ! -d "$SCAN_ROOT" ]; then
    echo "✗ scan root '$SCAN_ROOT' is not a directory."
    exit 1
fi

scan_abs="$(cd "$SCAN_ROOT" && pwd)"

# Relative to the scan root, so the same list works whether it is parsed on this
# machine or inside a container that has the scan root mounted.
mapfile -t files < <(cd "$scan_abs" && find . -iname '*.R' -not -path '*/.*' | sed 's|^\./||' | sort)

if [ "${#files[@]}" -eq 0 ]; then
    echo "✗ found no R file under '$SCAN_ROOT' at all."
    echo "    Either the search is wrong, or the R examples are gone and this gate"
    echo "    is now checking nothing. Delete it, or fix the discovery — do not"
    echo "    leave it passing over an empty set."
    exit 1
fi

# One Rscript invocation over every file: reports each, then the count, so a run
# that silently checked fewer files than it found is visible in the output.
read -r -d '' R_CODE <<'RCODE' || true
files <- commandArgs(trailingOnly = TRUE)
bad <- 0L
for (f in files) {
  ok <- tryCatch({
    invisible(parse(f))
    TRUE
  }, error = function(e) {
    cat("  FAIL ", f, ": ", conditionMessage(e), "\n", sep = "")
    FALSE
  })
  if (isTRUE(ok)) cat("  ok   ", f, "\n", sep = "")
  else bad <- bad + 1L
}
cat("parsed ", length(files) - bad, " of ", length(files), " file(s)\n", sep = "")
quit(status = if (bad > 0L) 1L else 0L)
RCODE

echo "→ parsing ${#files[@]} R file(s) under '$SCAN_ROOT'"

# Rscript directly when the toolchain is here — which is the case inside the CI
# container, where the leg apt-installs r-base-core. Otherwise a pinned r-base
# image, because most developer machines have Docker and no R. This is also why
# the gate is a sibling Dagger function rather than a ShellGates command: that
# container is deliberately toolchain-free, and shelling out to Docker from
# inside the Dagger engine is not available to it.
#
# It does not skip. A gate that passes when it could not run is worse than no
# gate: it reports the file as checked.
if command -v Rscript >/dev/null 2>&1; then
    ( cd "$scan_abs" && Rscript -e "$R_CODE" "${files[@]}" )
elif command -v docker >/dev/null 2>&1; then
    echo "  (no Rscript on PATH — parsing in $R_IMAGE)"
    docker run --rm -v "$scan_abs:/scan:ro" -w /scan "$R_IMAGE" \
        Rscript -e "$R_CODE" "${files[@]}"
else
    echo "✗ neither Rscript nor docker is available. This gate parses; it does not skip."
    echo "    Install R (r-base-core), or Docker so it can use $R_IMAGE."
    exit 1
fi

echo "OK: all ${#files[@]} R example file(s) parse."
