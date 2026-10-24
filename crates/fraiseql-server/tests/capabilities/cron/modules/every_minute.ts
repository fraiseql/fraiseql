// Each firing records the timezone its schedule was read in. The guest is called with
// the trigger payload's data: `{ schedule, timezone, executed_at }`.
export default async (event) => {
  const zone = JSON.stringify(event.timezone);
  await Deno.core.ops.fraiseql_query(`mutation { recordTick(zone: ${zone}) { id } }`, "{}");
  return { ticked: true };
};
