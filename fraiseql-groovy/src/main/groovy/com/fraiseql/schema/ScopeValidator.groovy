package com.fraiseql.schema

/**
 * Validator for field-level scope format and patterns
 *
 * Scope format: action:resource
 * Examples: read:user.email, admin:*, write:Post.*
 *
 * Rules:
 * - Action: [a-zA-Z_][a-zA-Z0-9_]*
 * - Resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
 */
class ScopeValidator {

  private static final String ACTION_PATTERN = '^[a-zA-Z_][a-zA-Z0-9_]*$'
  private static final String RESOURCE_PATTERN = '^([a-zA-Z_][a-zA-Z0-9_.]*|\\*)$'

  /**
   * Validates scope format: action:resource
   *
   * @param scope The scope string to validate
   * @return true if valid, false otherwise
   */
  static boolean validate(String scope) {
    if (!scope || scope.isEmpty()) {
      return false
    }

    if (scope == '*') {
      return true
    }

    String[] parts = scope.split(':')
    if (parts.length != 2) {
      return false
    }

    String action = parts[0]
    String resource = parts[1]

    if (!action || !resource) {
      return false
    }

    return action.matches(ACTION_PATTERN) && resource.matches(RESOURCE_PATTERN)
  }

  /**
   * Validates a list of scopes
   *
   * @param scopes The list of scopes to validate
   * @return true if all are valid, false otherwise
   */
  static boolean validateAll(List<String> scopes) {
    if (!scopes || scopes.isEmpty()) {
      return false
    }
    return scopes.every { scope -> validate(scope) }
  }
}
