#!/usr/bin/env bash
# Unit tests for tools/check-gate-reachability.py.
#
# Run directly:  bash tools/tests/gate_reachability_test.sh
# Exits non-zero if any assertion fails.
#
# The gate counts a gate as run by CI only when a CI leg INVOKES it, and holds the
# make targets CI runs to failing when a gate fails. Each red case below is a way a
# gate can look covered and run nowhere, or run and have its failure dropped: a
# mention instead of an invocation, a commented line, a self-test that runs it over
# fixtures, a Dagger function no workflow calls, a target only `make preflight`
# reaches, an exec that discards make's exit code, a recipe that ignores its own.
# Each must be named. The green cases are the shapes the real tree uses, so the gate
# is not passing by being strict about everything.
# shellcheck disable=SC2016 # Reason: the fixtures hold literal `$(...)` and backticks.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GATE="$REPO_ROOT/tools/check-gate-reachability.py"

TESTS_RUN=0
TESTS_FAILED=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

EXEC='git init -q . >/dev/null && make -k --no-print-directory shell-gates'

# fixture <dir>: a workflow calling `dagger call preflight`; Preflight reaching
# ShellGates (one make exec) and Lint (a script list); an Unused function; and a
# Makefile whose shell-gates target runs a lint and a self-test.
fixture() {
    local dir="$1"
    mkdir -p "$dir/.github/workflows" "$dir/.dagger" "$dir/tools/tests"
    cat >"$dir/.github/workflows/preflight.yml" <<'YML'
name: preflight
on: push
jobs:
  preflight:
    runs-on: ubuntu-latest
    steps:
      - run: dagger call preflight --source=.
YML
    cat >"$dir/.dagger/main.go" <<GO
package main

func (m *FraiseqlCi) Preflight(ctx context.Context) (string, error) {
	m.Lint(ctx)
	return m.ShellGates(ctx)
}

func (m *FraiseqlCi) ShellGates(ctx context.Context) (string, error) {
	script := "$EXEC"
	return run([]string{"bash", "-c", script})
}

func (m *FraiseqlCi) Lint(ctx context.Context) (string, error) {
	script := strings.Join([]string{
		"bash tools/check-b.sh",
		"bash tools/tests/b_test.sh",
	}, "\n")
	return run(script)
}

func (m *FraiseqlCi) Unused(ctx context.Context) (string, error) {
	return run("true")
}
GO
    cat >"$dir/Makefile" <<'MK'
.PHONY: shell-gates
shell-gates: lint-a
shell-gates: test-a-gate

.PHONY: lint-a
lint-a:
	@python3 tools/check-a.py

test-a-gate:
	@bash tools/tests/a_test.sh
MK
    printf '#!/usr/bin/env python3\n' >"$dir/tools/check-a.py"
    printf '#!/usr/bin/env bash\ntrue\n' >"$dir/tools/check-b.sh"
    printf '#!/usr/bin/env bash\nbash tools/check-a.py\n' >"$dir/tools/tests/a_test.sh"
    printf '#!/usr/bin/env bash\nbash tools/check-b.sh\n' >"$dir/tools/tests/b_test.sh"
}

# case_dir <name>: a fresh fixture for one case.
case_dir() {
    local dir="$WORK/$1"
    fixture "$dir"
    printf '%s' "$dir"
}

run_gate() { GATE_REACHABILITY_ROOT="$1" python3 "$GATE" >"$1.out" 2>&1; }

# expect_red <name> <dir> <text the output must contain>
expect_red() {
    local name="$1" dir="$2" want="$3"
    TESTS_RUN=$((TESTS_RUN + 1))
    if run_gate "$dir"; then
        echo "FAIL: $name: the gate passed"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    elif ! grep -qF -- "$want" "$dir.out"; then
        echo "FAIL: $name: red, but the output does not say: $want"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    else
        echo "ok:   $name"
    fi
}

