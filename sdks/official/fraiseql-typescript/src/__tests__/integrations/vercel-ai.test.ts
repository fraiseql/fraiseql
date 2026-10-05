// Against the real `ai` package, not a mock: a mocked `tool()` that echoed back whatever
// key it was given is how the integration shipped a tool with no input schema.
import { describe, expect, it, vi } from "vitest";
import { z } from "zod";

import { FraiseQLClient } from "../../client";
import { fraiseqlTool } from "../../integrations/vercel-ai";

function makeMockClient(data: Record<string, unknown>): FraiseQLClient {
  const fetchMock = vi.fn().mockResolvedValue({
    status: 200,
    ok: true,
    statusText: "OK",
    headers: { get: () => null },
    json: () => Promise.resolve({ data }),
  });
  return new FraiseQLClient({
    url: "http://localhost:4000/graphql",
    fetch: fetchMock as unknown as typeof fetch,
  });
}

const parameters = z.object({ id: z.string() });
// The execution options' shape is the installed `ai`'s (7 added a required `context`); the
// integration ignores them, so a minimal object typed as whatever `execute` declares.
type CallOptions = Parameters<NonNullable<ReturnType<typeof fraiseqlTool>["execute"]>>[1];
const callOptions = { toolCallId: "c1", messages: [], context: {} } as unknown as CallOptions;

describe("fraiseqlTool (Vercel AI)", () => {
  it("hands the model the parameter schema, under the key the installed `ai` reads", () => {
    const t = fraiseqlTool(makeMockClient({}), {
      name: "getUser",
      description: "Fetch a user",
      query: "query($id: ID!) { user(id: $id) { id } }",
      parameters,
    });
    // AI SDK 5+ reads `inputSchema`; a tool without it offers the model no arguments.
    expect(t.inputSchema).toBe(parameters);
    expect(t.description).toBe("Fetch a user");
  });

  it("execute calls client.query with the model's arguments", async () => {
    const userData = { user: { id: "1", name: "Alice" } };
    const client = makeMockClient(userData);
    const querySpy = vi.spyOn(client, "query");
    const t = fraiseqlTool(client, {
      name: "getUser",
      description: "Fetch a user",
      query: "query($id: ID!) { user(id: $id) { id name } }",
      parameters,
    });
    const result = await t.execute?.({ id: "1" }, callOptions);
    expect(querySpy).toHaveBeenCalledWith("query($id: ID!) { user(id: $id) { id name } }", {
      id: "1",
    });
    expect(result).toEqual(userData);
  });

  it("applies the transform", async () => {
    const t = fraiseqlTool(makeMockClient({ users: [{ id: "1" }, { id: "2" }] }), {
      name: "listUsers",
      description: "List users",
      query: "{ users { id } }",
      parameters: z.object({}),
      transform: (data) => (data["users"] as { id: string }[]).map((u) => u.id),
    });
    expect(await t.execute?.({}, callOptions)).toEqual(["1", "2"]);
  });
});
