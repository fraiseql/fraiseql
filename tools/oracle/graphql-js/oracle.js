"use strict";
// graphql-js differential oracle.
//
// FraiseQL (at --server) and graphql-js (started here, over fixture/schema.graphql and
// the same database rows) answer the same documents: the targeted cases in
// lib/cases.js and a seeded random corpus from lib/fuzz.js. The oracle also diffs the
// two servers' introspection and runs the graphql-http audit against FraiseQL.
//
// Every disagreement is compared with baseline.json, in both directions:
//   - a disagreement the baseline does not list is a regression: exit 1;
//   - a listed disagreement that no longer occurs is progress the baseline must
//     record: exit 1 until `--bless` rewrites it.
// So the baseline only ever shrinks toward full agreement, and every change to it is
// a reviewed diff.
//
// usage: node oracle.js --server <url> --database <postgres-url>
//                       [--baseline <path>] [--report <path>] [--bless] [--bless-sdl]
// ORACLE_PRIVATE_MODULE may name a module, kept outside the repository, exporting
// `{ cases, mutations }`: extra targeted cases, and replacements for fuzz mutation
// slots by name. Such a run is measured against its own --baseline.
// Exit codes: 0 agreement with the baseline, 1 the gate failed, 2 the harness failed.

const fs = require("node:fs");
const path = require("node:path");
const { parseArgs } = require("node:util");
const { buildClientSchema, buildSchema, parse, printSchema, validate } = require("graphql");
const { auditServer } = require("graphql-http");
const { compare, fingerprint, hasErrors, send } = require("./lib/compare.js");
const { generate, MUTATIONS } = require("./lib/fuzz.js");
const { introspectionDiff, QUERY } = require("./lib/introspection.js");
const { startReference } = require("./lib/reference.js");
const { startProxy } = require("./lib/proxy.js");
const publicCases = require("./lib/cases.js");

const SEEDS = [11, 12];
const FUZZ_PER_SEED = 4000;
const WORKERS = 16;
// Disagreements the spec leaves to the implementation, or that this oracle does not
// yet rank: error locations, error count, and non-standard top-level error keys.
const IGNORED = new Set(["err-locations", "err-toplevel-keys", "err-count"]);

const here = __dirname;
const SDL_PATH = path.join(here, "fixture", "schema.graphql");

const { values: opt } = parseArgs({
  options: {
    server: { type: "string" },
    database: { type: "string" },
    baseline: { type: "string", default: path.join(here, "baseline.json") },
    report: { type: "string" },
    bless: { type: "boolean", default: false },
    "bless-sdl": { type: "boolean", default: false },
    selftest: { type: "boolean", default: false },
  },
});

function loadPrivate() {
  const p = process.env.ORACLE_PRIVATE_MODULE;
  if (!p) return { cases: [], mutations: MUTATIONS, label: "public" };
  const m = require(path.resolve(p));
  const mutations = MUTATIONS.map((slot) => (m.mutations && m.mutations[slot.name]) || slot);
  return { cases: m.cases || [], mutations, label: `private:${path.basename(p)}` };
}

async function blessSdl(server) {
  const r = await send(server, { query: QUERY });
  if (!r.body.data) throw new Error(`introspection failed: ${JSON.stringify(r.body).slice(0, 500)}`);
  fs.writeFileSync(SDL_PATH, `${printSchema(buildClientSchema(r.body.data))}\n`);
  console.log(`wrote ${path.relative(process.cwd(), SDL_PATH)} from the server's introspection`);
}

async function runAll(cases, fqUrl, refUrl, schema) {
  const results = new Array(cases.length);
  let next = 0;
  async function worker() {
    while (next < cases.length) {
      const i = next++;
      const c = cases[i];
      const body = { query: c.query };
      if (c.variables !== undefined) body.variables = c.variables;
      if (c.operationName !== undefined) body.operationName = c.operationName;
      const [fq, ref] = await Promise.all([send(fqUrl, body), send(refUrl, body)]);
      let valid;
      try {
        valid = validate(schema, parse(c.query)).length === 0;
      } catch {
        valid = false;
      }
      const issues = compare(fq, ref).filter((x) => !IGNORED.has(x.split(":")[0]));
      results[i] = { case: c, fp: fingerprint(c), valid, issues, fq, ref };
    }
  }
  await Promise.all(Array.from({ length: WORKERS }, worker));
  return results;
}

