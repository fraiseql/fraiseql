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
     * @param string|null $sqlSource The SQL view backing this type (e.g. 'v_user').
     * @param string|null $description Optional description for schema documentation.
     * @param bool $isInput Whether this type represents a GraphQL input type.
     * @param bool $relay Whether this type implements the Relay Node interface.
     * @param bool $isError Whether this type represents a mutation error type.
     * @param bool $crud When true, auto-generate CRUD queries and mutations for this type.
     * @param bool $cascade When true, generated CRUD mutations include cascade support.
     */
    public function __construct(
        public ?string $name = null,
        public ?string $sqlSource = null,
        public ?string $description = null,
        public bool $isInput = false,
        public bool $relay = false,
        public bool $isError = false,
        public bool $crud = false,
        public bool $cascade = false,
    ) {
    }
}
