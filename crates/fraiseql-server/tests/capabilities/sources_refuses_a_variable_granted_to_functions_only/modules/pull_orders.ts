// Records page 1 only once it has read a variable granted to functions, not to sources.
export default async () => {
  const value = Deno.core.ops.fraiseql_env_var("CAPABILITY_FIXTURE_FUNCTIONS_ONLY");
  if (value !== "granted-to-functions") {
    return { page: null };
  }
  await Deno.core.ops.fraiseql_query(
    `mutation ($page: Int!) { recordPage(page: $page) { id } }`,
    JSON.stringify({ page: 1 }),
  );
  return { page: 1 };
};
