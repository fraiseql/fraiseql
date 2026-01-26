<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;
use FraiseQL\Security\RoleMatchStrategy;

/**
 * Marks a type or field with role-based access control requirements.
 *
 * Enforces that users must have specified roles before accessing
 * the decorated type or field.
 *
 * Example:
 * ```php
 * #[RoleRequired(
 *     roles: ['manager', 'director'],
 *     strategy: RoleMatchStrategy::ANY,
 *     description: 'Managers and directors can view salaries'
 * )]
 * #[GraphQLType(name: 'SalaryData')]
 * class SalaryData { }
 * ```
 *
 * @package FraiseQL\Attributes
 */
#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY)]
final readonly class RoleRequired
{
    /**
     * @param array<string> $roles Required roles
     * @param RoleMatchStrategy|string $strategy Role matching strategy
     * @param bool $hierarchy Whether roles form a hierarchy
     * @param string $description Description of the requirement
     * @param string $errorMessage Custom error message
     * @param string $operations Operation-specific rules
     * @param bool $inherit Whether to inherit from parent types
     * @param bool $cacheable Whether to cache role validation
     * @param int $cacheDurationSeconds Cache duration in seconds
     */
    public function __construct(
        public array $roles = [],
        public RoleMatchStrategy|string $strategy = RoleMatchStrategy::ANY,
        public bool $hierarchy = false,
        public string $description = '',
        public string $errorMessage = '',
        public string $operations = '',
        public bool $inherit = true,
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 600,
    ) {
    }
}