async function audit(url) {
  const results = await auditServer({ url });
  return results
    .filter((r) => r.status === "warn" || r.status === "error")
    .map((r) => `${r.status.toUpperCase()} ${r.id} ${r.name}`)
    .sort();
}

function setDiff(now, before) {
  const b = new Set(before);
  const n = new Set(now);
  return { added: [...n].filter((x) => !b.has(x)), removed: [...b].filter((x) => !n.has(x)) };
}

const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : "n/a");
const clip = (v, n = 400) => JSON.stringify(v).slice(0, n);

function showCase(r) {
  const c = r.case;
  console.log(`  ${c.name} [${r.fp}] ${r.valid ? "valid" : "invalid"}: ${r.issues.join(",")}`);
  console.log(`    query: ${c.query}`);
  if (c.variables !== undefined) console.log(`    variables: ${clip(c.variables)}`);
  if (c.operationName !== undefined) console.log(`    operationName: ${clip(c.operationName)}`);
  console.log(`    FQ  (${r.fq.status}): ${clip(r.fq.body)}`);
  console.log(`    REF (${r.ref.status}): ${clip(r.ref.body)}`);
}

/** Runs every document against both servers; returns the measured state and the failing results by key. */
async function measure(server, database, priv) {
  const sdl = fs.readFileSync(SDL_PATH, "utf8");
  const schema = buildSchema(sdl);
  const targeted = [...publicCases, ...priv.cases].map((c) => ({ ...c, name: `case:${c.name}` }));
  const fuzz = SEEDS.flatMap((s) => generate(FUZZ_PER_SEED, s, priv.mutations));

  const ref = await startReference({ sdl, databaseUrl: database });
  let results;
  let intro;
  let auditLines;
  try {
    results = await runAll([...targeted, ...fuzz], server, ref.url, schema);
    intro = await introspectionDiff(server, ref.url);
    auditLines = await audit(server);
  } finally {
    ref.close();
  }

  const t = results.slice(0, targeted.length);
  const f = results.slice(targeted.length);
  const fv = f.filter((r) => r.valid);
  const fi = f.filter((r) => !r.valid);
  const metrics = {
    targeted_agree: t.filter((r) => !r.issues.length).length,
    targeted_total: t.length,
    fuzz_valid_agree: fv.filter((r) => !r.issues.length).length,
    fuzz_valid_total: fv.length,
    fuzz_invalid_executed: fi.filter((r) => !hasErrors(r.fq)).length,
    fuzz_invalid_total: fi.length,
    introspection_diff_lines: intro.length,
    audit_warn: auditLines.filter((l) => l.startsWith("WARN")).length,
    audit_error: auditLines.filter((l) => l.startsWith("ERROR")).length,
  };
  const failing = {};
  for (const r of results) if (r.issues.length) failing[`${r.fp} ${r.issues.join(",")}`] = r;
  const current = {
    corpus: { label: priv.label, seeds: SEEDS, fuzz_per_seed: FUZZ_PER_SEED, targeted: targeted.length },
    metrics,
    disagreements: Object.keys(failing).sort(),
    introspection: intro,
    audit: auditLines,
  };
  return { current, failing };
}

function printMetrics(label, m) {
  console.log(`graphql-js oracle (${label})`);
  console.log(`  targeted cases agreeing:      ${m.targeted_agree}/${m.targeted_total}`);
  console.log(
    `  valid documents agreeing:     ${m.fuzz_valid_agree}/${m.fuzz_valid_total} (${pct(m.fuzz_valid_agree, m.fuzz_valid_total)})`,
  );
  console.log(
    `  invalid documents executed:   ${m.fuzz_invalid_executed}/${m.fuzz_invalid_total} (${pct(m.fuzz_invalid_executed, m.fuzz_invalid_total)})`,
  );
  console.log(`  introspection diff lines:     ${m.introspection_diff_lines}`);
  console.log(`  graphql-http audit:           ${m.audit_warn} warn, ${m.audit_error} error`);
}

