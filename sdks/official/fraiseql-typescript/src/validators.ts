/**
 * Validation engine for custom GraphQL scalars.
 */

import { SchemaRegistry } from "./registry";
import { CustomScalar } from "./scalars";

/**
 * Raised when custom scalar validation fails.
 */
export class ScalarValidationError extends Error {
  /**
   * Creates a new ScalarValidationError.
   *
   * @param scalarName - Name of the scalar that failed validation
   * @param context - The validation context ("serialize", "parseValue", or "parseLiteral")
   * @param message - The underlying error message
   */
  constructor(
    public scalarName: string,
    public context: "serialize" | "parseValue" | "parseLiteral",
    message: string
  ) {
    super(
      `Scalar ${JSON.stringify(scalarName)} validation failed in ${context}: ${message}`
    );
    this.name = "ScalarValidationError";
  }
}

/**
 * Execute validation for a custom scalar.
 *
 * @param scalarClass - The CustomScalar subclass to validate with
 * @param value - The value to validate
 * @param context - One of "serialize", "parseValue", or "parseLiteral"
 * @returns The validated/converted value
 * @throws ScalarValidationError if validation fails
 * @throws Error if context is unknown
 *
 * @example
 * ```typescript
 * import { validateCustomScalar } from "fraiseql/validators"
 * import { Email } from "./scalars"
 *
 * // Parse a variable value from GraphQL
 * const emailValue = validateCustomScalar(Email, "user@example.com", "parseValue")
 * // Returns "user@example.com"
 *
 * // Validation fails
 * try {
 *   validateCustomScalar(Email, "invalid-email", "parseValue")
 * } catch (e) {
 *   // ScalarValidationError: Scalar "Email" validation failed in parseValue: Invalid email
 * }
 * ```
 */
export function validateCustomScalar(
  scalarClass: typeof CustomScalar,
  value: unknown,
  context: "serialize" | "parseValue" | "parseLiteral" = "parseValue"
): unknown {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- instantiating abstract class subclass
  const instance = new (scalarClass as new () => any)();
  const scalarName = instance.name;

  try {
    switch (context) {
      case "serialize":
        return instance.serialize(value);
      case "parseValue":
        return instance.parseValue(value);
      case "parseLiteral":
        return instance.parseLiteral(value);
      default:
        throw new Error(`Unknown validation context: ${context}`);
    }
  } catch (error) {
    if (error instanceof ScalarValidationError) {
      throw error;
    }

    const message =
      error instanceof Error ? error.message : String(error);
    throw new ScalarValidationError(scalarName, context, message);
  }
}

/**
 * Get all registered custom scalars.
 *
 * @returns Map of scalar names to CustomScalar classes
 *
 * @example
 * ```typescript
 * import { getAllCustomScalars } from "fraiseql/validators"
 *
 * const scalars = getAllCustomScalars()
 * // Map { "Email" => class Email, "Phone" => class Phone, ... }
 * ```
 */
export function getAllCustomScalars(): Map<string, typeof CustomScalar> {
  return SchemaRegistry.getCustomScalars();
}
