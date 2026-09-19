#!/usr/bin/env bash
# write_selection_sources_test.sh — the red capability of
# tools/check-write-selection-sources.py.
#
# None of what makes that gate worth having is visible from a passing run of the real
# tree: it passes, and it would pass just as quietly if every rule matched nothing.
# What has to be true is that each rule reddens on the shape it names, that each rule
# STAYS GREEN on the adjacent shape that is correct — an empty `inline_arguments`, an
# error message that begins with the word "mutation", a `#[cfg(test)]` module, a doc
# comment quoting the defect — and that the gate refuses a tree it can no longer see.
#
# The twins are the point. #1352's fix would have looked identical under a gate that
# flagged any `&[]` in the call, and that gate would have gone red on
# `(…, &selections, &[])`, which is the correct code.
set -uo pipefail

repo_root="$(git rev-parse --show-toplevel)"
gate="${repo_root}/tools/check-write-selection-sources.py"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

failures=0
pass() { echo "  ok   $1"; }
fail() { echo "  FAIL $1"; failures=$((failures + 1)); sed 's/^/       /' "${tmp}/out"; }

run_gate() {
  python3 "$gate" --root "$1" > "${tmp}/out" 2>&1
  echo "$?" > "${tmp}/rc"
}

rc() { cat "${tmp}/rc"; }
said() { grep -F "$1" "${tmp}/out" >/dev/null; }

# A tree shaped like this repository's: the engine's write entries, the helper, one
# transport outside the engine that calls an entry, and the two federation namesakes.
fixture() {
  local root="$1"
  mkdir -p "${root}/crates/fraiseql-core/src/runtime/executor/runners/mutation"
  mkdir -p "${root}/crates/fraiseql-server/src/routes/grpc"
  mkdir -p "${root}/crates/fraiseql-federation/src/saga_executor"

  cat > "${root}/crates/fraiseql-core/src/runtime/executor/mutation.rs" <<'RS'
impl<A: DatabaseAdapter + SupportsMutations> Executor<A> {
    pub async fn execute_mutation(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        selections: &[FieldSelection],
    ) -> Result<serde_json::Value> {
        self.mutation_runner().execute_mutation(mutation_name, variables, selections).await
    }

    pub async fn execute_mutation_as(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        selections: &[FieldSelection],
    ) -> Result<MutationExecution> {
        self.execute_mutation_detailed(
            mutation_name,
            mutation_name,
            variables,
            security_context,
            selections,
            &[],
        )
        .await
    }

    pub(super) async fn execute_mutation_detailed(
        &self,
        mutation_name: &str,
        response_key: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        selections: &[FieldSelection],
        inline_arguments: &[GraphQLArgument],
    ) -> Result<MutationExecution> {
        runners::mutation::execute_mutation_impl(
            &self.ctx,
            mutation_name,
            response_key,
            variables,
            security_context,
            selections,
            inline_arguments,
        )
        .await
    }
}

pub fn mutation_return_selections(
    schema: &CompiledSchema,
    mutation_name: &str,
) -> Vec<FieldSelection> {
    derive(schema, mutation_name)
}
RS

  cat > "${root}/crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs" <<'RS'
pub async fn execute_mutation_impl(
    ctx: &ExecutorContext,
    mutation_name: &str,
    response_key: &str,
    variables: Option<&serde_json::Value>,
    security_context: Option<&SecurityContext>,
    selections: &[FieldSelection],
    inline_arguments: &[GraphQLArgument],
) -> Result<MutationExecution> {
    dispatch(ctx, mutation_name, response_key, variables, security_context, selections, inline_arguments)
}
RS

  cat > "${root}/crates/fraiseql-server/src/routes/grpc/handler.rs" <<'RS'
async fn mutate(executor: &Executor<A>, mutation_name: &str) -> Result<MutationResponse> {
    let selections =
        fraiseql_core::runtime::mutation_return_selections(executor.schema(), mutation_name);
    executor
        .execute_mutation_as(mutation_name, Some(&variables), security_context, &selections)
        .await
}
RS

  for f in "${root}/crates/fraiseql-federation/src/saga_compensator.rs" \
           "${root}/crates/fraiseql-federation/src/saga_executor/step.rs"; do
    cat > "$f" <<'RS'
async fn compensate(client: &HttpMutationClient) -> Result<Value> {
    client
        .execute_mutation(
            url.as_str(),
            &step.typename,
            mutation,
            variables,
            mutation_executor.metadata(),
            Some(&idempotency_key),
        )
        .await
}
RS
  done
}

