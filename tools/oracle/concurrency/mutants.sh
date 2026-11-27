#!/usr/bin/env bash
# The concurrency oracle's product mutants (see README.md, "Self-test"). Local only: each
# mutant is a server rebuild.
#
#   tools/oracle/concurrency/mutants.sh [MUTANT...]
#
# Copies HEAD with `git archive` into ORACLE_MUTANT_DIR (default: a temporary directory),
# builds the server there unmutated, and runs the oracle on it: the baseline must match.
# It does so again at the end, on the restored copy rebuilt.
# Then, for each mutant in mutants.json, applies its edits, rebuilds, runs the groups it
# names, and requires every cell in its `expect` list to be reported NEW. The copy's
# files are restored from their saved bytes after each mutant and compared with `cmp`.
# The working tree is never edited.
#
# Environment: CARGO_TARGET_DIR for the copy's builds (default: <copy>/target), and the
# variables run.sh reads (ORACLE_ADMIN_URL, ORACLE_DOCKER_LABEL, ...).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
copy="${ORACLE_MUTANT_DIR:-$(mktemp -d)}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$copy/target}"
features="cli,observers"
python3=python3

mkdir -p "$copy"
# `-m`: every file gets the time of extraction, so a build left in CARGO_TARGET_DIR by an
# earlier run (perhaps of a mutant) is older than every source and cannot be reused.
git -C "$repo" archive HEAD | tar -x -m -C "$copy"
build() {
  (cd "$copy" && cargo build --release -p fraiseql-cli --bin fraiseql-cli -q \
    && cargo build --release -p fraiseql-server --bin fraiseql-server --features "$features" -q)
}
oracle() {
  FRAISEQL_CLI="$CARGO_TARGET_DIR/release/fraiseql-cli" \
    FRAISEQL_SERVER="$CARGO_TARGET_DIR/release/fraiseql-server" \
    "$copy/tools/oracle/concurrency/run.sh" "$@"
}
field() { "$python3" -c 'import json,sys; m=[m for m in json.load(open(sys.argv[1]))["mutants"] if m["name"]==sys.argv[2]][0]; v=m[sys.argv[3]]; print(" ".join(v) if isinstance(v,list) else v)' "$here/mutants.json" "$1" "$2"; }

echo "=== control: the unmutated copy"
build
oracle >"$copy/control.log" 2>&1 || { echo "✗ control: the unmutated build does not match the baseline" >&2; tail -20 "$copy/control.log" >&2; exit 2; }
tail -1 "$copy/control.log"

names=("$@")
if [[ ${#names[@]} -eq 0 ]]; then
  mapfile -t names < <("$python3" -c 'import json,sys; [print(m["name"]) for m in json.load(open(sys.argv[1]))["mutants"]]' "$here/mutants.json")
fi
failed=0
for name in "${names[@]}"; do
  echo "=== mutant: $name"
  saved="$copy/.saved-$name"
  mkdir -p "$saved"
  # Apply every edit; each `old` must occur exactly once. The touched files are saved first.
  "$python3" - "$here/mutants.json" "$name" "$copy" "$saved" <<'PY'
import json, pathlib, shutil, sys
spec, name, copy, saved = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3]), pathlib.Path(sys.argv[4])
m = next(m for m in json.load(open(spec))["mutants"] if m["name"] == name)
for e in m["edits"]:
    p = copy / e["file"]
    keep = saved / e["file"]
    if not keep.exists():
        keep.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(p, keep)
    text = p.read_text()
    if text.count(e["old"]) != 1:
        sys.exit(f"{name}: {e['file']}: the edit's text occurs {text.count(e['old'])} times, not once")
    p.write_text(text.replace(e["old"], e["new"]))
PY
  log="$copy/mutant-$name.log"
  if build >"$log" 2>&1; then
    oracle --only "$(field "$name" groups)" >>"$log" 2>&1 || true
    for cell in $(field "$name" expect); do
      if grep -q "^✗ NEW $cell:" "$log"; then
        echo "  killed: $cell"
      else
        echo "  ✗ survived: $cell is not reported NEW" >&2
        failed=1
      fi
    done
  else
    echo "  ✗ the mutant does not build:" >&2
    tail -20 "$log" >&2
    failed=1
  fi
  # Restore the saved bytes and prove the copy is unmutated again. Not `cp -p`: the restored
  # file must be newer than the mutant's build, or cargo keeps that build for the next one.
  (cd "$saved" && find . -type f -print0) | while IFS= read -r -d '' f; do
    cp "$saved/$f" "$copy/$f"
    cmp -s "$saved/$f" "$copy/$f" || { echo "✗ $f was not restored" >&2; exit 3; }
  done
  rm -rf "$saved"
done
# The restored copy, rebuilt, must match the baseline again: a restoration cargo did not
# see would leave a mutant in every later build.
echo "=== control: the restored copy"
build
oracle >"$copy/control-restored.log" 2>&1 || { echo "✗ control: the restored build does not match the baseline" >&2; tail -20 "$copy/control-restored.log" >&2; exit 2; }
tail -1 "$copy/control-restored.log"
[[ $failed -eq 0 ]] && echo "mutants: every expected cell was reported NEW"
exit "$failed"
