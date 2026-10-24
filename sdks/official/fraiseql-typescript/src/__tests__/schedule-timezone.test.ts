import { FraiseFunction } from "../functions";
import { Source } from "../sources";
import { SchemaRegistry } from "../registry";

describe("timezone on a schedule (#1658)", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("carries a source's timezone", () => {
    class Sources {
      @Source({ schedule: "30 2 * * *", timezone: "Europe/Paris" })
      nightlyPull(): void {}
    }
    void Sources;

    expect(SchemaRegistry.getSchema().sources?.[0]?.timezone).toBe("Europe/Paris");
  });

  it("omits a source's timezone when none is declared", () => {
    class Sources {
      @Source({ schedule: "30 2 * * *" })
      nightlyPull(): void {}
    }
    void Sources;

    expect(SchemaRegistry.getSchema().sources?.[0]).not.toHaveProperty("timezone");
  });

  it("carries a cron function's timezone", () => {
    class Functions {
      @FraiseFunction({ trigger: "cron:30 2 * * *", timezone: "Europe/Paris" })
      nightly_report(): void {}
    }
    void Functions;

    expect(SchemaRegistry.getSchema().functions?.[0]?.timezone).toBe("Europe/Paris");
  });

  it("omits a function's timezone when none is declared", () => {
    class Functions {
      @FraiseFunction({ trigger: "cron:30 2 * * *" })
      nightly_report(): void {}
    }
    void Functions;

    expect(SchemaRegistry.getSchema().functions?.[0]).not.toHaveProperty("timezone");
  });
});
