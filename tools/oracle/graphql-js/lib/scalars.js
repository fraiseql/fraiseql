"use strict";
// The custom scalars the fixture SDL names, coercing as their `@specifiedBy`
// references define. graphql-js's `buildSchema` gives every custom scalar a
// pass-through coercion, which would make the reference accept values the scalar's
// definition refuses. A refusal of such a value is a coercion error the spec asks
// for, so the reference must refuse it too.

const { GraphQLError, Kind } = require("graphql");

const isDate = (y, m, d) => {
  const t = new Date(Date.UTC(y, m - 1, d));
  return t.getUTCFullYear() === y && t.getUTCMonth() === m - 1 && t.getUTCDate() === d;
};

const TIME = String.raw`([01]\d|2[0-3]):[0-5]\d:[0-5]\d(\.\d+)?`;
const DATE = String.raw`(\d{4})-(\d{2})-(\d{2})`;

/** name -> predicate over a string value. */
const RULES = {
  // https://scalars.graphql.org/chillicream/date-time: a date, `T`, a time, then `Z`
  // or a `±HH:MM` offset, which is required and never `-00:00`.
  DateTime: (s) => {
    const m = new RegExp(`^${DATE}[Tt]${TIME}([Zz]|[+-]([01]\\d|2[0-3]):[0-5]\\d)$`).exec(s);
    return Boolean(m) && isDate(+m[1], +m[2], +m[3]) && !s.endsWith("-00:00");
  },
  // https://scalars.graphql.org/chillicream/local-date-time: no offset.
  LocalDateTime: (s) => {
    const m = new RegExp(`^${DATE}[Tt]${TIME}$`).exec(s);
    return Boolean(m) && isDate(+m[1], +m[2], +m[3]);
  },
  // https://scalars.graphql.org/chillicream/local-date
  Date: (s) => {
    const m = new RegExp(`^${DATE}$`).exec(s);
    return Boolean(m) && isDate(+m[1], +m[2], +m[3]);
  },
  // https://scalars.graphql.org/chillicream/local-time
  Time: (s) => new RegExp(`^${TIME}$`).test(s),
  // RFC 4122 textual form.
  UUID: (s) => /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(s),
};

/** Gives each scalar of `schema` named in RULES its specified input coercion. */
function applySpecifiedScalars(schema) {
  for (const [name, ok] of Object.entries(RULES)) {
    const type = schema.getType(name);
    if (!type) continue;
    const check = (v) => {
      if (typeof v !== "string" || !ok(v)) {
        throw new GraphQLError(`${name} cannot represent ${JSON.stringify(v)}: not the form its @specifiedBy defines`);
      }
      return v;
    };
    type.parseValue = check;
    type.parseLiteral = (ast) => {
      if (ast.kind !== Kind.STRING) throw new GraphQLError(`${name} cannot represent a non-string literal`);
      return check(ast.value);
    };
  }
  return schema;
}

module.exports = { applySpecifiedScalars, RULES };
