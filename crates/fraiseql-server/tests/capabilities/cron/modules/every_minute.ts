// Each firing records the timezone its schedule was read in. The guest is called with
// the trigger payload's data: `{ schedule, timezone, executed_at }`.
export default async (event) => {
  await Deno.core.ops.fraiseql_query(
    `mutation ($zone: String!) { recordTick(zone: $zone) { id } }`,
    JSON.stringify({ zone: event.timezone }),
  );
  return { ticked: true };
};
