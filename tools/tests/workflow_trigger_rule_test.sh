#!/usr/bin/env bash
# Acceptance test for the ONE implementation of GitHub's push ref-filter rule (#1301).
#
# Run directly:  bash tools/tests/workflow_trigger_rule_test.sh
# Exits non-zero if any assertion fails.
#
# `branches`/`branches-ignore` define the BRANCH half of a workflow's `push:` ref
# filter and `tags`/`tags-ignore` the TAG half; defining only one half leaves the
# other undefined, and the workflow does not run for events affecting the
# undefined ref kind. Four gates needed that rule and four gates wrote it, and two
# of the four were wrong in opposite directions for months. What follows is the
# shared table those four are now measured against, so a change to the rule cannot
# land in one reading of it only.
#
# The table is the point. Every row is driven through the rule itself AND through
# each consumer's own entry point, so a consumer that stops importing the rule and
# starts re-deriving it fails here even while its own suite passes.
#
# No Rust toolchain, no cargo, no network.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass() { TESTS_RUN=$((TESTS_RUN + 1)); echo "PASS  $1"; }
fail() {
    TESTS_RUN=$((TESTS_RUN + 1)); TESTS_FAILED=$((TESTS_FAILED + 1))
    echo "FAIL  $1"
    [ -n "${2:-}" ] && printf '%s\n' "$2" | sed 's/^/        /'
    return 0
}

# ── The shared table ─────────────────────────────────────────────────────────
#
# Written once, here, and read by every check below. Columns are the rule's own
# answers; the consumer columns are derived from them by each consumer's stated
# policy, which is why the two SDK/coverage answers legitimately differ on one row.
#
#   case | on:-block | no_branch | any_branch | every_branch | no_tag
#
# `any_branch` is #1119's question (is this SDK gated on a branch push at all?)
# and `every_branch` is #1289's (can this context be a REQUIRED check?). They
# differ on `branches: ['feature/**']` on purpose: a wildcard prefix gates the
# branches a contributor would use, but a required check has to run on every
# branch or GitHub reports "not run" and blocks the merge forever.
cat >"$WORK/table.py" <<'PY'
TABLE = [
    # (label,            push block,                     no_branch, any, every, no_tag)
    ("bare push",        "  push:\n",                        False, True,  True,  False),
    ("branches ['**']",  "  push:\n    branches: ['**']\n",  False, True,  True,  True),
    ("branches [dev]",   "  push:\n    branches: [dev]\n",   False, False, False, True),
    ("branches feature/**",
                         "  push:\n    branches: ['feature/**']\n",
                                                             False, True,  False, True),
    ("branches block **","  push:\n    branches:\n      - '**'\n",
                                                             False, True,  True,  True),
    ("tags only",        "  push:\n    tags: ['v*']\n",       True, False, False, False),
    ("tags + branches",  "  push:\n    branches: ['**']\n    tags: ['v*']\n",
                                                             False, True,  True,  False),
    # The exclusion pattern deliberately does NOT match the probe tag: this row
    # asks which ref HALVES are defined, and a `tags-ignore: ['v*']` would answer
    # False in the claims column for a pattern reason instead.
    ("tags-ignore only", "  push:\n    tags-ignore: ['release-*']\n",
                                                              True, False, False, False),
    ("branches-ignore",  "  push:\n    branches-ignore: [gh-pages]\n",
                                                             False, True,  True,  True),
    ("branches-ignore + tags",
                         "  push:\n    branches-ignore: [gh-pages]\n    tags: ['v*']\n",
                                                             False, True,  True,  False),
    ("paths only",       "  push:\n    paths:\n      - 'sdks/official/demo/**'\n",
                                                             False, True,  True,  False),
]
PY

# ── driver ───────────────────────────────────────────────────────────────────
#
# One argument: the tree whose tools/ to load. Prints one `label|field=value`
# line per assertion so a mutation's damage is attributable to a row, not to a
# summary. Exits 1 on the first disagreement it can attribute, 2 if it could not
# run at all — a harness must never read "could not run" as "found nothing".
cat >"$WORK/drive.py" <<'PY'
import importlib.util, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from table import TABLE  # noqa: E402

TREE = Path(sys.argv[1])


def load(name, rel):
    path = TREE / rel
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        print(f"CANNOT-RUN: no loader for {path}", file=sys.stderr)
        raise SystemExit(2)
    m = importlib.util.module_from_spec(spec)
    sys.modules[name] = m
    try:
        spec.loader.exec_module(m)
    except Exception as exc:  # a mutated copy that will not import
        print(f"CANNOT-RUN: {rel}: {type(exc).__name__}: {exc}", file=sys.stderr)
        raise SystemExit(2)
    return m


