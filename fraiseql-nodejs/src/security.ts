/**
 * Security module for FraiseQL Node.js
 *
 * Implements advanced authorization and security features including:
 * - Custom authorization rules with context variables
 * - Role-based access control (RBAC) with multiple strategies
 * - Attribute-based access control (ABAC)
 * - Reusable authorization policies
 * - Caching and audit logging
 */

/**
 * Defines how to match multiple roles in RBAC
 */
export enum RoleMatchStrategy {
  /** User must have at least one of the specified roles */
  ANY = 'any',

  /** User must have all of the specified roles */
  ALL = 'all',

  /** User must have exactly these roles, no more, no less */
  EXACTLY = 'exactly',
}

/**
 * Defines the type of authorization policy
 */
export enum AuthzPolicyType {
  /** Role-based access control */
  RBAC = 'rbac',

  /** Attribute-based access control */
  ABAC = 'abac',

  /** Custom rule expressions */
  CUSTOM = 'custom',

  /** Hybrid approach combining multiple methods */
  HYBRID = 'hybrid',
}

/**
 * Configuration for custom authorization rules
 */
export interface AuthorizeConfig {
  /** Authorization rule expression */
  rule?: string;

  /** Reference to a named policy */
  policy?: string;

  /** Description of what this rule protects */
  description?: string;

  /** Custom error message on denial */
  errorMessage?: string;

  /** Whether to apply hierarchically to child fields */
  recursive?: boolean;

  /** Operation-specific rules (read, create, update, delete) */
  operations?: string;

  /** Whether to cache authorization decisions */
  cacheable?: boolean;

  /** Cache duration in seconds */
  cacheDurationSeconds?: number;
}

/**
 * Configuration for role-based access control
 */
export interface RoleRequiredConfig {
  /** Required roles */
  roles?: string[];

  /** Role matching strategy (ANY, ALL, EXACTLY) */
  strategy?: RoleMatchStrategy;

  /** Whether roles form a hierarchy */
  hierarchy?: boolean;

  /** Description of the role requirement */
  description?: string;

  /** Custom error message on denial */
  errorMessage?: string;

  /** Operation-specific rules */
  operations?: string;

  /** Whether to inherit role requirements from parent types */
  inherit?: boolean;

  /** Whether to cache role validation results */
  cacheable?: boolean;

  /** Cache duration in seconds */
  cacheDurationSeconds?: number;
}

/**
 * Configuration for reusable authorization policies
 */
export interface AuthzPolicyConfig {
  /** Policy name */
  name: string;

  /** Policy description */
  description?: string;

  /** Authorization rule expression */
  rule?: string;

  /** Attribute conditions for ABAC policies */
  attributes?: string[];

  /** Policy type (RBAC, ABAC, CUSTOM, HYBRID) */
  type?: AuthzPolicyType;

  /** Whether to cache authorization decisions */
  cacheable?: boolean;

  /** Cache duration in seconds */
  cacheDurationSeconds?: number;

  /** Whether to apply recursively to nested types */
  recursive?: boolean;

  /** Operation-specific rules */
  operations?: string;

  /** Whether to log access decisions */
  auditLogging?: boolean;

  /** Custom error message */
  errorMessage?: string;
}

/**
 * Builder for custom authorization rules
 *
 * Example:
 * ```typescript
 * new AuthorizeBuilder()
 *   .rule("isOwner($context.userId, $field.ownerId)")
 *   .description("Ensures users can only access their own notes")
 *   .build();
 * ```
 */
export class AuthorizeBuilder {
  private config: AuthorizeConfig = {
    cacheable: true,
    cacheDurationSeconds: 300,
  };

  /**
   * Set the authorization rule expression
   */
  rule(rule: string): AuthorizeBuilder {
    this.config.rule = rule;
    return this;
  }

  /**
   * Set the reference to a named authorization policy
   */
  policy(policy: string): AuthorizeBuilder {
    this.config.policy = policy;
    return this;
  }

  /**
   * Set the description of what this rule protects
   */
  description(description: string): AuthorizeBuilder {
    this.config.description = description;
    return this;
  }

  /**
   * Set the custom error message
   */
  errorMessage(errorMessage: string): AuthorizeBuilder {
    this.config.errorMessage = errorMessage;
    return this;
  }

  /**
   * Set whether to apply rule hierarchically to child fields
   */
  recursive(recursive: boolean): AuthorizeBuilder {
    this.config.recursive = recursive;
    return this;
  }

  /**
   * Set which operations this rule applies to (read, create, update, delete)
   */
  operations(operations: string): AuthorizeBuilder {
    this.config.operations = operations;
    return this;
  }

  /**
   * Set whether to cache authorization decisions
   */
  cacheable(cacheable: boolean): AuthorizeBuilder {
    this.config.cacheable = cacheable;
    return this;
  }

  /**
   * Set the cache duration in seconds
   */
  cacheDurationSeconds(duration: number): AuthorizeBuilder {
    this.config.cacheDurationSeconds = duration;
    return this;
  }

  /**
   * Build the authorization configuration
   */
  build(): AuthorizeConfig {
    return { ...this.config };
  }
}

/**
 * Builder for role-based access control rules
 *
 * Example:
 * ```typescript
 * new RoleRequiredBuilder()
 *   .roles('manager', 'director')
 *   .strategy(RoleMatchStrategy.ANY)
 *   .description("Managers and directors can view salaries")
 *   .build();
 * ```
 */