/**
 * Compares a measurement with a baseline. Pure: returns what it found, prints nothing.
 * `ok` holds only when the two are equal in every direction.
 */
function gate(current, base) {
  if (canonCorpus(base.corpus) !== canonCorpus(current.corpus)) {
    return { ok: false, corpusChanged: true, regressions: {}, progress: {} };
  }
  const regressions = {};
  const progress = {};
  for (const key of ["disagreements", "introspection", "audit"]) {
    const d = setDiff(current[key], base[key] || []);
    regressions[key] = d.added;
    progress[key] = d.removed;
  }
  const count = (o) => Object.values(o).reduce((n, xs) => n + xs.length, 0);
  return { ok: count(regressions) === 0 && count(progress) === 0, corpusChanged: false, regressions, progress, count };
}

function printGate(g, current, base, failing) {
  if (g.corpusChanged) {
    console.log(`FAIL: the corpus changed (${clip(base.corpus)} -> ${clip(current.corpus)}); re-bless the baseline`);
    return;
  }
  const r = g.regressions;
  const p = g.progress;
  if (r.disagreements.length) {
    console.log(`\nREGRESSION: ${r.disagreements.length} disagreement(s) not in the baseline:`);
    for (const k of r.disagreements) showCase(failing[k]);
  }
  for (const l of r.introspection) console.log(`REGRESSION: introspection: ${l}`);
  for (const l of r.audit) console.log(`REGRESSION: audit: ${l}`);
  const progress = g.count(p);
  if (progress) {
    console.log(`\nPROGRESS not yet recorded: ${progress} baseline entr${progress === 1 ? "y" : "ies"} no longer occur:`);
    for (const k of p.disagreements) console.log(`  ${k}`);
    for (const l of p.introspection) console.log(`  introspection: ${l}`);
    for (const l of p.audit) console.log(`  audit: ${l}`);
    console.log("Record it: rerun with --bless and commit baseline.json with the change.");
  }
  if (g.ok) console.log("\nOK: agreement matches the baseline exactly");
  else console.log(`\nFAIL: ${g.count(r)} regression(s), ${progress} unrecorded improvement(s)`);
}

const readBaseline = (p) => JSON.parse(fs.readFileSync(p, "utf8"));

/**
 * Proves the gate goes RED. On a server that matches the baseline it checks that:
 *  - each gate site (disagreements, introspection, audit, in both directions, and the
 *    corpus) fails when the baseline is changed by exactly one entry, naming that entry;
 *  - a server that starts executing one class of invalid document (simulated by a proxy
 *    that answers those requests with data) is reported, and only that class is;
 *  - a server that returns one wrong value is reported, and only those documents are.
 */