scov = load("_t_scov", "tools/check-suite-coverage.py")
reach = load("_t_reach", "tools/check-workflow-job-reachability.py")
claims = load("_t_claims", "tools/check-sdk-publication-claims.py")

bad = 0
checked = 0
mismatched: set[tuple[str, str]] = set()


def check(label, field, got, want):
    global bad, checked
    checked += 1
    status = "ok" if got == want else "MISMATCH"
    print(f"{label}|{field}={got}|want={want}|{status}")
    if got != want:
        bad += 1
        mismatched.add((label, field))


for label, block, no_branch, any_b, every_b, no_tag in TABLE:
    doc = scov.parse_yaml("on:\n" + block)
    cfg = (doc.get("on") or {}).get("push")
    f = scov.push_ref_filter(cfg)

    # 1. the rule itself
    check(label, "rule.no_branch", f.reaches_no_branch(), no_branch)
    check(label, "rule.any_branch", f.reaches_any_branch(), any_b)
    check(label, "rule.every_branch", f.reaches_every_branch(), every_b)
    check(label, "rule.no_tag", f.reaches_no_tag(), no_tag)

    # 2. consumer: check-suite-coverage.py — may this context be REQUIRED?
    check(label, "coverage", scov._push_reaches_working_branches(doc["on"]), every_b)

    # 3. consumer: check-workflow-job-reachability.py — does a branch world exist?
    #    Derived from the rule's no_branch column, which is the invariant that
    #    ties this consumer to the other three.
    worlds = reach.worlds_for(doc["on"])
    check(label, "reach.branch_world",
          any(w.refs.kind == "branch" for w in worlds), not no_branch)
    check(label, "reach.tag_world",
          any(w.refs.kind == "tag" for w in worlds), not no_tag)

    # 4. consumer: check-sdk-publication-claims.py — does a tag push start it?
    #    `no_tag` is the only column it reads; a row whose `tags:` list excludes
    #    the probe tag is a pattern question, not a ref-half one, so the probe
    #    tag matches every list in the table.
    check(label, "claims.tag_push",
          claims._tag_push_runs_workflow(doc["on"], "v9.9.9"), not no_tag)

print(f"CHECKED {checked}")
if checked != len(TABLE) * 8:
    print(f"CANNOT-RUN: expected {len(TABLE) * 8} assertions, made {checked}",
          file=sys.stderr)
    raise SystemExit(2)

# With two extra arguments the driver asserts a SPECIFIC mismatch instead of the
# absence of all of them — the mutation harness's question. Comparing here rather
# than grepping the output is deliberate: `branches ['**']` and `feature/**` are
# regex metacharacters, and a grep over them matches nothing and reads exactly
# like "the mutation broke some other row".
if len(sys.argv) == 4:
    want = (sys.argv[2], sys.argv[3])
    if want not in mismatched:
        print(f"EXPECTED-MISMATCH-ABSENT: {want[0]}|{want[1]} did not fail; "
              f"mismatches were {sorted(mismatched)}", file=sys.stderr)
        raise SystemExit(1)
    print(f"ATTRIBUTED {want[0]}|{want[1]}")
    raise SystemExit(0)

raise SystemExit(1 if bad else 0)
PY

echo "workflow push ref-filter rule — one implementation, four consumers"
echo
echo "── 1. the shared table, on the tree as committed ──"

rc=0
out="$(python3 "$WORK/drive.py" "$REPO_ROOT" 2>&1)" || rc=$?
if [ "$rc" -eq 0 ]; then
    n="$(printf '%s' "$out" | sed -n 's/^CHECKED //p')"
    pass "all four consumers agree with the table ($n assertions)"
else
    fail "the table and the tree disagree (exit $rc)" "$out"
fi

# The count is asserted, not assumed: a driver that silently checked fewer rows
# would report a clean sweep over nothing.
want_assertions=$(( $(grep -c '^    ("' "$WORK/table.py") * 8 ))
if printf '%s' "$out" | grep -qx "CHECKED $want_assertions"; then
    pass "the driver made all $want_assertions assertions"
else
    fail "assertion COUNT is not $want_assertions" "$out"
fi

echo
echo "── 2. red capability: each branch of the rule, mutated ──"
#
# A pin that passes when the thing it pins is broken is decoration. Every branch
# of the rule is deleted in turn, in a COPY of the tree, and the row that branch
# exists for must be the row that fails. Mutating a copy is deliberate: reverting
# in place by `git checkout` no-ops on an untracked file and discards real work on
# a tracked one.

