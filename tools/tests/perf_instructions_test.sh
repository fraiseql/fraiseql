#!/usr/bin/env bash
# perf_instructions_test.sh — the red capability of tools/check-perf-instructions.py.
#
# Each case writes iai-callgrind summary.json files for two benches and a baseline, then
# runs the comparator. A rise or a drop past the threshold is RED; a change inside it is
# GREEN; and every way the gate could measure nothing (no summaries, a 0 count, a bench
# or baseline entry with no partner) is RED rather than vacuously green.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-perf-instructions.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
cases=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

# summary <home> <id> <count> [Left|Both]
summary() {
  local dir="$1/b/executor/execute.$2" form="${4:-Both}" metrics
  mkdir -p "$dir"
  if [ "$form" = Left ]; then
    metrics="{\"Left\": {\"Int\": $3}}"
  else
    metrics="{\"Both\": [{\"Int\": $3}, {\"Int\": 1}]}"
  fi
  printf '{"module_path": "b::executor::execute", "id": "%s", "profiles": [{"summaries": {"total": {"summary": {"Callgrind": {"Ir": {"metrics": %s}}}}}}]}\n' \
    "$2" "$metrics" > "$dir/summary.json"
}

# new_case <name> <lean count> <fat count>: baseline 1000 and 2000.
new_case() {
  home="${tmp}/$1/iai"
  base="${tmp}/$1/baseline.json"
  mkdir -p "$home"
  summary "$home" lean "$2"
  summary "$home" fat "$3"
  printf '{"benchmarks": {"b::executor::execute.lean": 1000, "b::executor::execute.fat": 2000}}\n' > "$base"
}

# expect <rc> <substring> <description>
expect() {
  cases=$((cases + 1))
  python3 "$gate" --home "$home" --baseline "$base" > "${tmp}/out" 2>&1
  local rc=$?
  if [[ "$rc" -eq "$1" ]] && grep -qF -- "$2" "${tmp}/out"; then pass "$3"; else fail "$3 (rc=$rc, want $1, '$2')"; fi
}

echo "check-perf-instructions.py:"

new_case equal 1000 2000
expect 0 "perf gate: ok (2 benches" "counts equal to the baseline pass"

new_case inside 1029 1950
expect 0 "perf gate: ok" "a 2.9% rise and a 2.5% drop stay inside 3%"

new_case rise 1031 2000
expect 1 "execute.lean: +3.10% instructions, above the 3.0% limit" "a 3.1% rise is red"

new_case five 1000 2100
expect 1 "execute.fat: +5.00% instructions" "an injected 5% regression is red"

new_case drop 900 2000
expect 1 "execute.lean: -10.00%; lock it in: \"b::executor::execute.lean\": 900" "a drop past 3% must be locked in"

new_case left 1000 2000
summary "$home" lean 1100 Left
expect 1 "execute.lean: +10.00%" "a first run (Left metrics) is read too"

new_case zero 0 2000
expect 1 "callgrind measured nothing" "a 0 count is red: callgrind measured nothing"

new_case zero_print 0 2000
cases=$((cases + 1))
if python3 "$gate" --home "$home" --print > "${tmp}/out" 2>&1; then
  fail "--print refuses a 0 count rather than write it into a baseline"
else
  pass "--print refuses a 0 count rather than write it into a baseline"
fi

new_case extra 1000 2000
summary "$home" nested 500
expect 1 "execute.nested: no baseline entry" "a bench with no baseline entry is red"

new_case stale 1000 2000
rm -r "$home/b/executor/execute.fat"
expect 1 "execute.fat: in the baseline but not measured" "a baseline entry no bench measured is red"

new_case empty 1000 2000
rm -r "$home/b"
expect 1 "nothing was measured" "no summaries at all is red"

new_case nobase 1000 2000
rm "$base"
expect 1 "not found" "a missing baseline is red"

# Runs in subdirectories of the home: their mean is compared, so one noisy run passes.
new_case mean 1000 2000
mv "$home/b" "$home/r1"; summary "$home/r2" lean 1050; summary "$home/r2" fat 2000
summary "$home/r3" lean 980; summary "$home/r3" fat 2000
expect 0 "execute.lean: 1010 instructions" "the mean of the runs is compared, not one noisy run"

new_case mean_red 1000 2000
mv "$home/b" "$home/r1"; summary "$home/r2" lean 1100; summary "$home/r2" fat 2000
summary "$home/r3" lean 1090; summary "$home/r3" fat 2000
expect 1 "execute.lean: +6.30%" "a regression in most runs is red"

new_case print 1000 2000
cases=$((cases + 1))
python3 "$gate" --home "$home" --print > "${tmp}/out" 2>&1
if python3 -c 'import json,sys; d=json.load(open(sys.argv[1]))["benchmarks"]; sys.exit(d != {"b::executor::execute.lean": 1000, "b::executor::execute.fat": 2000})' "${tmp}/out"; then
  pass "--print writes the measured counts as a baseline"
else
  fail "--print writes the measured counts as a baseline"
fi

echo "${cases} cases, ${failures} failed"
[ "$failures" -eq 0 ]