# expect_green <name> <dir>
expect_green() {
    local name="$1" dir="$2"
    TESTS_RUN=$((TESTS_RUN + 1))
    if run_gate "$dir" && grep -qF 'gate-reachability: OK' "$dir.out"; then
        echo "ok:   $name"
    else
        echo "FAIL: $name: the gate did not pass"
        sed 's/^/    /' "$dir.out"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

ORPHAN='tools/check-c.sh: no CI leg runs it'
new_check() { printf '#!/usr/bin/env bash\ntrue\n' >"$1/tools/check-c.sh"; }
# with_lint_a_line <dir> <line>: a second recipe line for lint-a, which shell-gates
# reaches. `%` delimits: the lines carry `|` and `#`.
with_lint_a_line() {
    local dir="$1" line="$2"
    sed -i "s%^\t@python3 tools/check-a.py\$%\t@python3 tools/check-a.py\n\t$line%" "$dir/Makefile"
}

# --- green -------------------------------------------------------------------

d="$(case_dir baseline)"
expect_green "every gate reached through shell-gates and the Lint function" "$d"

d="$(case_dir go-string-list)"
new_check "$d"
sed -i 's|return run("true")|return run("true")\n}\n\nfunc (m *FraiseqlCi) Fmt() {\n\tWithExec([]string{"bash", "tools/check-c.sh"})|' "$d/.dagger/main.go"
sed -i 's|return m.ShellGates(ctx)|m.Fmt()\n\treturn m.ShellGates(ctx)|' "$d/.dagger/main.go"
expect_green "a script in a Go string list, in a function the root references" "$d"

d="$(case_dir raw-string)"
new_check "$d"
cat >>"$d/.dagger/main.go" <<'GO'

func (m *FraiseqlCi) Combo() {
	script := `set +e
out=$(bash tools/check-c.sh 2>&1)
exit 0`
	run(script)
}
GO
sed -i 's|return m.ShellGates(ctx)|m.Combo()\n\treturn m.ShellGates(ctx)|' "$d/.dagger/main.go"
expect_green "a script in a raw string spanning lines" "$d"

d="$(case_dir sibling-wrapper)"
printf '#!/usr/bin/env python3\n' >"$d/tools/check-c.py"
printf '#!/usr/bin/env bash\nexec python3 "$(dirname "${BASH_SOURCE[0]}")/check-c.py" "$@"\n' >"$d/tools/check-b.sh"
expect_green "a script its reached wrapper runs from its own directory" "$d"

d="$(case_dir workflow-direct)"
new_check "$d"
printf '      - run: bash tools/check-c.sh\n' >>"$d/.github/workflows/preflight.yml"
expect_green "a script a workflow step runs" "$d"

d="$(case_dir exempted)"
new_check "$d"
printf '[[exempt]]\npath = "tools/check-c.sh"\nreason = "a workstation tool"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_green "a script nothing runs, exempted with a reason" "$d"

d="$(case_dir wrapper-target)"
printf '\nlint-b:\n\t@bash tools/check-b.sh\n' >>"$d/Makefile"
expect_green "a gate target that wraps a script a leg runs some other way" "$d"

d="$(case_dir target-exempted)"
printf '\nlint-local:\n\t@echo local\n' >>"$d/Makefile"
printf '[[exempt]]\ntarget = "lint-local"\nreason = "a local runner"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_green "a gate target nothing runs, exempted with a reason" "$d"

d="$(case_dir reasoned-true)"
with_lint_a_line "$d" '@grep -q x README || true # Reason: README may not exist yet'
expect_green "a discarded failure on a line saying # Reason: why" "$d"

d="$(case_dir substitution-true)"
with_lint_a_line "$d" '@n="$$(grep -c x README || true)"; test "$$n" -lt 3'
expect_green "\`|| true\` inside a command substitution, whose result is still tested" "$d"

d="$(case_dir unreached-swallow)"
printf '\nlocal-only:\n\t-@python3 tools/check-a.py || true\n' >>"$d/Makefile"
expect_green "a discarded failure in a target no CI leg runs" "$d"

d="$(case_dir makeflags-k)"
printf 'MAKEFLAGS += -k --no-print-directory\n' >>"$d/Makefile"
expect_green "MAKEFLAGS with -k and a long option holding an i" "$d"

# --- red: looks covered, runs nowhere ----------------------------------------

d="$(case_dir orphan)"
new_check "$d"
expect_red "a script nothing invokes" "$d" "$ORPHAN"

d="$(case_dir echo-mention)"
new_check "$d"
printf '\t@echo "see tools/check-c.sh"\n' >>"$d/Makefile"
expect_red "a script only named in an echo" "$d" "$ORPHAN"

d="$(case_dir case-arm)"
new_check "$d"
printf 'case "$f" in\n    tools/check-c.sh) return 0 ;;\nesac\n' >>"$d/tools/check-b.sh"
expect_red "a script only named in a case arm" "$d" "$ORPHAN"

d="$(case_dir self-test-only)"
new_check "$d"
printf 'bash tools/check-c.sh\n' >>"$d/tools/tests/a_test.sh"
expect_red "a script run only by a self-test, over its fixtures" "$d" "$ORPHAN"

d="$(case_dir unused-function)"
new_check "$d"
sed -i 's|return run("true")|return run("bash tools/check-c.sh")|' "$d/.dagger/main.go"
expect_red "a script run only by a Dagger function no workflow calls" "$d" "$ORPHAN"

d="$(case_dir go-comment)"
new_check "$d"
sed -i 's|		"bash tools/check-b.sh",|		// "bash tools/check-c.sh",\n		"bash tools/check-b.sh",|' "$d/.dagger/main.go"
expect_red "a script on a commented Go line" "$d" "$ORPHAN"

d="$(case_dir yaml-comment)"
new_check "$d"
printf '      # - run: bash tools/check-c.sh\n' >>"$d/.github/workflows/preflight.yml"
expect_red "a script on a commented workflow line" "$d" "$ORPHAN"

d="$(case_dir preflight-only)"
sed -i '/^shell-gates: test-a-gate$/d' "$d/Makefile"
printf '\npreflight: shell-gates test-a-gate\n' >>"$d/Makefile"
expect_red "a gate moved from shell-gates to \`preflight\` alone" "$d" \
    'tools/tests/a_test.sh: no CI leg runs it'
expect_red "  ...and its target is named too" "$d" 'make test-a-gate: no CI leg runs it'

d="$(case_dir inline-target)"
printf '\ntest-c-gate:\n\t@test -f README\n' >>"$d/Makefile"
expect_red "an inline gate target nothing reaches" "$d" 'make test-c-gate: no CI leg runs it'

# --- red: the exec drops make's verdict --------------------------------------

EXACT='ShellGates: must run exactly'
d="$(case_dir exec-or-true)"
sed -i 's/shell-gates"$/shell-gates || true"/' "$d/.dagger/main.go"
expect_red "(d) the exec ends \`|| true\`" "$d" "$EXACT"

d="$(case_dir exec-ignore-errors)"
sed -i 's/make -k /make -i -k /' "$d/.dagger/main.go"
expect_red "(e) the exec passes -i" "$d" "$EXACT"

d="$(case_dir exec-other-target)"
sed -i 's/--no-print-directory shell-gates/--no-print-directory lint-a/' "$d/.dagger/main.go"
expect_red "(f) the exec runs another target" "$d" "$EXACT"

d="$(case_dir exec-in-comment)"
sed -i "s|^\tscript := \"$EXEC\"|\t// script := \"$EXEC\"\n\tscript := \"true\"|" "$d/.dagger/main.go"
expect_red "(g) the exec survives only in a comment" "$d" "$EXACT"

d="$(case_dir exec-second-make)"
sed -i 's|return run(\[\]string{"bash", "-c", script})|run([]string{"bash", "-c", "make lint-a"})\n\treturn run([]string{"bash", "-c", script})|' "$d/.dagger/main.go"
expect_red "the function runs a second make command" "$d" "$EXACT"

d="$(case_dir exec-unreached)"
sed -i 's|return m.ShellGates(ctx)|return "", nil|' "$d/.dagger/main.go"
expect_red "no workflow reaches ShellGates" "$d" 'ShellGates: no workflow reaches it'

# --- red: a recipe CI runs drops its own failure (i, j, k) --------------------

d="$(case_dir dash-prefix)"
sed -i 's|^\t@python3 tools/check-a.py$|\t-@python3 tools/check-a.py|' "$d/Makefile"
expect_red "(i) make's \`-\` prefix on a gate's recipe line" "$d" \
    'make lint-a: a recipe line ignores its failure (`-`)'

d="$(case_dir at-dash-prefix)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@-python3 tools/check-a.py|' "$d/Makefile"
expect_red "(i) the \`-\` prefix after \`@\`" "$d" \
    'make lint-a: a recipe line ignores its failure (`-`)'

d="$(case_dir dash-prefix-deep)"
sed -i 's|^\t@bash tools/tests/a_test.sh$|\t-bash tools/tests/a_test.sh|' "$d/Makefile"
expect_red "(i) the \`-\` prefix in another target shell-gates reaches" "$d" \
    'make test-a-gate: a recipe line ignores its failure (`-`)'

SWALLOW='make lint-a: a command ends by discarding its failure'
d="$(case_dir or-true)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@python3 tools/check-a.py \|\| true|' "$d/Makefile"
expect_red "(j) \`|| true\` ending the command" "$d" "$SWALLOW"

d="$(case_dir or-colon)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@python3 tools/check-a.py \|\| :|' "$d/Makefile"
expect_red "(j) \`|| :\` ending the command" "$d" "$SWALLOW"

d="$(case_dir semicolon-true)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@python3 tools/check-a.py; true|' "$d/Makefile"
expect_red "(j) \`; true\` ending the command" "$d" "$SWALLOW"

d="$(case_dir or-true-midline)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@python3 tools/check-a.py \|\| true; echo done|' "$d/Makefile"
expect_red "(j) \`|| true\` ending a command mid-line" "$d" "$SWALLOW"

d="$(case_dir or-true-comment)"
with_lint_a_line "$d" '@grep -q x README || true # README is optional'
expect_red "(j) \`|| true\` followed by a comment that gives no reason" "$d" \
    'make lint-a: a command ends by discarding its failure'

d="$(case_dir or-true-continued)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@python3 tools/check-a.py \\\n\t\t\|\| true|' "$d/Makefile"
expect_red "(j) \`|| true\` on a continuation line" "$d" "$SWALLOW"

d="$(case_dir ignore-all)"
printf '\n.IGNORE:\n' >>"$d/Makefile"
expect_red "(k) \`.IGNORE:\` with no prerequisites" "$d" '`.IGNORE:` with no prerequisites'

d="$(case_dir ignore-target)"
printf '\n.IGNORE: lint-a\n' >>"$d/Makefile"
expect_red "(k) \`.IGNORE:\` naming a target CI runs" "$d" '`.IGNORE:` names lint-a'

d="$(case_dir makeflags-i)"
printf 'MAKEFLAGS += -i\n' >>"$d/Makefile"
expect_red "(k) MAKEFLAGS carrying -i" "$d" 'MAKEFLAGS carries -i'

d="$(case_dir makeflags-cluster)"
printf 'MAKEFLAGS += -ki\n' >>"$d/Makefile"
expect_red "(k) MAKEFLAGS carrying -i in a cluster" "$d" 'MAKEFLAGS carries -i'

d="$(case_dir makeflags-long)"
printf 'export MAKEFLAGS := --ignore-errors\n' >>"$d/Makefile"
expect_red "(k) MAKEFLAGS carrying --ignore-errors" "$d" 'MAKEFLAGS carries -i'

d="$(case_dir recipe-make-i)"
sed -i 's|^\t@python3 tools/check-a.py$|\t@$(MAKE) -i lint-b\n\t@python3 tools/check-a.py|' "$d/Makefile"
expect_red "(k) a recipe CI runs calls make with -i" "$d" 'make lint-a: runs make with -i'

# --- red: exemptions ---------------------------------------------------------

d="$(case_dir stale-exemption)"
printf '[[exempt]]\npath = "tools/check-b.sh"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a script CI runs" "$d" \
    'tools/check-b.sh: exempted, but a CI leg runs it'

d="$(case_dir missing-exemption)"
printf '[[exempt]]\npath = "tools/check-gone.sh"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a script that does not exist" "$d" \
    'tools/check-gone.sh: exempted, but no such gate script exists'

d="$(case_dir stale-target-exemption)"
printf '[[exempt]]\ntarget = "lint-a"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a target CI runs" "$d" \
    'make lint-a: exempted, but a CI leg runs it'

d="$(case_dir missing-target-exemption)"
printf '[[exempt]]\ntarget = "lint-gone"\nreason = "was local"\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption for a target that does not exist" "$d" \
    'make lint-gone: exempted, but no such target exists'

d="$(case_dir no-reason)"
new_check "$d"
printf '[[exempt]]\npath = "tools/check-c.sh"\nreason = " "\n' \
    >"$d/tools/gate-reachability-exemptions.toml"
expect_red "an exemption with no reason" "$d" 'exemption for tools/check-c.sh gives no reason'

# --- red: the parser went blind ----------------------------------------------

d="$(case_dir no-dagger-call)"
sed -i 's|dagger call preflight|echo preflight|' "$d/.github/workflows/preflight.yml"
expect_red "no workflow runs \`dagger call\`" "$d" 'no `dagger call` in any workflow'

d="$(case_dir unknown-root)"
sed -i 's|dagger call preflight|dagger call elsewhere|' "$d/.github/workflows/preflight.yml"
expect_red "no workflow's \`dagger call\` names a Go function" "$d" \
    "no workflow's \`dagger call\` names a function"

echo ""
echo "$TESTS_RUN cases, $TESTS_FAILED failed"
if [ "$TESTS_FAILED" -ne 0 ]; then
    exit 1
fi
echo "OK: check-gate-reachability.py names every gate no CI leg runs, and every way a leg drops a failure."
