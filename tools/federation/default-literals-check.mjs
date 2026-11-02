// Hold FraiseQL's published default values to graphql-js (#1888).
//
// A default is published twice: in the SDL (`field: Type = <literal>`) and as
// introspection's `__InputValue.defaultValue`. Both must be GraphQL literals of the
// declared type. graphql-js does not reject one that is not: `buildSchema` and
// `buildClientSchema` coerce each default with `valueFromAST` and silently leave
// `defaultValue` undefined when it does not coerce (a JSON string `"ACTIVE"` for an
// enum), and `parseValue` throws on a JSON object (`{"k":1}`). So this checks that
// every published default survives as a defined value, and that an operation
// omitting a defaulted non-null field validates.
//
// Usage:
//   node default-literals-check.mjs <schema.graphql> <introspection.json> <operation>
//
// Exit codes: 0 = every default kept and the operation valid, 1 = a check failed,
// 2 = bad invocation.

import { readFileSync } from "node:fs";
import {
  buildClientSchema,
  buildSchema,
  isInputObjectType,
  isObjectType,
  parse,
  validate,
} from "graphql";

const [sdlFile, introspectionFile, operation] = process.argv.slice(2);
if (!sdlFile || !introspectionFile || !operation) {
  console.error("usage: default-literals-check.mjs <schema.graphql> <introspection.json> <operation>");
  process.exit(2);
}

const failures = [];

// Every input field and argument of `schema`, with the default graphql-js kept for it.
function inputValues(schema) {
  const out = [];
  for (const type of Object.values(schema.getTypeMap())) {
    if (type.name.startsWith("__")) continue;
    if (isInputObjectType(type)) {
      for (const field of Object.values(type.getFields())) {
        out.push({ site: `${type.name}.${field.name}`, value: field.defaultValue });
      }
    } else if (isObjectType(type)) {
      for (const field of Object.values(type.getFields())) {
        for (const arg of field.args) {
          out.push({ site: `${type.name}.${field.name}(${arg.name}:)`, value: arg.defaultValue });
        }
      }
    }
  }
  return out;
}

// SDL: each `= <literal>` the file declares must come back as a defined default.
const sdl = readFileSync(sdlFile, "utf8");
const declared = new Set();
let owner = null;
for (const line of sdl.split("\n")) {
  const head = line.match(/^(?:input|type) (\w+)/);
  if (head) owner = head[1];
  const field = line.match(/^ {2}(\w+)(?:\(([^)]*)\))?: [^=]*?( = .+)?$/);
  if (!owner || !field) continue;
  if (field[3]) declared.add(`${owner}.${field[1]}`);
  for (const arg of (field[2] ?? "").matchAll(/(\w+): [^,=]+ = /g)) {
    declared.add(`${owner}.${field[1]}(${arg[1]}:)`);
  }
}
if (declared.size === 0) failures.push(`${sdlFile} declares no default: nothing to check`);

let built;
try {
  built = buildSchema(sdl);
} catch (e) {
  failures.push(`buildSchema refused ${sdlFile}: ${e.message}`);
}
if (built) {
  const kept = new Map(inputValues(built).map((v) => [v.site, v.value]));
  for (const site of declared) {
    if (kept.get(site) === undefined) failures.push(`SDL default of ${site} did not coerce to its type`);
  }
  const errors = validate(built, parse(operation));
  for (const e of errors) failures.push(`operation omitting defaulted fields is invalid: ${e.message}`);
  console.log(`SDL: ${declared.size} defaults kept by buildSchema; operation checked`);
}

// Introspection: each non-null `defaultValue` must parse and coerce.
const introspection = JSON.parse(readFileSync(introspectionFile, "utf8"));
const published = [];
for (const type of introspection.__schema.types) {
  for (const f of type.inputFields ?? []) {
    if (f.defaultValue != null) published.push(`${type.name}.${f.name}`);
  }
  for (const field of type.fields ?? []) {
    for (const a of field.args ?? []) {
      if (a.defaultValue != null) published.push(`${type.name}.${field.name}(${a.name}:)`);
    }
  }
}
let client;
try {
  client = buildClientSchema(introspection);
} catch (e) {
  failures.push(`buildClientSchema refused ${introspectionFile}: ${e.message}`);
}
if (client) {
  const kept = new Map(inputValues(client).map((v) => [v.site, v.value]));
  for (const site of published) {
    if (kept.get(site) === undefined) failures.push(`defaultValue of ${site} did not coerce to its type`);
  }
  console.log(`introspection: ${published.length} defaultValues kept by buildClientSchema`);
}

if (failures.length > 0) {
  console.error(`✗ default literals: ${failures.length} problem(s)`);
  for (const f of failures) console.error(`  - ${f}`);
  process.exit(1);
}
console.log("✓ default literals: every published default is a literal of its type");
