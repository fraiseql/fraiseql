// Compose FraiseQL-rendered subgraph SDLs with Apollo Federation v2 composition
// (`@apollo/composition`, the engine `rover supergraph compose` wraps) and fail on
// any composition error. This is the "real composer" half of the golden two-subgraph
// federation suite: the hermetic Rust test (crates/fraiseql-core/tests/federation_compose.rs)
// checks each subgraph's `_service` SDL invariants; this checks that the subgraphs
// actually compose into a supergraph together — the exact step CI never ran before
// (the old federation leg routed a *pre-composed, committed* single-subgraph fixture).
//
// Usage:
//   node compose-check.mjs <name>=<file.graphql> [<name>=<file.graphql> ...]
//   node compose-check.mjs --expect-fail=<CODE> <name>=<file> ...   # negative case
//
// Exit codes: 0 = composed as expected, 1 = unexpected result, 2 = bad invocation.

import { readFileSync } from "node:fs";
import { parse } from "graphql";
import { composeServices } from "@apollo/composition";

const raw = process.argv.slice(2);
let expectFailCode = null;
const specs = [];
for (const arg of raw) {
  if (arg.startsWith("--expect-fail=")) {
    expectFailCode = arg.slice("--expect-fail=".length);
    continue;
  }
  const eq = arg.indexOf("=");
  if (eq === -1) {
    console.error(`bad argument '${arg}' (expected <name>=<file>)`);
    process.exit(2);
  }
  specs.push({ name: arg.slice(0, eq), file: arg.slice(eq + 1) });
}

if (specs.length < 2) {
  console.error("usage: compose-check.mjs <name>=<file> <name>=<file> [...]");
  process.exit(2);
}

const services = specs.map(({ name, file }) => ({
  name,
  url: `http://localhost/${name}`,
  typeDefs: parse(readFileSync(file, "utf8")),
}));

const result = composeServices(services);
const errors = result.errors ?? [];
const names = specs.map((s) => s.name).join(" + ");

if (expectFailCode !== null) {
  // Negative case: composition MUST fail with the expected error code (e.g. two
  // change-log owners → INVALID_FIELD_SHARING).
  const codes = errors.map((e) => e.extensions?.code ?? "ERROR");
  if (codes.includes(expectFailCode)) {
    console.log(`✓ ${names}: composition correctly rejected with ${expectFailCode}`);
    process.exit(0);
  }
  console.error(
    `✗ ${names}: expected composition to fail with ${expectFailCode}, ` +
      (errors.length ? `got [${codes.join(", ")}]` : "but it composed cleanly"),
  );
  for (const e of errors) console.error(`    - [${e.extensions?.code ?? "ERROR"}] ${e.message}`);
  process.exit(1);
}

if (errors.length) {
  console.error(`✗ ${names}: composition FAILED with ${errors.length} error(s):`);
  for (const e of errors) console.error(`    - [${e.extensions?.code ?? "ERROR"}] ${e.message}`);
  process.exit(1);
}

const hints = result.hints ?? [];
console.log(`✓ ${names}: composed cleanly (${hints.length} hint(s))`);
for (const h of hints) console.log(`    · [${h.code ?? "HINT"}] ${h.message ?? h.toString()}`);
process.exit(0);
