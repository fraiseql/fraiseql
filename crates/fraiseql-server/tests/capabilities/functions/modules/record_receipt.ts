// Every order gets a receipt, written back through the GraphQL bridge.
export default async (event) => {
  await Deno.core.ops.fraiseql_query(
    `mutation ($orderLabel: String!) { recordReceipt(orderLabel: $orderLabel) { id } }`,
    JSON.stringify({ orderLabel: event.new.label }),
  );
  return { recorded: true };
};
