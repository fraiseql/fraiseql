// Every accepted delivery is recorded through the GraphQL bridge.
export default async () => {
  await Deno.core.ops.fraiseql_query(`mutation { recordDelivery(source: "orders") { id } }`, "{}");
  return { recorded: true };
};
