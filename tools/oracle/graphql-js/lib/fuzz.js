"use strict";
// Seeded random document generator over the fixture schema.
//
// `generate(n, seed, mutations)` returns `n` cases `{ name, query, variables, tags }`.
// Documents are valid by construction; about 15% are then mutated into (usually)
// invalid ones, tagged `mut:<name>`. The generator is a pure function of
// (n, seed, mutations): the same inputs give the same documents on every machine,
// which is what lets a baseline name failing cases by fingerprint.
//
// `mutations` is an ordered list of `{ name, apply(query) => query }`. The slot a
// case draws is chosen by index, so replacing one entry keeps every other case
// identical.

const POST_IDS = [
  "00000001-1111-4000-8000-000000000000",
  "00000004-1111-4000-8000-000000000000",
  "00000099-1111-4000-8000-000000000000",
];
const AUTHOR_IDS = ["00000002-0000-4000-8000-000000000000", "00000003-0000-4000-8000-000000000000"];
const globalId = (s) => Buffer.from(s).toString("base64");

/** The public mutation slots, in draw order. Each breaks one validation rule. */
const MUTATIONS = [
  // § 5.3.1 field selections: a field the type does not define.
  { name: "unknownField", apply: (q) => q.replace(/\{ (id|title|name)\b/, (_, f) => `{ zzz ${f}`) },
  // § 5.3.3 leaf field selections: a selection set on a scalar.
  { name: "leafSub", apply: (q) => q.replace(/\b(title|name)\b(?! *:)/, (_, f) => `${f} { x }`) },
  // § 5.2.1.1 operation name uniqueness.
  { name: "dupOp", apply: (q) => `${q} query Q { __typename }` },
  // § 5.5.1.4 fragments must be used.
  { name: "unusedFrag", apply: (q) => `${q} fragment Unused on Post { id }` },
  // § 5.7.1 directives are defined.
  { name: "unknownDir", apply: (q) => q.replace(/\b(id|title|name)\b(?! *:)/, (_, f) => `${f} @zz`) },
  // § 5.4.2 argument uniqueness.
  {
    name: "dupArg",
    apply: (q) => q.replace(/posts\(/, "posts(limit: 1, ").replace(/authors\(/, "authors(limit: 1, "),
  },
  // § 5.6.1 values of correct type.
  { name: "badArgType", apply: (q) => q.replace(/limit: (\d)/, (_, d) => `limit: "${d}"`) },
  // § 5.4.1 argument names.
  { name: "unknownArg", apply: (q) => q.replace(/\b(title|name)\b(?! *:)/, (_, f) => `${f}(x: 1)`) },
];

function generate(count, seed, mutations = MUTATIONS) {
  // xorshift32: small, portable, and identical across node versions.
  let state = seed >>> 0 || 1;
  const rnd = () => {
    state ^= state << 13;
    state >>>= 0;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    return state / 4294967296;
  };
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const chance = (p) => rnd() < p;
  const int = (a, b) => a + Math.floor(rnd() * (b - a + 1));
  const TYPES = {
    Post: {
      leaf: ["id", "title", "body", "views", "published", "createdAt", "tags", "__typename"],
      obj: { author: "Author" },
    },
    Author: { leaf: ["id", "name", "email", "age", "rating", "role", "nicknames", "__typename"], obj: {} },
    Node: { leaf: ["id", "__typename"], obj: {} },
  };
  const ALIASES = ["a", "b", "x", "id", "name", "title"];

  return Array.from({ length: count }, (_, i) => {
    const tags = new Set();
    const vars = {};
    const varDefs = [];
    const frags = [];
    let varCount = 0;
    const newVar = (type, value, absent) => {
      const n = `v${varCount++}`;
      varDefs.push(`$${n}: ${type}`);
      if (!absent) vars[n] = value;
      return `$${n}`;
    };

    function directive() {
      if (!chance(0.25)) return "";
      const kind = pick(["skip", "include"]);
      let cond;
      if (chance(0.5)) {
        cond = pick(["true", "false"]);
        tags.add("dir:lit");
      } else {
        const r = rnd();
        if (r < 0.7) cond = newVar("Boolean!", chance(0.5));
        else if (r < 0.85) {
          cond = newVar(`Boolean! = ${pick(["true", "false"])}`, undefined, true);
          tags.add("dir:var-default");
        } else {
          cond = newVar("Boolean = true", chance(0.5));
          tags.add("dir:var-nullable-default");
        }
        tags.add("dir:var");
      }
      tags.add(`dir:${kind}`);
      return ` @${kind}(if: ${cond})`;
    }

    function selection(type, depth) {
      const out = [];
      const n = int(1, 4);
      for (let k = 0; k < n; k++) {
        const r = rnd();
        const t = TYPES[type];
        if (r < 0.55 || depth > 3) {
          const f = pick(t.leaf);
          let alias = "";
          if (chance(0.08)) {
            alias = `${pick(ALIASES)}: `;
            tags.add("alias");
          }
          if (f === "__typename") tags.add("typename");
          out.push(alias + f + directive());
        } else if (r < 0.7 && Object.keys(t.obj).length) {
          const [f, ft] = pick(Object.entries(t.obj));
          tags.add("nested");
          out.push(`${chance(0.08) ? `${pick(ALIASES)}: ` : ""}${f}${directive()} { ${selection(ft, depth + 1)} }`);
        } else if (r < 0.82) {
          const on = type === "Node" ? pick(["Post", "Author", "Node", null]) : pick([type, "Node", null]);
          tags.add(`inline:${on || "untyped"}`);
          const inner = on === null ? type : on;
          out.push(`...${on ? ` on ${on}` : ""}${directive()} { ${selection(inner, depth + 1)} }`);
        } else {
          const on = type === "Node" ? pick(["Post", "Author", "Node"]) : pick([type, "Node"]);
          const name = `F${frags.length}`;
          frags.push(null);
          frags[frags.length - 1] = `fragment ${name} on ${on} { ${selection(on, depth + 1)} }`;
          tags.add(`frag:${on}`);
          out.push(`...${name}${directive()}`);
        }
      }
      return out.join(" ");
    }

    function listArgs(type) {
      const a = [];
      if (chance(0.5)) {
        const v = int(0, 9);
        if (chance(0.4)) {
          tags.add("var:Int");
          a.push(`limit: ${chance(0.2) ? newVar("Int", null, chance(0.5)) : newVar(pick(["Int", "Int!"]), v)}`);
        } else a.push(`limit: ${v}`);
      }
      if (chance(0.3)) a.push(`offset: ${int(0, 4)}`);
      if (chance(0.4)) {
        const fields = type === "Post" ? ["views", "title", "createdAt", "id"] : ["name", "rating", "age", "role"];
        const one = `{ field: ${pick(fields)}${chance(0.6) ? `, direction: ${pick(["ASC", "DESC"])}` : ""} }`;
        if (chance(0.3)) {
          tags.add("var:orderBy");
          a.push(
            `orderBy: ${newVar(`[${type}OrderByInput!]`, [{ field: pick(fields), direction: pick(["ASC", "DESC"]) }])}`,
          );
        } else if (chance(0.3)) {
          tags.add("list-coerce");
          a.push(`orderBy: ${one}`);
        } else a.push(`orderBy: [${one}]`);
      }
      if (chance(0.4)) {
        const preds =
          type === "Post"
            ? [
                () => `views: { gt: ${int(0, 80)} }`,
                () => `views: { in: [${int(1, 8) * 10}, ${int(1, 8) * 10}] }`,
                () => `published: { eq: ${pick(["true", "false"])} }`,
                () => `title: { startswith: "Post ${int(1, 8)}" }`,
                () => `author: { name: { eq: "Author ${int(1, 5)}" } }`,
                () => {
                  tags.add("var:where-nested");
                  return `views: { lt: ${newVar("Int!", int(0, 90))} }`;
                },
              ]
            : [
                () => `age: { gt: ${int(20, 26)} }`,
                () => `role: { eq: ${pick(["ADMIN", "EDITOR", "READER"])} }`,
                () => `role: { in: [${pick(["ADMIN", "EDITOR"])}, READER] }`,
                () => `name: { eq: "Author ${int(1, 5)}" }`,
              ];
        const k = int(1, 2);
        const ps = Array.from({ length: k }, () => pick(preds)());
        a.push(chance(0.3) ? `where: { _or: [${ps.map((p) => `{ ${p} }`).join(", ")}] }` : `where: { ${ps.join(", ")} }`);
        tags.add("where");
      }
      return a.length ? `(${a.join(", ")})` : "";
    }

    function root() {
      const out = [];
      const n = int(1, 3);
      for (let k = 0; k < n; k++) {
        const alias = chance(0.4) ? `${pick(["a", "b", "c"])}: ` : "";
        const r = rnd();
        if (r < 0.35) out.push(`${alias}posts${listArgs("Post")}${directive()} { ${selection("Post", 1)} }`);
        else if (r < 0.55) out.push(`${alias}authors${listArgs("Author")}${directive()} { ${selection("Author", 1)} }`);
        else if (r < 0.7) {
          const id = pick(POST_IDS);
          out.push(
            `${alias}post(id: ${chance(0.3) ? newVar("ID!", id) : JSON.stringify(id)})${directive()} { ${selection("Post", 1)} }`,
          );
        } else if (r < 0.78) {
          out.push(`${alias}author(id: ${JSON.stringify(pick(AUTHOR_IDS))})${directive()} { ${selection("Author", 1)} }`);
        } else if (r < 0.9) {
          tags.add("node");
          const id = chance(0.5) ? globalId(`Post:${pick(POST_IDS)}`) : globalId(`Author:${pick(AUTHOR_IDS)}`);
          out.push(`${alias}node(id: ${JSON.stringify(id)})${directive()} { ${selection("Node", 1)} }`);
        } else {
          tags.add("typename-root");
          out.push(`${alias}__typename${directive()}`);
        }
      }
      return out.join(" ");
    }

    const body = root();
    let query = `query Q${varDefs.length ? `(${varDefs.join(", ")})` : ""} { ${body} } ${frags.join(" ")}`;
    if (chance(0.15)) {
      const m = pick(mutations);
      tags.add(`mut:${m.name}`);
      query = m.apply(query);
    }
    return { name: `fuzz:${seed}:${i}`, query, variables: vars, tags: [...tags] };
  });
}

module.exports = { generate, MUTATIONS };