new_tree() {
  local root="${tmp}/$1"
  rm -rf "$root"
  mkdir -p "$root"
  fixture "$root"
  echo "$root"
}

grpc="crates/fraiseql-server/src/routes/grpc/handler.rs"
entries="crates/fraiseql-core/src/runtime/executor/mutation.rs"

# ── 1. The fixture, unmutated, passes ────────────────────────────────────────────────
root="$(new_tree baseline)"
run_gate "$root"
if [ "$(rc)" = "0" ] && said "OK: " && said "outside the engine"; then
  pass "a tree where every write derives its selection set passes"
else
  fail "the unmutated fixture should pass"
fi

# ── 2. Rule 1: an empty selection set in the SELECTIONS slot ─────────────────────────
# #1352's shape, as the REST anonymous arm spelled it.
root="$(new_tree empty_slot)"
sed -i 's|, security_context, &selections)|, security_context, \&[])|' "${root}/${grpc}"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "EMPTY selection set (#1352)"; then
  pass "an empty selection set at a write entry fails the gate"
else
  fail "an empty selection set at a write entry should fail the gate"
fi

# ── 3. THE TWIN: an empty `inline_arguments` in the NEXT slot is correct code ────────
# `execute_mutation_as` already passes `&[]` there, on every call, in the real tree. A
# gate that flagged an empty slice anywhere in the call would redden on the fix itself.
root="$(new_tree empty_inline)"
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "an empty inline_arguments slot is not read as an empty selection set"
else
  fail "the gate must be positional: (…, &selections, &[]) is correct code"
fi

# ── 4. Rule 1: the other spelling of empty ───────────────────────────────────────────
root="$(new_tree unwrap_default)"
sed -i 's|, security_context, &selections)|, security_context, \&derive(schema, name).unwrap_or_default())|' "${root}/${grpc}"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "EMPTY selection set (#1352)"; then
  pass "an inline unwrap_or_default() in the selections slot fails the gate"
else
  fail "unwrap_or_default() in the selections slot should fail the gate"
fi

# ── 5. Rule 2: a transport that derives its own selection set ────────────────────────
# The gRPC shape before this phase: its own copy of the derivation, one function away
# from the call site, so rule 1 could not see the emptiness.
root="$(new_tree own_derivation)"
sed -i 's|fraiseql_core::runtime::mutation_return_selections(executor.schema(), mutation_name)|return_type_selections(executor.schema(), mutation_name)|' "${root}/${grpc}"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "does not derive its selection set from the one helper"; then
  pass "a transport with its own selection-set derivation fails the gate"
else
  fail "a transport that does not call the helper should fail the gate"
fi

# ── 6. Rule 3: a GraphQL document built by formatting, on a request path ─────────────
# #1331's shape, verbatim.
root="$(new_tree formatted_document)"
cat >> "${root}/crates/fraiseql-server/src/routes/grpc/handler.rs" <<'RS'

fn synthesise(mutation_name: &str, args_str: &str, fields: &str) -> String {
    format!("mutation {{ {mutation_name}({args_str}) {{ {fields} }} }}")
}
RS
run_gate "$root"
if [ "$(rc)" = "1" ] && said "built by string formatting on a request path (#1331)"; then
  pass "a GraphQL document built by format! on a request path fails the gate"
else
  fail "a formatted GraphQL document on a request path should fail the gate"
fi

# ── 7. THE TWIN: an error message that begins with the word "mutation" ───────────────
# `routes/before_mutation/mod.rs` carries exactly this, and the gate's first draft went
# red on it: the keyword alone is not a document, the selection set that follows is.
root="$(new_tree message_not_document)"
cat >> "${root}/crates/fraiseql-server/src/routes/grpc/handler.rs" <<'RS'

fn refusal(name: &str) -> String {
    format!("mutation `{}` declares a before:mutation chain, but this build has none", name)
}
RS
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "an error message opening with the word mutation is not read as a document"
else
  fail "prose beginning with a GraphQL keyword must not fail the gate"
fi

# ── 8. An inline #[cfg(test)] module may pass &[] ────────────────────────────────────
# `runtime/executor/security.rs` does, deliberately. Excluding test FILES is not enough.
root="$(new_tree inline_test_module)"
cat >> "${root}/${entries}" <<'RS'

#[cfg(test)]
mod enrichment_entry_point_tests {
    use super::*;

    #[tokio::test]
    async fn the_mutation_chokepoint_refuses_an_unresolved_principal() {
        let err = executor()
            .execute_mutation_as("createOrder", None, Some(&unresolved()), &[])
            .await
            .expect_err("an unresolved principal must not write");
        assert!(is_refusal(&err));
    }
}
RS
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "an inline #[cfg(test)] module passing &[] does not fail the gate"
else
  fail "a #[cfg(test)] module is not production code"
