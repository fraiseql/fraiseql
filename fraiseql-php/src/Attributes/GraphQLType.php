<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;

/**
 * PHP 8 Attribute for defining GraphQL type definitions.
 *
 * Usage:
 * ```php
 * #[GraphQLType(name: 'User')]
 * class User {
 *     #[GraphQLField]
 *     public int $id;
 *
 *     #[GraphQLField]
 *     public string $name;
 * }
 * ```
 */
#[Attribute(Attribute::TARGET_CLASS)]
final readonly class GraphQLType
{
    /**
     * @param string|null $name Optional custom GraphQL type name. Defaults to class name.
     * @param string|null $description Optional description for schema documentation.
     * @param bool $isInput Whether this type represents a GraphQL input type.
     */
    public function __construct(
        public ?string $name = null,
        public ?string $description = null,
        public bool $isInput = false,
    ) {
    }
}
