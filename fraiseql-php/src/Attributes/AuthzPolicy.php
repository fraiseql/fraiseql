<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;
use FraiseQL\Security\AuthzPolicyType;

/**
 * Marks a type or field with a named, reusable authorization policy.
 *
 * Allows defining policies once and applying them to multiple fields,
 * supporting RBAC, ABAC, custom rules, and hybrid approaches.
 *
 * Example:
 * ```php
 * #[AuthzPolicy(
 *     name: 'piiAccess',
 *     type: AuthzPolicyType::RBAC,
 *     rule: "hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')",
 *     description: 'Access to Personally Identifiable Information'
 * )]
 * #[GraphQLType(name: 'Customer')]
 * class Customer { }
 * ```
 *
 * @package FraiseQL\Attributes
 */
#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY)]
final readonly class AuthzPolicy
{
    /**
     * @param string $name Policy name
     * @param AuthzPolicyType|string $type Policy type (RBAC, ABAC, CUSTOM, HYBRID)
     * @param string $rule Authorization rule expression
     * @param array<string> $attributes Attribute conditions for ABAC
     * @param string $description Policy description
     * @param string $errorMessage Custom error message
     * @param bool $recursive Whether to apply recursively
     * @param string $operations Operation-specific rules
     * @param bool $auditLogging Whether to log access decisions
     * @param bool $cacheable Whether to cache decisions
     * @param int $cacheDurationSeconds Cache duration in seconds
     */
    public function __construct(
        public string $name,
        public AuthzPolicyType|string $type = AuthzPolicyType::CUSTOM,
        public string $rule = '',
        public array $attributes = [],
        public string $description = '',
        public string $errorMessage = '',
        public bool $recursive = false,
        public string $operations = '',
        public bool $auditLogging = false,
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 300,
    ) {
    }
}