fi

# ── 9. A comment quoting the defect is prose ─────────────────────────────────────────
# This gate's own header, and `execute_mutation_with_security`'s rustdoc, both quote the
# format! that #1331 was. A gate that counted prose would go red on its own explanation.
root="$(new_tree comment_quoting_defect)"
cat >> "${root}/crates/fraiseql-server/src/routes/grpc/handler.rs" <<'RS'

/// It used to reach the engine as `format!("mutation {{ {name}({args}) {{ {f} }} }}")`,
/// a round-trip through text that could not represent every JSON value (#1331).
// Nor does a line comment: format!("mutation {{ {name} }}")
fn documented() {}
RS
run_gate "$root"
if [ "$(rc)" = "0" ]; then
  pass "a comment quoting a formatted document does not fail the gate"
else
  fail "a comment quoting the defect is prose, not a request path"
fi

# ── 10. A renamed write entry blinds the gate, so the gate refuses ───────────────────
root="$(new_tree renamed_entry)"
sed -i 's|pub async fn execute_mutation_as(|pub async fn execute_mutation_for(|' "${root}/${entries}"
sed -i 's|\.execute_mutation_as(|.execute_mutation_for(|' "${root}/${grpc}"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "no longer exists"; then
  pass "a renamed write entry fails the gate rather than silently unpinning it"
else
  fail "a renamed write entry should fail the gate"
fi

# ── 11. The helper rule 2 names must exist ───────────────────────────────────────────
root="$(new_tree no_helper)"
sed -i 's|pub fn mutation_return_selections(|pub fn derive_return_selections(|' "${root}/${entries}"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "no longer exists"; then
  pass "a missing selection-set helper fails the gate"
else
  fail "rule 2 without its helper has no subject and must fail"
fi

# ── 12. Rule 2 pinning nothing is a failure, not a pass ──────────────────────────────
root="$(new_tree no_external_caller)"
rm "${root}/${grpc}"
mkdir -p "${root}/crates/fraiseql-server/src/routes"
echo 'fn health() {}' > "${root}/crates/fraiseql-server/src/routes/health.rs"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "Rule 2 is pinning nothing"; then
  pass "a tree where rule 2 pins nothing fails rather than reporting OK"
else
  fail "rule 2 with no subject should fail"
fi

# ── 13. Rule 3 scanning nothing under a scope is a failure ───────────────────────────
# Per prefix: `crates/fraiseql-core/src/runtime/` always has files, so a combined count
# would stay non-zero while every transport had moved out from under the rule.
root="$(new_tree scope_moved)"
mkdir -p "${root}/crates/fraiseql-server/src/api/grpc"
mv "${root}/${grpc}" "${root}/crates/fraiseql-server/src/api/grpc/handler.rs"
rm -rf "${root}/crates/fraiseql-server/src/routes"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "rule 3 scanned no files under crates/fraiseql-server/src/routes/"; then
  pass "a moved transport scope fails the gate rather than leaving rule 3 vacuous"
else
  fail "rule 3 over an empty scope should fail"
fi

# ── 14. A NAMESAKE that stopped calling must not stay listed ─────────────────────────
root="$(new_tree stale_namesake)"
: > "${root}/crates/fraiseql-federation/src/saga_executor/step.rs"
run_gate "$root"
if [ "$(rc)" = "1" ] && said "no longer calls"; then
  pass "a stale NAMESAKES entry fails the gate"
else
  fail "a NAMESAKES entry that excuses nothing would excuse a real call added later"
fi

# ── 15. An unlisted namesake is reported, not guessed at ─────────────────────────────
root="$(new_tree unlisted_namesake)"
mkdir -p "${root}/crates/fraiseql-federation/src"
cat > "${root}/crates/fraiseql-federation/src/other_client.rs" <<'RS'
async fn forward(client: &HttpMutationClient) -> Result<Value> {
    client.execute_mutation(url, typename, name, vars, metadata, key).await
}
RS
run_gate "$root"
if [ "$(rc)" = "1" ] && said "argument count it does not take"; then
  pass "a call sharing an entry's name with another arity is reported, not guessed"
else
  fail "an unlisted namesake should fail the gate"
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: ${failures} check-write-selection-sources.py assertion(s) failed"
  exit 1
fi
echo "OK: check-write-selection-sources.py goes red on each shape it names, and stays green on their twins."
