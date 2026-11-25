import { vi } from "vitest";
import { FraiseQLClient } from "../client";
import type { Decimal } from "../scalars";

// A Decimal travels as the string of its exact digits. The server takes a Decimal
// argument as a string such as "1.50", or an exact integer, and refuses a number with a
// fraction. `Decimal` is a branded string, so it is sent and read as one; a JS number is
// a binary float and is sent as a number, which the server refuses for a Decimal.

const DIGITS = [
  "1.50",
  "100.00",
  "12345678901234567890.12",
  "123456789012345678901234567890",
  "-0.00",
];

/** A fetch that records the body it is given and answers by parsing `responseText`. */
function echoFetch(responseText: (sentBody: string) => string) {
  return vi.fn().mockImplementation((_url: string, init: { body: string }) => {
    const text = responseText(init.body);
    return Promise.resolve({
      status: 200,
      statusText: "OK",
      ok: true,
      headers: { get: () => null },
      json: () => Promise.resolve(JSON.parse(text)),
      text: () => Promise.resolve(text),
    });
  });
}

function sentBody(fetchMock: ReturnType<typeof vi.fn>): string {
  const call = fetchMock.mock.calls[0];
  if (call === undefined) {
    throw new Error("the client sent no request");
  }
  return (call[1] as { body: string }).body;
}

describe("Decimal values", () => {
  it.each(DIGITS)("a Decimal variable %s is sent and read as its exact digits", async (digits) => {
    const fetchMock = echoFetch(
      (body) => `{"data":{"echo":${JSON.stringify(JSON.parse(body).variables)}}}`
    );
    const client = new FraiseQLClient({
      url: "http://localhost:4000/graphql",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const amount = digits as Decimal;
    const data = await client.query<{ echo: { a: Decimal } }>(
      "query($a: Decimal) { echo(a: $a) }",
      { a: amount }
    );

    expect(sentBody(fetchMock)).toContain(`"a":"${digits}"`);
    expect(data.echo.a).toBe(digits);
  });

  it("a response Decimal is the exact string, never a number", async () => {
    const fetchMock = echoFetch(() => `{"data":{"amount":"12345678901234567890.12"}}`);
    const client = new FraiseQLClient({
      url: "http://localhost:4000/graphql",
      fetch: fetchMock as unknown as typeof fetch,
    });

    const data = await client.query<{ amount: Decimal }>("{ amount }");
    expect(typeof data.amount).toBe("string");
    expect(data.amount).toBe("12345678901234567890.12");
  });

  it("a JS number is sent as a number, which a Decimal argument refuses", async () => {
    const fetchMock = echoFetch(() => `{"data":{}}`);
    const client = new FraiseQLClient({
      url: "http://localhost:4000/graphql",
      fetch: fetchMock as unknown as typeof fetch,
    });

    await client.query("query($a: Decimal) { echo(a: $a) }", { a: 1.5 });
    expect(sentBody(fetchMock)).toContain(`"a":1.5`);
  });
});
