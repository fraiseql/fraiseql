// Run `make codegen` first to produce ./generated.
import {
  FraiseqlClient,
  getUser,
  users,
  postsConnection,
  createUser,
  isErrorResult,
  type User,
} from "./generated";

const token = ""; // supply your auth token here
const client = new FraiseqlClient({
  endpoint: "https://api.example.com/graphql",
  // Headers can be static or computed per request (e.g. for auth tokens):
  headers: () => ({ authorization: `Bearer ${token}` }),
});

async function main(): Promise<void> {
  // Single, nullable result. `User` holds only the leaf fields the default
  // document fetches — `tenant`/`posts` are intentionally not part of the type.
  const user: User | null = await getUser(client, { id: "u1" });
  if (user) {
    console.log(user.email, user.role, user.createdAt);
  }

  // List query with an input filter.
  const admins = await users(client, { filter: { role: "ADMIN" } });
  console.log(`${admins.length} admins`);

  // Relay connection — forward pagination.
  const page = await postsConnection(client, { first: 10 });
  for (const edge of page.edges) {
    console.log(edge.cursor, edge.node.title);
  }
  if (page.pageInfo.hasNextPage) {
    console.log("more pages available");
  }

  // Mutation returning a discriminated result union.
  const result = await createUser(client, {
    input: { email: "alice@example.com", role: "EDITOR" },
  });
  if (isErrorResult(result)) {
    // Narrowed to EmailTakenError: `status` is the injected error_class.
    console.error(`createUser failed (${result.status}): ${result.message}`);
  } else {
    // Narrowed to User.
    console.log(`created user ${result.id}`);
  }
}

void main();