export class RoleRequiredBuilder {
  private config: RoleRequiredConfig = {
    strategy: RoleMatchStrategy.ANY,
    inherit: true,
    cacheable: true,
    cacheDurationSeconds: 600,
  };

  /**
   * Set required roles (variadic for convenience)
   */
  roles(...roles: string[]): RoleRequiredBuilder {
    this.config.roles = roles;
    return this;
  }

  /**
   * Set required roles from an array
   */
  rolesArray(roles: string[]): RoleRequiredBuilder {
    this.config.roles = roles;
    return this;
  }

  /**
   * Set the role matching strategy
   */
  strategy(strategy: RoleMatchStrategy): RoleRequiredBuilder {
    this.config.strategy = strategy;
    return this;
  }

  /**
   * Set whether roles form a hierarchy
   */
  hierarchy(hierarchy: boolean): RoleRequiredBuilder {
    this.config.hierarchy = hierarchy;
    return this;
  }

  /**
   * Set the description of the role requirement
   */
  description(description: string): RoleRequiredBuilder {
    this.config.description = description;
    return this;
  }

  /**
   * Set the custom error message
   */
  errorMessage(errorMessage: string): RoleRequiredBuilder {
    this.config.errorMessage = errorMessage;
    return this;
  }

  /**
   * Set which operations this rule applies to
   */
  operations(operations: string): RoleRequiredBuilder {
    this.config.operations = operations;
    return this;
  }

  /**
   * Set whether to inherit role requirements from parent types
   */
  inherit(inherit: boolean): RoleRequiredBuilder {
    this.config.inherit = inherit;
    return this;
  }

  /**
   * Set whether to cache role validation results
   */
  cacheable(cacheable: boolean): RoleRequiredBuilder {
    this.config.cacheable = cacheable;
    return this;
  }

  /**
   * Set the cache duration in seconds
   */
  cacheDurationSeconds(duration: number): RoleRequiredBuilder {
    this.config.cacheDurationSeconds = duration;
    return this;
  }

  /**
   * Build the role configuration
   */
  build(): RoleRequiredConfig {
    return { ...this.config };
  }
}

/**
 * Builder for reusable authorization policies
 *
 * Example:
 * ```typescript
 * new AuthzPolicyBuilder('piiAccess')
 *   .type(AuthzPolicyType.RBAC)
 *   .rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
 *   .description("Access to Personally Identifiable Information")
 *   .build();
 * ```
 */
export class AuthzPolicyBuilder {
  private config: AuthzPolicyConfig;

  constructor(name: string) {
    this.config = {
      name,
      type: AuthzPolicyType.CUSTOM,
      cacheable: true,
      cacheDurationSeconds: 300,
      auditLogging: false,
    };
  }

  /**
   * Set the policy description
   */
  description(description: string): AuthzPolicyBuilder {
    this.config.description = description;
    return this;
  }

  /**
   * Set the authorization rule expression
   */
  rule(rule: string): AuthzPolicyBuilder {
    this.config.rule = rule;
    return this;
  }

  /**
   * Set attribute conditions for ABAC policies (variadic)
   */
  attributes(...attributes: string[]): AuthzPolicyBuilder {
    this.config.attributes = attributes;
    return this;
  }

  /**
   * Set attribute conditions from an array
   */
  attributesArray(attributes: string[]): AuthzPolicyBuilder {
    this.config.attributes = attributes;
    return this;
  }

  /**
   * Set the policy type
   */
  type(type: AuthzPolicyType): AuthzPolicyBuilder {
    this.config.type = type;
    return this;
  }

  /**
   * Set whether to cache authorization decisions
   */
  cacheable(cacheable: boolean): AuthzPolicyBuilder {
    this.config.cacheable = cacheable;
    return this;
  }

  /**
   * Set the cache duration in seconds
   */
  cacheDurationSeconds(duration: number): AuthzPolicyBuilder {
    this.config.cacheDurationSeconds = duration;
    return this;
  }

  /**
   * Set whether to apply recursively to nested types
   */
  recursive(recursive: boolean): AuthzPolicyBuilder {
    this.config.recursive = recursive;
    return this;
  }

  /**
   * Set which operations this policy applies to
   */
  operations(operations: string): AuthzPolicyBuilder {
    this.config.operations = operations;
    return this;
  }

  /**
   * Set whether to log access decisions
   */
  auditLogging(auditLogging: boolean): AuthzPolicyBuilder {
    this.config.auditLogging = auditLogging;
    return this;
  }

  /**
   * Set the custom error message
   */
  errorMessage(errorMessage: string): AuthzPolicyBuilder {
    this.config.errorMessage = errorMessage;
    return this;
  }

  /**
   * Build the authorization policy configuration
   */
  build(): AuthzPolicyConfig {
    return { ...this.config };
  }
}

/**
 * Decorator for custom authorization rules
 */
export function Authorize(config: AuthorizeConfig): MethodDecorator & ClassDecorator {
  return function <T extends Function>(target: T | any, propertyKey?: string | symbol): any {
    return target;
  };
}

/**
 * Decorator for role-based access control
 */
export function RoleRequired(config: RoleRequiredConfig): MethodDecorator & ClassDecorator {
  return function <T extends Function>(target: T | any, propertyKey?: string | symbol): any {
    return target;
  };
}

/**
 * Decorator for authorization policies
 */
export function AuthzPolicy(config: AuthzPolicyConfig): ClassDecorator & MethodDecorator {
  return function <T extends Function>(target: T | any, propertyKey?: string | symbol): any {
    return target;
  };
}
