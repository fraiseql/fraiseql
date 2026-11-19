#!/usr/bin/env bash
# config_refusals_test.sh — the red capability of tools/check-config-refusals.py.
#
# Every case is a tree holding one file on the fraiseql.toml load path (and, from case 10,
# `commands/run.rs`, where that file meets the server's). A `bail!` there is a
# refused key that carries no diagnostic unless the allow-list names it as a value check; each
# way that can go wrong must fail, by name.
set -uo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
gate="${repo_root}/tools/check-config-refusals.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

mk() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }

F=crates/fraiseql-cli/src/config/section.rs

# new_case <n> <section.rs> <allow-list>
new_case() {
  root="${tmp}/case$1"
  mk "${root}/${F}" "$2"
  mk "${root}/crates/fraiseql-cli/src/schema/merger.rs" '// nothing raised here'
  mk "${root}/allow.txt" "$3"
}

run_gate() {
  CONFIG_REFUSALS_ROOT="$root" CONFIG_REFUSALS_ALLOWLIST="${root}/allow.txt" \
    python3 "$gate" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

expect_green() {
  if [ "$(cat "${tmp}/rc")" = 0 ]; then pass "$1"; else fail "$1"; fi
}

expect_red() { # expect_red <text that must be printed> <label>
  if [ "$(cat "${tmp}/rc")" = 1 ] && grep -F -- "$1" "${tmp}/out" >/dev/null; then
    pass "$2"
  else
    fail "$2"
  fi
}

PORT='fn check(port: u16) -> anyhow::Result<()> {
    if port == 0 {
        anyhow::bail!("[server] port must be non-zero");
    }
    Ok(())
}'
PORT_ENTRY="$F | [server] port must be non-zero | [server] port | value check: port 0"

# ── 1. A listed value check passes ──────────────────────────────────────────────────
new_case 1 "$PORT" "$PORT_ENTRY"
run_gate
expect_green "a value check listed with its key and reason"

# ── 2. An unlisted bail! fails, naming its file, line and message ───────────────────
new_case 2 "$PORT
fn redis(url: Option<&str>) -> anyhow::Result<()> {
    if url.is_some() {
        anyhow::bail!(\"[caching] redis_url is set, but nothing reads it\");
    }
    Ok(())
}" "$PORT_ENTRY"
run_gate
expect_red "${F}:9  [caching] redis_url is set, but nothing reads it" "an unlisted bail! is named by file, line and message"

# ── 3. A refusal(...) is not a site ─────────────────────────────────────────────────
new_case 3 "$PORT
fn redis(url: Option<&str>) -> anyhow::Result<()> {
    if url.is_some() {
        return Err(crate::config::refusal(Unread, \"caching\", \"redis_url\", m()));
    }
    Ok(())
}" "$PORT_ENTRY"
run_gate
expect_green "a refusal(...) needs no entry"

# ── 4. anyhow!(...) counts as bail! does ────────────────────────────────────────────
new_case 4 "fn f() -> anyhow::Error { anyhow!(\"[x] y is refused\") }" ''
run_gate
expect_red "[x] y is refused" "anyhow!(...) is a site"

# ── 5. An entry no site matches is stale ────────────────────────────────────────────
new_case 5 "$PORT" "$PORT_ENTRY
$F | [server] host must be set | [server] host | value check: empty host"
run_gate
expect_red "[server] host must be set" "a stale entry is named"

# ── 6. A malformed entry fails ──────────────────────────────────────────────────────
new_case 6 "$PORT" "$F | [server] port must be non-zero | [server] port"
run_gate
expect_red "malformed lines" "an entry without a reason is malformed"

# ── 7. A message passed through is listed by its line, and two alike are refused ────
new_case 7 'fn f(a: A, b: B) -> anyhow::Result<()> {
    a.check().map_err(|message| anyhow::anyhow!(message))?;
    b.check().map_err(|message| anyhow::anyhow!(message))?;
    Ok(())
}' "$F | a.check().map_err(|message| anyhow::anyhow!(message))?; | [a] | value check: a
$F | b.check().map_err(|message| anyhow::anyhow!(message))?; | [b] | value check: b"
run_gate
expect_green "a passed-through message is listed by its raising line"
new_case 7b 'fn f(a: A) -> anyhow::Result<()> {
    a.check().map_err(|message| anyhow::anyhow!(message))?;
    a.check().map_err(|message| anyhow::anyhow!(message))?;
    Ok(())
}' "$F | a.check().map_err(|message| anyhow::anyhow!(message))?; | [a] | value check: a"
run_gate
expect_red "read alike" "two sites one entry would stand for are refused"

# ── 8. A tree with no site at all is measuring nothing ──────────────────────────────
new_case 8 'fn f() {}' ''
run_gate
expect_red "measuring nothing" "an empty scope fails rather than passing vacuously"

# ── 9. Test files are not the load path ─────────────────────────────────────────────
new_case 9 "$PORT" "$PORT_ENTRY"
mk "${root}/crates/fraiseql-cli/src/config/tests.rs" 'fn t() { anyhow::bail!("in a test") }'
run_gate
expect_green "a bail! in tests.rs is not a site"

# ── 10. `fraiseql run` is on the path: its server-file refusals are sites too ───────
RUN=crates/fraiseql-cli/src/commands/run.rs
TWICE='fn refuse(made: bool) -> anyhow::Result<()> {
    if made {
        anyhow::bail!("`[server] port` in fraiseql.toml and `bind_addr` both set the same thing");
    }
    Ok(())
}'
new_case 10 "$PORT" "$PORT_ENTRY"
mk "${root}/${RUN}" "$TWICE"
run_gate
expect_red "${RUN}:3  \`[server] port\` in fraiseql.toml and \`bind_addr\` both set" "an unlisted bail! in commands/run.rs is named"
new_case 10b "$PORT" "$PORT_ENTRY"
mk "${root}/${RUN}" 'fn refuse(d: ConfigDiagnostic) -> anyhow::Result<()> { Err(d.into()) }'
run_gate
expect_green "a diagnostic raised in commands/run.rs needs no entry"
new_case 10c "$PORT" "$PORT_ENTRY
$RUN | Input file not found: {path} | (command line) | the named input does not exist"
mk "${root}/${RUN}" 'fn f() {}'
run_gate
expect_red "Input file not found" "a stale commands/run.rs entry is named"

echo
if [ "$failures" -ne 0 ]; then
  echo "config_refusals_test: ${failures} case(s) failed"
  exit 1
fi
echo "config_refusals_test: all cases passed"
