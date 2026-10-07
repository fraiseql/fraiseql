/**
 * The connector `schema.with-source.compiled.json`'s `example_source` runs.
 *
 * Release-smoke boots that schema to prove the source scheduler and the Deno runtime
 * mount together. An enabled source whose connector cannot load refuses the boot, so
 * the connector has to exist; it does nothing, because the smoke asserts boot, not
 * ingestion. See `sdks/official/fraiseql-typescript/examples/sources/` for a real one.
 */
export default async (): Promise<void> => {};
