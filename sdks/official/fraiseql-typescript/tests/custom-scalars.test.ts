/**
 * End-to-end tests for custom scalar support.
 */

import { describe, it, expect, beforeEach } from "@jest/globals";
import { CustomScalar, Scalar, validateCustomScalar, getAllCustomScalars, ScalarValidationError } from "../src";
import { SchemaRegistry } from "../src/registry";

/**
 * Email scalar for testing.
 */
@Scalar
class Email extends CustomScalar {
  name = "Email";

  private static readonly EMAIL_REGEX = /^[^@]+@[^@]+\.[^@]+$/;

  serialize(value: unknown): string {
    return String(value);
  }

  parseValue(value: unknown): string {
    const str = String(value).trim();
    if (!Email.EMAIL_REGEX.test(str)) {
      throw new Error(`Invalid email format: ${str}`);
    }
    return str;
  }

  parseLiteral(ast: unknown): string {
    if (ast && typeof ast === "object" && "value" in ast) {
      return this.parseValue((ast as any).value);
    }
    throw new Error(`Email literal must be string, got ${typeof ast}`);
  }
}

/**
 * Phone scalar for testing.
 */
@Scalar
class Phone extends CustomScalar {
  name = "Phone";

  serialize(value: unknown): string {
    return String(value);
  }

  parseValue(value: unknown): string {
    const str = String(value).trim();
    if (!str.startsWith("+")) {
      throw new Error("Phone must start with +");
    }
    const digits = str.slice(1);
    if (!/^\d+$/.test(digits)) {
      throw new Error("Phone must contain only digits after +");
    }
    if (digits.length < 10 || digits.length > 14) {
      throw new Error(`Phone must be 10-14 digits, got ${digits.length}`);
    }
    return str;
  }

  parseLiteral(ast: unknown): string {
    if (ast && typeof ast === "object" && "value" in ast) {
      return this.parseValue((ast as any).value);
    }
    throw new Error(`Phone literal must be string, got ${typeof ast}`);
  }
}

