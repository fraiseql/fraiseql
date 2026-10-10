import { describe, expect, it } from "vitest";

import { typeToGraphQL } from "../types";

// #1530: a JavaScript `Date` is an instant, so it exports as the engine's `DateTime`. It
// used to export as its constructor's name, `Date`: a calendar date, the wrong scalar.
describe("stdlib date types", () => {
  it("maps a JavaScript Date to DateTime", () => {
    expect(typeToGraphQL(Date)).toEqual(["DateTime", false]);
  });
});
