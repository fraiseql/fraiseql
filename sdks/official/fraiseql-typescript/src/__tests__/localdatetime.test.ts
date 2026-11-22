import { describe, expect, it } from "vitest";
import type { LocalDateTime } from "../index";
import { isScalarType } from "../scalars";
import { extractFieldInfo, typeToGraphQL } from "../types";

// A JavaScript `Date` is an instant and stays `DateTime`; a date and time with no offset
// (a PostgreSQL `timestamp`) is declared `LocalDateTime` by name (#2177).
describe("localdatetime", () => {
  it("knows LocalDateTime as a scalar name", () => {
    expect(isScalarType("LocalDateTime")).toBe(true);
  });

  it("reads the LocalDateTime name as written, and Date as DateTime", () => {
    expect(typeToGraphQL("LocalDateTime")).toEqual(["LocalDateTime", false]);
    expect(typeToGraphQL("LocalDateTime | null")).toEqual(["LocalDateTime", true]);
    expect(typeToGraphQL("LocalDateTime[]")).toEqual(["[LocalDateTime!]", false]);
    expect(typeToGraphQL(Date)).toEqual(["DateTime", false]);
    expect(extractFieldInfo({ startsAt: "LocalDateTime" })).toEqual({
      startsAt: { type: "LocalDateTime", nullable: false },
    });
  });

  it("brands a LocalDateTime string", () => {
    const startsAt = "2025-01-10T12:00:00" as LocalDateTime;
    expect(startsAt).toBe("2025-01-10T12:00:00");
  });
});
