/**
 * Read the decorated member's name, under either decorator protocol.
 *
 * A method decorator is called with two different second arguments depending on how
 * the file was transpiled:
 *
 *   - **Legacy** (`experimentalDecorators: true`, what `tsc` does for this package):
 *     `(target, propertyKey: string, descriptor)`.
 *   - **TC39 / stage 3** (what esbuild — and therefore `tsx`, `bun` and every bundler
 *     built on it — emits): `(value, context)`, where `context` is
 *     `{ kind, name, metadata, static, private, access }`.
 *
 * `@Observer` and `@Source` assumed the legacy shape and stored the second argument
 * directly as the member's name. Under `npx tsx` — the runner the SDK's own examples
 * and README tell you to use — that stored the whole context *object*, which serialized
 * into `schema.json` as `"name": {"kind":"method","name":"onHighValueOrder",...}` and
 * made the compiler reject the document with `invalid type: map, expected a string`.
 * That is the defect frozen into the committed `ecommerce_schema.json` artifacts (#925);
 * deleting the artifacts did not fix the producer.
 *
 * The SDK cannot dictate which transpiler a user runs, so it accepts both.
 */
export function decoratedMemberName(propertyKeyOrContext: unknown, decorator: string): string {
  if (typeof propertyKeyOrContext === "string") {
    return propertyKeyOrContext;
  }

  if (typeof propertyKeyOrContext === "object" && propertyKeyOrContext !== null) {
    const name = (propertyKeyOrContext as { name?: unknown }).name;
    if (typeof name === "string") {
      return name;
    }
  }

  // Refuse rather than register something unusable. A name that is not a string cannot
  // become a valid schema, and failing here names the decorator instead of surfacing as
  // an opaque parse error from the compiler two commands later.
  throw new Error(
    `@${decorator} could not determine the decorated member's name from ` +
      `${JSON.stringify(propertyKeyOrContext)}. Expected a string (legacy decorators) or a ` +
      `decorator context object with a string \`name\` (TC39 decorators).`
  );
}