async function selftest(server, database, priv, base) {
  const checks = [];
  const check = (name, ok, detail = "") => {
    checks.push(ok);
    console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? `: ${detail}` : ""}`);
  };
  const only = (g, key, expected) =>
    !g.ok &&
    !g.corpusChanged &&
    ["regressions", "progress"].every((side) =>
      ["disagreements", "introspection", "audit"].every((k) =>
        side === key[0] && k === key[1] ? canon(g[side][k]) === canon(expected) : g[side][k].length === 0,
      ),
    );
  const { canon } = require("./lib/compare.js");

  const { current } = await measure(server, database, priv);
  const g0 = gate(current, base);
  if (!g0.ok) throw new Error("the self-test needs a server that matches the baseline; run the oracle first");
  check("unchanged server matches the baseline", g0.ok);

  for (const k of ["disagreements", "introspection", "audit"]) {
    const entry = base[k][0];
    if (entry !== undefined) {
      const dropped = { ...base, [k]: base[k].slice(1) };
      check(`${k}: an entry missing from the baseline is a regression`, only(gate(current, dropped), ["regressions", k], [entry]));
    } else check(`${k}: the baseline has an entry to remove`, false, "empty");
    const fake = `selftest-${k}-entry`;
    const extra = { ...base, [k]: [...base[k], fake] };
    check(`${k}: a baseline entry that no longer occurs is unrecorded progress`, only(gate(current, extra), ["progress", k], [fake]));
  }
  const otherCorpus = { ...base, corpus: { ...base.corpus, seeds: [...base.corpus.seeds, 1] } };
  check("a changed corpus fails", gate(current, otherCorpus).corpusChanged === true);

  async function viaProxy(name, rewrite, marker) {
    const proxy = await startProxy(server, rewrite);
    let m;
    try {
      m = await measure(proxy.url, database, priv);
    } finally {
      proxy.close();
    }
    const g = gate(m.current, base);
    const regs = g.regressions.disagreements || [];
    const allMarked = regs.every((k) => marker(m.failing[k]));
    // A document that already disagreed and now disagrees differently is listed under
    // both regressions (new issue set) and progress (old one); only those may appear
    // as progress.
    const fp = (k) => k.split(" ")[0];
    const regFps = new Set(regs.map(fp));
    const nothingElse =
      !g.corpusChanged &&
      g.regressions.introspection.length === 0 &&
      g.regressions.audit.length === 0 &&
      g.progress.introspection.length === 0 &&
      g.progress.audit.length === 0 &&
      g.progress.disagreements.every((k) => regFps.has(fp(k)));
    check(name, !g.ok && regs.length > 0 && allMarked && nothingElse, `${regs.length} regression(s)`);
  }

  // A server that stops rejecting unknown directives (§ 5.7.1).
  await viaProxy(
    "a server executing documents with an unknown directive is reported, and nothing else",
    (req, res) => (typeof req.query === "string" && req.query.includes(" @zz") && hasErrors({ body: res }) ? { data: {} } : undefined),
    (r) => r.case.query.includes(" @zz") && r.issues.includes("REF-only-error"),
  );
  // A server that answers one wrong value.
  await viaProxy(
    "a server returning one wrong value is reported, and nothing else",
    (req, res) => {
      const text = JSON.stringify(res);
      return res.data && text.includes('"Post 1"') ? JSON.parse(text.replaceAll('"Post 1"', '"Post 1 (altered)"')) : undefined;
    },
    (r) => JSON.stringify(r.fq.body).includes("Post 1 (altered)") && r.issues.includes("data"),
  );

  const failed = checks.filter((ok) => !ok).length;
  console.log(failed ? `\nSELFTEST FAIL: ${failed} of ${checks.length} checks` : `\nSELFTEST OK: ${checks.length} checks`);
  return failed ? 1 : 0;
}

async function main() {
  if (!opt.server) throw new Error("--server is required");
  if (opt["bless-sdl"]) return blessSdl(opt.server);
  if (!opt.database) throw new Error("--database is required");
  const priv = loadPrivate();
  if (opt.selftest) return selftest(opt.server, opt.database, priv, readBaseline(opt.baseline));

  const { current, failing } = await measure(opt.server, opt.database, priv);
  printMetrics(priv.label, current.metrics);
  if (opt.report) {
    const cases = Object.values(failing);
    fs.writeFileSync(opt.report, `${JSON.stringify({ ...current, cases }, null, 1)}\n`);
  }
  if (opt.bless) {
    fs.writeFileSync(opt.baseline, `${JSON.stringify(current, null, 1)}\n`);
    console.log(`blessed ${path.relative(process.cwd(), opt.baseline)}`);
    return 0;
  }
  if (!fs.existsSync(opt.baseline)) {
    console.log(`FAIL: no baseline at ${opt.baseline}; run with --bless to record one`);
    return 1;
  }
  const base = readBaseline(opt.baseline);
  const g = gate(current, base);
  printGate(g, current, base, failing);
  return g.ok ? 0 : 1;
}

const canonCorpus = (c) => JSON.stringify(c ?? null);

main().then(
  (code) => process.exit(code ?? 0),
  (err) => {
    console.error(`oracle harness error: ${err && err.stack ? err.stack : err}`);
    process.exit(2);
  },
);