describe("Custom Scalar Support", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("Scalar Registration", () => {
    it("should register a scalar globally", () => {
      // Before registration, no scalars
      expect(SchemaRegistry.getCustomScalars().size).toBe(0);

      // Register Email
      @Scalar
      class EmailScalar extends CustomScalar {
        name = "Email";
        serialize(v: unknown) { return v; }
        parseValue(v: unknown) { return v; }
        parseLiteral(v: unknown) { return v; }
      }

      // After registration, Email is in registry
      expect(SchemaRegistry.getCustomScalars().has("Email")).toBe(true);
    });

    it("should validate class is CustomScalar", () => {
      class NotAScalar {
        name = "NotAScalar";
      }

      expect(() => {
        Scalar(NotAScalar as any);
      }).toThrow("CustomScalar subclasses");
    });

    it("should validate name attribute", () => {
      // @ts-expect-error intentionally missing abstract 'name' for error-path test
      class NoNameScalar extends CustomScalar {
        serialize(v: unknown) { return v; }
        parseValue(v: unknown) { return v; }
        parseLiteral(v: unknown) { return v; }
      }

      expect(() => {
        Scalar(NoNameScalar);
      }).toThrow("must have a 'name' property");
    });

    it("should prevent duplicate names", () => {
      @Scalar
      class Email1 extends CustomScalar {
        name = "Email";
        serialize(v: unknown) { return v; }
        parseValue(v: unknown) { return v; }
        parseLiteral(v: unknown) { return v; }
      }

      class Email2 extends CustomScalar {
        name = "Email";
        serialize(v: unknown) { return v; }
        parseValue(v: unknown) { return v; }
        parseLiteral(v: unknown) { return v; }
      }

      expect(() => {
        Scalar(Email2);
      }).toThrow("already registered");
    });
  });

  describe("Validation Engine", () => {
    it("should validate parse_value successfully", () => {
      const result = validateCustomScalar(Email, "user@example.com", "parseValue");
      expect(result).toBe("user@example.com");
    });

    it("should reject invalid values", () => {
      expect(() => {
        validateCustomScalar(Email, "invalid-email", "parseValue");
      }).toThrow(ScalarValidationError);
    });

    it("should serialize values", () => {
      const result = validateCustomScalar(Email, "user@example.com", "serialize");
      expect(result).toBe("user@example.com");
    });

    it("should parse literals", () => {
      const ast = { value: "user@example.com" };
      const result = validateCustomScalar(Email, ast, "parseLiteral");
      expect(result).toBe("user@example.com");
    });

    it("should handle multiple scalar types", () => {
      // Email validation
      const email = validateCustomScalar(Email, "test@test.com");
      expect(email).toBe("test@test.com");

      // Phone validation
      const phone = validateCustomScalar(Phone, "+12025551234");
      expect(phone).toBe("+12025551234");

      // Both fail appropriately
      expect(() => {
        validateCustomScalar(Email, "notanemail");
      }).toThrow(ScalarValidationError);

      expect(() => {
        validateCustomScalar(Phone, "invalid");
      }).toThrow(ScalarValidationError);
    });

    it("should throw on invalid context", () => {
      expect(() => {
        validateCustomScalar(Email, "test@test.com", "invalid" as any);
      }).toThrow(ScalarValidationError);
    });
  });

  describe("Error Messages", () => {
    it("should include scalar name in error", () => {
      try {
        validateCustomScalar(Email, "notanemail");
        expect.fail("Should have thrown");
      } catch (e) {
        expect(String(e)).toContain("Email");
      }
    });

    it("should include context in error", () => {
      try {
        validateCustomScalar(Email, "notanemail", "parseValue");
        expect.fail("Should have thrown");
      } catch (e) {
        expect(String(e)).toContain("parseValue");
      }
    });

    it("should include original message", () => {
      try {
        validateCustomScalar(Email, "notanemail");
        expect.fail("Should have thrown");
      } catch (e) {
        expect(String(e)).toContain("Invalid email format");
      }
    });
  });

  describe("Utility Functions", () => {
    beforeEach(() => {
      SchemaRegistry.registerScalar("Email", Email);
      SchemaRegistry.registerScalar("Phone", Phone);
    });

    it("should get all custom scalars", () => {
      const scalars = getAllCustomScalars();
      expect(scalars.has("Email")).toBe(true);
      expect(scalars.has("Phone")).toBe(true);
    });

    it("should return empty map when none registered", () => {
      SchemaRegistry.clear();
      const scalars = getAllCustomScalars();
      expect(scalars.size).toBe(0);
    });
  });

  describe("Schema Export", () => {
    beforeEach(() => {
      SchemaRegistry.registerScalar("Email", Email);
      SchemaRegistry.registerScalar("Phone", Phone);
    });

    it("should include custom scalars in schema", () => {
      const schema = SchemaRegistry.getSchema();

      expect((schema as any).customScalars).toBeDefined();
      expect((schema as any).customScalars.Email).toBeDefined();
      expect((schema as any).customScalars.Phone).toBeDefined();
    });

    it("should have correct schema structure", () => {
      const schema = SchemaRegistry.getSchema();
      const emailDef = (schema as any).customScalars.Email;

      expect(emailDef.name).toBe("Email");
      expect(emailDef.validate).toBe(true);
      expect(emailDef.description).toBeDefined();
    });
  });
});

describe("Email Scalar Implementation", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("should validate correct emails", () => {
    const email = new Email();
    const result = email.parseValue("test@example.com");
    expect(result).toBe("test@example.com");
  });

  it("should reject emails without @", () => {
    const email = new Email();
    expect(() => {
      email.parseValue("notanemail");
    }).toThrow("Invalid email format");
  });

  it("should serialize correctly", () => {
    const email = new Email();
    const result = email.serialize("test@example.com");
    expect(result).toBe("test@example.com");
  });

  it("should parse literals", () => {
    const email = new Email();
    const ast = { value: "test@example.com" };
    const result = email.parseLiteral(ast);
    expect(result).toBe("test@example.com");
  });
});

describe("Phone Scalar Implementation", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  it("should validate E.164 format", () => {
    const phone = new Phone();
    const result = phone.parseValue("+12025551234");
    expect(result).toBe("+12025551234");
  });

  it("should require + prefix", () => {
    const phone = new Phone();
    expect(() => {
      phone.parseValue("2025551234");
    }).toThrow("must start with +");
  });

  it("should validate digit count", () => {
    const phone = new Phone();
    expect(() => {
      phone.parseValue("+123");
    }).toThrow("must be 10-14 digits");
  });

  it("should serialize correctly", () => {
    const phone = new Phone();
    const result = phone.serialize("+12025551234");
    expect(result).toBe("+12025551234");
  });
});
