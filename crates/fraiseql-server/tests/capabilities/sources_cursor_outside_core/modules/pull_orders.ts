// Each firing pulls the page after the cursor, records it, and advances the cursor.
export default async () => {
  const before = JSON.parse(await Deno.core.ops.fraiseql_cursor_get());
  const page = (before && before.page ? before.page : 0) + 1;
  await Deno.core.ops.fraiseql_query(
    `mutation ($page: Int!) { recordPage(page: $page) { id } }`,
    JSON.stringify({ page }),
  );
  await Deno.core.ops.fraiseql_cursor_advance(JSON.stringify({ page }));
  return { page };
};
