#!/usr/bin/env bash
# Red-capability pin for tools/check-delivery-coverage.py.
#
# Run directly:  bash tools/tests/delivery_coverage_test.sh
# Exits non-zero if any assertion fails.
#
# The gate's claim is that a delivery artifact cannot arrive ungated and a ledger row
# cannot outlive its artifact. Every mutation below is a way that claim could be false
# while the gate still printed OK — which is the only interesting question about a
# coverage gate, since a coverage gate that cannot go red is indistinguishable from a
# `true` with good prose.
#
# Two of them are load-bearing beyond the rest:
#
#   * D1 removes the `dagger call image-boots` RUN step while leaving the workflow's
#     prose comment that narrates the same command. A grep-based gate passes that
#     mutation. Reading parsed `run:` steps is the whole reason this gate can say a leg
#     is wired into CI rather than merely mentioned near it.
#   * S2 gives an exempted artifact an executing leg and requires the gate to go RED on
#     the now-stale exemption. Without it the exemptions are write-only: they would
#     outlive the gaps they describe and keep excusing coverage that had arrived, which
#     is the failure mode #1055 and #883 are both instances of.
#
# The fixture is assembled with `find`, never `git ls-files` — this test runs inside the
# Dagger ShellGates container, where the repository is `git init`ed with an empty index
# and `git ls-files` returns nothing at all.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A fixture repo carrying only what the gate reads: the four sibling parsers it imports,
# the Go module table, every workflow, every Cargo.toml (crate discovery walks the
# workspace members), the chart, the two deploy test scripts and the SDK directory names.
make_fixture() {
    local dir="$1"
    mkdir -p "$dir/tools/tests" "$dir/.dagger" "$dir/.github/workflows"

    cp "$REPO_ROOT/tools/check-delivery-coverage.py" \
       "$REPO_ROOT/tools/check-image-parity.py" \
       "$REPO_ROOT/tools/check-publish-parity.py" \
       "$REPO_ROOT/tools/check-suite-coverage.py" \
       "$REPO_ROOT/tools/delivery-artifacts.toml" \
       "$REPO_ROOT/tools/compose-stack-test.sh" \
       "$REPO_ROOT/tools/chart-deploy-test.sh" \
       "$REPO_ROOT/tools/consume-published-artifacts.sh" \
       "$dir/tools/"
    cp "$REPO_ROOT"/.dagger/*.go "$dir/.dagger/"
    cp "$REPO_ROOT"/.github/workflows/*.yml "$dir/.github/workflows/"

    # Every Cargo.toml, paths preserved. Two constraints decided this loop, and both
    # are properties of the ShellGates container rather than preferences: `find` not
    # `git ls-files`, which returns nothing there (see header); and cp into a mkdir'd
    # path rather than `cpio -pdm`, because shellBase installs exactly make, git, gawk,
    # findutils, grep, ca-certificates and python3, and cpio is not among them —
    # measured MISSING in ghcr.io/fraiseql/ubuntu:24.04, so the first draft of this file
    # would have failed in CI while passing on every developer machine. `tar` and `perl`
    # do happen to be present as Ubuntu essential packages, but leaning on either is
    # leaning on a base-image detail rather than a declared dependency, so the mutations
    # below use only sed and python3.
    ( cd "$REPO_ROOT" && find . -name Cargo.toml -not -path './target/*' -not -path './.git/*' -print0 ) \
        | while IFS= read -r -d '' f; do
              mkdir -p "$dir/$(dirname "$f")"
              cp "$REPO_ROOT/$f" "$dir/$f"
          done

    mkdir -p "$dir/deploy/kubernetes/helm/fraiseql"
    cp "$REPO_ROOT/deploy/kubernetes/helm/fraiseql/Chart.yaml" "$dir/deploy/kubernetes/helm/fraiseql/"

    # Discovery reads directory NAMES here, so empty directories are faithful.
    ( cd "$REPO_ROOT/sdks/official" && find . -maxdepth 1 -mindepth 1 -type d -printf '%f\0' ) \
        | xargs -0 -I{} mkdir -p "$dir/sdks/official/{}"
    ( cd "$REPO_ROOT/sdks/community" && find . -maxdepth 1 -mindepth 1 -type d -printf '%f\0' ) \
        | xargs -0 -I{} mkdir -p "$dir/sdks/community/{}"
}

# expect <pass|fail> <case-id> <description> <mutation...>
expect() {
    local want="$1" id="$2" desc="$3"; shift 3
    local dir="$WORK/$id" got
    TESTS_RUN=$((TESTS_RUN + 1))
    make_fixture "$dir"
    ( cd "$dir" && "$@" ) || {
        printf '  ❌ %-4s %s — the mutation itself failed\n' "$id" "$desc"
        TESTS_FAILED=$((TESTS_FAILED + 1)); return
    }

    # Exit 1 is a FINDING; exit 2 is the gate's own FATAL (an unreadable ledger or a
    # missing discovery source). They are distinguished deliberately: a mutation that
    # happened to corrupt the manifest would exit 2, and a harness that only asked
    # "non-zero?" would score that as a successful red proof for an assertion it never
    # reached. E1 did exactly that on the first run of this file.
    # `|| rc=$?` rather than a bare call: `set -e` would abort this script on the very
    # non-zero exit every red proof below is trying to observe.
    local rc=0
    ( cd "$dir" && python3 tools/check-delivery-coverage.py >"$dir/.out" 2>&1 ) || rc=$?
    case "$rc" in
        0) got=pass ;;
        1) got=fail ;;
        *) got=fatal ;;
    esac

    if [ "$got" = "$want" ]; then
        printf '  ✅ %-4s %s\n' "$id" "$desc"
    else
        printf '  ❌ %-4s %s — expected %s, got %s\n' "$id" "$desc" "$want" "$got"
        sed 's/^/        /' "$dir/.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

noop() { :; }

echo "delivery-coverage gate self-test"
echo

echo "── the unmutated tree passes (or every assertion below is vacuous) ──"
expect pass P0 "repository as it stands" noop

echo
echo "── discovery ↔ ledger, both directions ──"
expect fail A1 "a new image variant ships with no ledger row" \
    sed -i 's|{name: "tutorial", dockerfile: "tutorial/Dockerfile", buildContext: ".", buildArgs: "", optional: true},|{name: "tutorial", dockerfile: "tutorial/Dockerfile", buildContext: ".", buildArgs: "", optional: true},\n\t{name: "sidecar", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "", optional: true},|' .dagger/image.go

expect fail A2 "a ledger row whose artifact stopped shipping" \
    sed -i '/name: "tutorial", dockerfile: "tutorial\/Dockerfile"/d' .dagger/image.go

expect fail A3 "an SDK directory arrives with no row" \
    mkdir -p sdks/official/fraiseql-zig

echo
echo "── a leg named by the ledger must exist ──"
expect fail B1 "a workflow job that does not exist" \
    sed -i 's|"workflow:docker-build.yml:build-and-push"|"workflow:docker-build.yml:build-and-shove"|' tools/delivery-artifacts.toml

expect fail B2 "a workflow file that does not exist" \
    sed -i 's|"workflow:docker-build.yml:build-and-push"|"workflow:docker-shove.yml:build-and-push"|' tools/delivery-artifacts.toml

expect fail B3 "a Dagger function that does not exist" \
    sed -i 's|"dagger:ImageBoots"|"dagger:ImageBoosts"|' tools/delivery-artifacts.toml

expect fail B4 "an unknown leg prefix" \
    sed -i 's|"dagger:ImageBoots"|"jenkins:ImageBoots"|' tools/delivery-artifacts.toml

echo
echo "── …and must be wired into CI, not merely exist ──"
# PublishDryRun is real, working, and called by no workflow — only by
# `make release-validate` on a developer's machine. A ledger that accepted it would
# record CI coverage that CI does not perform.
expect fail C1 "a real Dagger function that no workflow calls (PublishDryRun)" \
    sed -i 's|"dagger:ImageBoots"|"dagger:PublishDryRun"|' tools/delivery-artifacts.toml

expect fail C2 "a real tool script that no workflow invokes" \
    sed -i 's|"tool:tools/compose-stack-test.sh"|"tool:tools/check-image-parity.py"|' tools/delivery-artifacts.toml

echo
echo "── the load-bearing one: a comment is not an invocation ──"
# Delete the `dagger call image-boots` RUN step, leaving dagger-image.yml's prose header
# — which narrates the same command — untouched. A grep over the file still finds the
# string. The gate must not.
expect fail D1 "the run: step is deleted while the comment naming it remains" \
    sed -i '/^          dagger call image-boots --source=. \\$/,+1d' .github/workflows/dagger-image.yml

echo
echo "── executes is the gated column ──"
#
# ⚠ These four used to anchor on `applies_to = "image:tutorial"`. When #1221 gave that
# image a real boot tier its exemption was deleted, every mutation became a no-op, and
# four assertions passed while testing NOTHING — the exact failure this file exists to
# catch, in the file itself. (S4 below had the same fault, on the `crate:*` wildcard #1222
# removed.) So none of them names a ledger row any more: each DISCOVERS its target at
# runtime, and fails loudly if the ledger has none to offer rather than silently doing
# nothing. A mutation that cannot find its target is not a passing test.
mutate() {
    python3 - "$1" <<'PYEOF'
import pathlib, re, sys
mode = sys.argv[1]
p = pathlib.Path("tools/delivery-artifacts.toml")
t = p.read_text()

def first_exempt_block():
    m = re.search(r'\[\[exempt\]\]\n(?:[a-z_]+ = [^\n]*\n)+', t)
    if not m:
        raise SystemExit("FIXTURE ERROR: the ledger has no [[exempt]] block to mutate")
    return m

if mode == "drop-exemption":
    # E1: an artifact loses its exemption while still having no executing leg.
    m = first_exempt_block()
    aid = re.search(r'applies_to = "([^"]+)"', m.group(0)).group(1)
    if "*" in aid:
        raise SystemExit(f"FIXTURE ERROR: first exemption {aid!r} is a wildcard; want a named one")
    t = t[: m.start()] + t[m.end() :]

elif mode == "stale-exemption":
    # S1: an exemption that matches no artifact at all.
    m = first_exempt_block()
    aid = re.search(r'applies_to = "([^"]+)"', m.group(0)).group(1)
    t = t.replace(f'applies_to = "{aid}"', f'applies_to = "{aid}-no-such-artifact"', 1)

elif mode == "exemption-gains-leg":
    # S2: the exempted artifact gains real coverage, so the exemption is now a lie.
    m = first_exempt_block()
    aid = re.search(r'applies_to = "([^"]+)"', m.group(0)).group(1)
    if "*" in aid:
        raise SystemExit(f"FIXTURE ERROR: first exemption {aid!r} is a wildcard; want a named one")
    blocks = t.split("[[artifact]]")
    hit = False
    for i, b in enumerate(blocks):
        if re.search(rf'^id = "{re.escape(aid)}"$', b, re.M):
            em = re.search(r'^executes = \[\]$', b, re.M)
            if em:
                blocks[i] = b[: em.start()] + 'executes = ["dagger:Images"]' + b[em.end() :]
                hit = True
    if not hit:
        raise SystemExit(f"FIXTURE ERROR: no artifact row {aid!r} with an empty executes")
    t = "[[artifact]]".join(blocks)

elif mode == "duplicate-exemption":
    # S4: a SECOND exemption matching an artifact that already has one.
    m = first_exempt_block()
    aid = re.search(r'applies_to = "([^"]+)"', m.group(0)).group(1)
    t = t.rstrip("\n") + (
        f'\n\n[[exempt]]\napplies_to = "{aid}"\nissue = 1222\n'
        'reason = "a second exemption for an artifact that already has one"\n'
    )

elif mode == "exemption-without-issue":
    # S3: an exemption carrying no issue number.
    m = first_exempt_block()
    body = re.sub(r'^issue = \d+\n', "", m.group(0), count=1, flags=re.M)
    if body == m.group(0):
        raise SystemExit("FIXTURE ERROR: first exemption has no `issue =` line to remove")
    t = t[: m.start()] + body + t[m.end() :]

else:
    raise SystemExit(f"unknown mutation {mode!r}")

p.write_text(t)
PYEOF
}

expect fail E1 "an artifact with no executing leg and no exemption" mutate drop-exemption

echo
echo "── exemptions, and the self-clearing half ──"
expect fail S1 "an exemption matching no artifact (stale)" mutate stale-exemption

# The exempted artifact gains real coverage. The exemption is now a lie, and nothing
# except this rule would ever notice.
expect fail S2 "an exempted artifact gains an executing leg — the exemption is now stale" \
    mutate exemption-gains-leg

expect fail S3 "an exemption with no issue number" mutate exemption-without-issue

expect fail S4 "two exemptions matching the same artifact" mutate duplicate-exemption

echo
echo "── ledger hygiene ──"
expect fail H1 "the same artifact declared twice" \
    python3 -c 'import re,pathlib;p=pathlib.Path("tools/delivery-artifacts.toml");t=p.read_text();m=re.search(r"\[\[artifact\]\]\nid = \"chart:fraiseql\"\n(?:.*\n)*?executes = \[[^\]]*\]\n",t);p.write_text(t.replace(m.group(0),m.group(0)+"\n"+m.group(0),1))'

echo
printf 'delivery-coverage self-test: %d run, %d failed\n' "$TESTS_RUN" "$TESTS_FAILED"
[ "$TESTS_FAILED" -eq 0 ] || exit 1
