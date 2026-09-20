#!/usr/bin/env bash
# check-ci-install-pins.sh — fail when CI installs a third-party tool without naming
# a version.
#
# WHY THIS GATE EXISTS
# --------------------
# On 2026-09-20 the `integration (saml)` leg went red twice in nine hours on a branch
# nobody touched. `.dagger/main.go` installed the #946 SCIM conformance client with
# `pip install scim2-tester httpx`, unpinned, so the gate's verdict tracked PyPI rather
# than the tree: scim2-models 0.8.0 (07:10Z) removed a method scim2-client called, and
# scim2-client 0.8.0 (15:29Z) renamed its exception hierarchy out from under
# scim2-tester, which reported a conformant DELETE→204→GET→404 sequence as
# `object_deletion: User not found`. Two reds, two upstream releases, zero commits.
#
# A build that can go red because someone else shipped is a build whose reds get
# discounted, which is the failure mode that matters — the next real red is read as
# "upstream again". So: every install in CI names a version.
#
# WHAT COUNTS AS A PIN
# --------------------
#   pip     every package argument carries `==`, or the command installs from `-r <file>`
#   cargo   `--version` is present, or `--path` (a local build of this workspace), or
#           `--git` with `--rev` (a commit is a version)
# A shell variable is a pin: `cargo install fraiseql-cli --version "$version"` names
# exactly one version, chosen by the caller rather than by the registry's clock.
#
# WHAT IT DELIBERATELY DOES NOT REACH
# -----------------------------------
# * Lines inside a fenced ``` block. `sbom-generation.yml` prints installation
#   *instructions* into its report; that text is documentation for a human, not a step
#   this repository runs, and pinning it would publish a version that ages.
# * `npm`/`go install`: `go install` already requires an `@version` suffix by
#   construction, and the two npm installs in tools/ name `fraiseql@$version`. Stated
#   so a reader does not read silence as coverage — add them here if that changes.
#
# Exemptions live in EXEMPT below, each with the reason it is not a defect, and each
# must still match something: a stale entry fails the run, so a pin that lands later
# cannot leave a permanent hole behind it.
#
# Pure grep/sed over files found with `find`, no toolchain, no git history, no
# `git ls-files` (ShellGates runs `git init -q .` in a tree with no commits, where that
# would return nothing and the gate would pass vacuously) → Dagger ShellGates.
#
# Overrides, for testing:
#   CI_INSTALL_PINS_ROOT=<dir>   treat this directory as the repo root
set -euo pipefail

if [ -n "${CI_INSTALL_PINS_ROOT:-}" ]; then
  cd "$CI_INSTALL_PINS_ROOT"
elif repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$repo_root"
fi

# Each entry: <substring of the offending line>|<reason>.
# Matched against the whole line, so an entry is as narrow as the text it quotes.
EXEMPT=(
  "pip install --upgrade pip|the installer bootstrap, not a dependency. Every package this repository installs carries its own \`==\`, so the resolver's choice is already determined; pinning pip itself would freeze the resolver instead."
)

violations=0
matched_exemptions=()

is_exempt() {
  local line="$1" i=0
  for entry in "${EXEMPT[@]}"; do
    local needle="${entry%%|*}"
    if [[ "$line" == *"$needle"* ]]; then
      matched_exemptions+=("$i")
      return 0
    fi
    i=$((i + 1))
  done
  return 1
}

report() {
  printf 'check-ci-install-pins: %s:%s installs without naming a version\n' "$1" "$2" >&2
  printf '    %s\n' "$3" >&2
  violations=$((violations + 1))
}

scanned=0
while IFS= read -r file; do
  scanned=$((scanned + 1))
  in_fence=0
  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    # A fenced block is documentation printed by the workflow, not a step it runs.
    case "$line" in
      *'```'*) in_fence=$((1 - in_fence)); continue ;;
    esac
    [ "$in_fence" -eq 1 ] && continue
    # A commented-out command is not a step this repository runs.
    trimmed="${line#"${line%%[![:space:]]*}"}"
    case "$trimmed" in
      '#'*|'//'*|'///'*|'*'*) continue ;;
    esac

    if [[ "$line" == *"pip install"* || "$line" == *"pip3 install"* ]]; then
      is_exempt "$line" && continue
      # `-r <file>` is a pin set; `==` is a pin.
      if [[ "$line" != *"=="* && "$line" != *" -r "* && "$line" != *"--requirement"* ]]; then
        report "$file" "$lineno" "$trimmed"
      fi
    fi

    if [[ "$line" == *"cargo install"* ]]; then
      is_exempt "$line" && continue
      if [[ "$line" != *"--version"* && "$line" != *"--path"* && "$line" != *"--rev"* ]]; then
        report "$file" "$lineno" "$trimmed"
      fi
    fi
  done < "$file"
done < <(
  # This script and its self-test quote the very commands they search for, so they are
  # out of scope by name: a gate that matches its own source reports itself and nothing
  # else. Excluded by path rather than by a pattern, so the exclusion cannot widen.
  find .github/workflows .dagger tools Makefile \
    \( -name '*.yml' -o -name '*.yaml' -o -name '*.go' -o -name '*.sh' -o -name 'Makefile' \) \
    -type f \
    ! -name 'check-ci-install-pins.sh' \
    ! -name 'ci_install_pins_test.sh' \
    2>/dev/null | sort
)

if [ "$scanned" -eq 0 ]; then
  echo "check-ci-install-pins: no files in scope — refusing to report success" >&2
  exit 1
fi

# An exemption that stops triggering is a hole nobody is watching any more.
stale=0
for i in "${!EXEMPT[@]}"; do
  hit=0
  for m in ${matched_exemptions[@]+"${matched_exemptions[@]}"}; do
    [ "$m" = "$i" ] && hit=1
  done
  if [ "$hit" -eq 0 ]; then
    printf 'check-ci-install-pins: exemption %q never matched — remove it\n' "${EXEMPT[$i]%%|*}" >&2
    stale=$((stale + 1))
  fi
done

if [ "$violations" -gt 0 ] || [ "$stale" -gt 0 ]; then
  cat >&2 <<'EOF'

    Pin it. For pip, `pkg==X.Y.Z` (or a requirements file, as
    tools/scim-conformance-requirements.txt does for the SCIM conformance client).
    For cargo, `--version X.Y.Z --locked`.

    If the install genuinely must float, add it to EXEMPT in this script with the
    reason — and expect the next reader to ask why a CI verdict is allowed to depend
    on a release nobody here made.
EOF
  exit 1
fi

echo "OK: every pip/cargo install in .github/workflows, .dagger, tools and the Makefile" \
     "names a version (${scanned} files scanned, ${#EXEMPT[@]} exemption(s), all live)."
