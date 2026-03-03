<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;

/**
 * PHP 8 Attribute for defining GraphQL field definitions.
 *
 * Usage:
 * ```php
 * #[GraphQLType(name: 'User')]
 * class User {
 *     #[GraphQLField(type: 'Int', nullable: false)]
 *     public int $id;
 *
 *     #[GraphQLField(type: 'String', nullable: false)]
 *     public string $name;
 *
 *     #[GraphQLField(type: 'String', nullable: true, scope: 'read:user.email')]
 *     public ?string $email;
 *
 *     #[GraphQLField(type: 'Float', scopes: ['admin', 'auditor'])]
 *     public float $salary;
 * }
 * ```
 */
#[Attribute(Attribute::TARGET_PROPERTY)]
final readonly class GraphQLField
{
    /**
     * @param string|null $type Optional explicit GraphQL type. Auto-detected from property type if not specified.
     * @param string|null $description Optional field description for schema documentation.
     * @param bool $nullable Whether the field is nullable in GraphQL.
     * @param string|null $resolver Optional custom resolver method name.
     * @param string|null $scope Optional JWT scope required to access this field (single scope).
     * @param array<string>|null $scopes Optional JWT scopes required to access this field (multiple scopes).
     */
    public function __construct(
        public ?string $type = null,
        public ?string $description = null,
        public bool $nullable = false,
        public ?string $resolver = null,
        public ?string $scope = null,
        public ?array $scopes = null,
    ) {
    }
}
