// Every order gets a receipt, written back through the GraphQL bridge.
export default async (event) => {
  const label = JSON.stringify(event.new.label);
  await Deno.core.ops.fraiseql_query(
    `mutation { recordReceipt(orderLabel: ${label}) { id } }`,
    "{}",
  );
  return { recorded: true };
};
