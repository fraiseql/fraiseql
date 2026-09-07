#!/usr/bin/env bash
# Diff tools/required-checks.toml against the live `dev` branch ruleset (#1289).
#
# Run:  make lint-required-checks     (or: bash tools/check-required-checks.sh)
#
# `tools/required-checks.toml` is what lets `tools/check-suite-coverage.py` refuse a
# test suite whose only coverage is a leg that cannot fail a merge. That file is a
# MIRROR: a GitHub ruleset lives in GitHub, and the offline Dagger legs have neither
# a network nor a token, so nothing inside `preflight` can read the authority. A
# mirror nobody diffs is a claim, and a claim about a merge gate is the dangerous
# kind — the gate would report protection that does not exist.
#
# So this lives outside preflight deliberately, and has to be run by hand when the
# mirror or the ruleset changes, and before cutting a release. That residual is
# recorded rather than hidden.
#
# ⚠ Exit codes are split so a harness cannot read "could not run" as "found
# nothing":
#   0  the mirror and the ruleset agree
#   1  they disagree — the finding is printed
#   2  the check could not run (no gh, not authenticated, API refused)
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIRROR="$REPO_ROOT/tools/required-checks.toml"
SLUG="${FRAISEQL_REPO:-fraiseql/fraiseql}"

die_cannot_run() {
    echo "required-checks: CANNOT RUN: $1" >&2
    exit 2
}

command -v gh >/dev/null 2>&1 || die_cannot_run "the \`gh\` CLI is not on PATH"
[ -f "$MIRROR" ] || die_cannot_run "$MIRROR is missing"

# The ruleset id and branch are declared beside the list they describe, so this
# script never has to be told which ruleset it is checking.
read -r RULESET BRANCH <<EOF
$(python3 - "$MIRROR" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    data = tomllib.load(f)
print(data.get("ruleset", ""), data.get("branch", ""))
PY
)
EOF
[ -n "$RULESET" ] && [ -n "$BRANCH" ] || die_cannot_run \
    "required-checks.toml declares no \`ruleset\`/\`branch\` — the script cannot know what to diff"

LIVE_JSON="$(mktemp)"
trap 'rm -f "$LIVE_JSON"' EXIT
if ! gh api "repos/$SLUG/rulesets/$RULESET" >"$LIVE_JSON" 2>"$LIVE_JSON.err"; then
    die_cannot_run "gh api repos/$SLUG/rulesets/$RULESET failed (the token may lack
repository-administration read): $(cat "$LIVE_JSON.err")"
fi

python3 - "$MIRROR" "$BRANCH" "$LIVE_JSON" <<'PY'
import json, sys, tomllib

mirror_path, branch, live_path = sys.argv[1], sys.argv[2], sys.argv[3]
with open(live_path, encoding="utf-8") as f:
    live = json.load(f)
with open(mirror_path, "rb") as f:
    declared = [str(c) for c in tomllib.load(f).get("required", [])]

# The ruleset must actually target the branch the mirror names. A ruleset that
# stopped applying to `dev` would otherwise diff clean while gating nothing —
# the same "runs but does not gate" shape one level up.
includes = ((live.get("conditions") or {}).get("ref_name") or {}).get("include") or []
if f"refs/heads/{branch}" not in includes and "~ALL" not in includes:
    print(f"required-checks: FAIL — ruleset {live.get('id')} does not target refs/heads/{branch}; "
          f"it targets {includes}")
    sys.exit(1)

if live.get("enforcement") != "active":
    print(f"required-checks: FAIL — ruleset {live.get('id')} enforcement is "
          f"{live.get('enforcement')!r}, so nothing it lists is required")
    sys.exit(1)

actual = []
for rule in live.get("rules", []):
    if rule.get("type") == "required_status_checks":
        actual += [c["context"] for c in rule["parameters"]["required_status_checks"]]

missing = sorted(set(declared) - set(actual))   # mirror claims a gate GitHub does not enforce
extra = sorted(set(actual) - set(declared))     # GitHub enforces a gate the tree does not know

if missing or extra:
    print(f"required-checks: FAIL — tools/required-checks.toml and ruleset {live.get('id')} disagree.")
    for c in missing:
        print(f"  ✗ declared but NOT required on {branch}: {c!r}")
        print("      the suite-coverage gate is reporting a merge gate that does not exist")
    for c in extra:
        print(f"  ✗ required on {branch} but NOT declared: {c!r}")
        print("      the suite-coverage gate is under-counting what protects the branch")
    sys.exit(1)

print(f"required-checks: OK — {len(declared)} contexts, mirror and ruleset {live.get('id')} agree.")
PY
