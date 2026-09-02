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
     * @param list<\FraiseQL\Relationship> $relationships Relationships to other types,
     *        followed by REST resource embedding (#1266) — `?select=orders(id,total)`,
     *        `?select=orders.count`, `?orders.status=paid` — and published in the served
     *        OpenAPI document and the generated client's `relationships` module.
     *
     * @throws \FraiseQL\FraiseQLException If a relationship name is declared twice; an
     *         embed resolves the first and the rest are unreachable, which no compiler
     *         diagnostic can attribute back to this declaration.
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
        public array $relationships = [],
    ) {
        $seen = [];
        foreach ($relationships as $relationship) {
            if (!$relationship instanceof \FraiseQL\Relationship) {
                throw new \FraiseQL\FraiseQLException(
                    'relationships must be a list of FraiseQL\Relationship',
                );
            }
            if (isset($seen[$relationship->name])) {
                throw new \FraiseQL\FraiseQLException(sprintf(
                    'relationship "%s" is declared more than once; an embed resolves the '
                        . 'first and the rest are unreachable',
                    $relationship->name,
                ));
            }
            $seen[$relationship->name] = true;
        }
    }
}