mutate() {
    local label="$1" want_row="$2" want_field="$3" find="$4" replace="$5"
    local dir="$WORK/mut" rc out
    rm -rf "$dir"; mkdir -p "$dir/tools"
    cp "$REPO_ROOT"/tools/*.py "$dir/tools/"
    python3 - "$dir/tools/check-suite-coverage.py" "$find" "$replace" <<'PY'
import sys
from pathlib import Path
p, find, replace = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = p.read_text()
if find not in text:
    sys.exit(f"MUTATION ANCHOR MISSING: {find!r} — the rule moved; re-derive this pin")
p.write_text(text.replace(find, replace, 1))
PY
    # The failure must be attributable to the row this branch exists for, and to
    # that row's own field: a mutation that reddens some OTHER row proves nothing
    # about this one. The driver does the comparison — see its tail for why.
    rc=0
    out="$(python3 "$WORK/drive.py" "$dir" "$want_row" "$want_field" 2>&1)" || rc=$?
    case "$rc" in
        0) pass "$label → ${want_row} / ${want_field} fails" ;;
        2) fail "$label: the mutated tree could not run at all (exit 2)" "$out" ;;
        *) fail "$label: the branch was deleted and ${want_row}/${want_field} still passed" \
                "$(printf '%s\n' "$out" | grep -E 'MISMATCH|EXPECTED-MISMATCH-ABSENT' || true)" ;;
    esac
}

mutate "reaches_no_branch always False" "tags only" "rule.no_branch" \
    "return self.tag_half_defined and not self.branch_half_defined" \
    "return False"

mutate "reaches_no_tag always False" "branches ['**']" "rule.no_tag" \
    "return self.branch_half_defined and not self.tag_half_defined" \
    "return False"

mutate "branch half forgets branches-ignore" "branches-ignore" "rule.no_tag" \
    '_BRANCH_KEYS = ("branches", "branches-ignore")' \
    '_BRANCH_KEYS = ("branches",)'

mutate "tag half forgets tags-ignore" "tags-ignore only" "rule.no_branch" \
    '_TAG_KEYS = ("tags", "tags-ignore")' \
    '_TAG_KEYS = ("tags",)'

mutate "every_branch accepts any allow-list" "branches [dev]" "rule.every_branch" \
    'return any(p in ("*", "**") for p in self.branch_patterns)' \
    'return True'

mutate "any_branch rejects a wildcard prefix" "branches feature/**" "rule.any_branch" \
    'p in ("*", "**") or p.endswith("**") for p in self.branch_patterns' \
    'p in ("*", "**") for p in self.branch_patterns'

echo
echo "── 3. the SDK gate, end to end, over the two rows it used to get wrong ──"
#
# Driving the rule proves the rule. These prove the WIRING: before #1301 this gate
# regexed `^\s*branches:` and `^\s*tags:` out of raw text, so `branches-ignore:`
# and `tags-ignore:` matched neither and inverted the verdict in both directions.

sdk_fixture() {
    local dir="$1" push="$2"
    mkdir -p "$dir/tools" "$dir/sdks/official/fraiseql-demo" "$dir/.github/workflows"
    cp "$REPO_ROOT/tools/check-sdk-workflow-coverage.py" \
       "$REPO_ROOT/tools/check-suite-coverage.py" "$dir/tools/"
    { echo "name: Demo"; echo "on:"; printf '%s' "$push"
      echo "jobs:"; echo "  t:"; echo "    runs-on: ubuntu-latest"
      echo "    steps:"; echo "      - run: echo hi"; } >"$dir/.github/workflows/demo.yml"
}

sdk_expect() {
    local label="$1" want="$2" dir="$3" needle="${4:-}"
    local rc=0 out
    set +e
    out="$(SDK_WORKFLOW_ROOT="$dir" python3 "$dir/tools/check-sdk-workflow-coverage.py" 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -ne "$want" ]; then
        fail "$label: exit $rc, wanted $want" "$out"; return
    fi
    if [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle"; then
        fail "$label: output did not mention '$needle'" "$out"; return
    fi
    pass "$label"
}

# `tags-ignore` alone leaves the branch half undefined: no branch push runs this,
# so the SDK is NOT gated. The old regex saw no `tags:` key and answered "covered".
sdk_fixture "$WORK/sdk_tagsig" "  push:
    tags-ignore: ['v*']
    paths:
      - 'sdks/official/fraiseql-demo/**'
"
sdk_expect "a tags-ignore-only workflow does not gate an SDK" 1 "$WORK/sdk_tagsig" \
    "sdks/official/fraiseql-demo"

# The mirror: `branches-ignore` DOES define the branch half, so every branch but
# the excluded ones runs it. The old regex saw no `branches:` key, saw `tags:`,
# and answered "tags-only" — refusing a workflow that gates fine.
sdk_fixture "$WORK/sdk_brig" "  push:
    branches-ignore: [gh-pages]
    tags: ['v*']
    paths:
      - 'sdks/official/fraiseql-demo/**'
"
sdk_expect "a branches-ignore workflow beside tags DOES gate an SDK" 0 "$WORK/sdk_brig"

# Flow style is REFUSED now, where it used to be skipped in silence. The old
# hand parser produced no `push:` block for it and moved on; the shared parser
# resolves one level of flow and raises on a nested collection rather than
# handing back `"['**']"`, which every caller would have read as "branches is
# not a list". Neither answer gates the SDK — but one of them says so.
sdk_fixture "$WORK/sdk_flow" "  push: {branches: ['**'], paths: ['sdks/official/fraiseql-demo/**']}
"
sdk_expect "a nested flow collection is refused LOUDLY, not skipped" 2 "$WORK/sdk_flow" \
    "nested flow collection"

# A flow mapping whose values are plain scalars is within the parser's one level
# and still reads — the refusal above is about NESTING, not about flow style.
sdk_fixture "$WORK/sdk_flow_ok" "  push: {branches: dev}
    paths:
      - 'sdks/official/fraiseql-demo/**'
"
sdk_expect "a single-level flow mapping still parses" 1 "$WORK/sdk_flow_ok" \
    "sdks/official/fraiseql-demo"

echo
echo "── 4. no fifth copy ──"

rc=0; out="$(python3 "$REPO_ROOT/tools/check-trigger-rule-copies.py" 2>&1)" || rc=$?
[ "$rc" -eq 0 ] && pass "the rule is stated once in the tree as committed" \
                || fail "tools/ carries a second implementation" "$out"

# Red capability for that gate, in both idioms it knows, and for the case where
# the owner itself has gone: a copy-refusing gate whose subject was deleted would
# otherwise pass over a tree with no rule in it at all.
copy_probe() {
    local label="$1" want="$2" body="$3"
    local dir="$WORK/copies" rc out
    rm -rf "$dir"; mkdir -p "$dir/tools"
    cp "$REPO_ROOT/tools/check-suite-coverage.py" "$dir/tools/"
    printf '%s\n' "$body" >"$dir/tools/newcomer.py"
    rc=0
    out="$(TRIGGER_RULE_ROOT="$dir" python3 "$REPO_ROOT/tools/check-trigger-rule-copies.py" 2>&1)" || rc=$?
    [ "$rc" -eq "$want" ] && pass "$label" || fail "$label: exit $rc, wanted $want" "$out"
}

copy_probe "a membership-test copy is refused" 1 \
    'def reaches(cfg):
    return "tags" in cfg and "branches" not in cfg'

copy_probe "a regex-over-raw-YAML copy is refused" 1 \
    'import re
def reaches(text):
    return re.search(r"^\s*branches:\s*(.*)$", text)'

copy_probe "reading a branches VALUE, halves already known, is not a copy" 0 \
    'def patterns(cfg):
    return list(cfg.get("branches") or []) + list(cfg.get("branches-ignore") or [])'

copy_probe "prose quoting the idiom is not a copy" 0 \
    'def helper(cfg):
    """Historically this was `"branches" in cfg` and a re.search(r"tags:").

    Both forms are described here and neither is executed.
    """
    return None'

rm -rf "$WORK/copies"; mkdir -p "$WORK/copies/tools"
rc=0
out="$(TRIGGER_RULE_ROOT="$WORK/copies" python3 "$REPO_ROOT/tools/check-trigger-rule-copies.py" 2>&1)" || rc=$?
[ "$rc" -eq 1 ] && pass "a tree whose owner is gone FAILS rather than passing vacuously" \
                || fail "an ownerless tree returned $rc, wanted 1" "$out"

echo
if [ "$TESTS_FAILED" -ne 0 ]; then
    echo "workflow trigger rule self-test: $TESTS_FAILED of $TESTS_RUN FAILED"
    exit 1
fi
echo "workflow trigger rule self-test: $TESTS_RUN/$TESTS_RUN passed"
