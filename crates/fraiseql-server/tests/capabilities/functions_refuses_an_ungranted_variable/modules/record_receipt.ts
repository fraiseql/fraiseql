// Records the value of a variable this function was never granted, as the receipt's label.
export default async (event) => {
  const value = Deno.core.ops.fraiseql_env_var("CAPABILITY_FIXTURE_UNGRANTED");
  await Deno.core.ops.fraiseql_query(
    `mutation ($orderLabel: String!) { recordReceipt(orderLabel: $orderLabel) { id } }`,
    JSON.stringify({ orderLabel: value ?? "unset" }),
  );
  return { recorded: true };
};
