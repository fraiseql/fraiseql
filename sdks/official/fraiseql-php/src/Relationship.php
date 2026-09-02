<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * A relationship to another type, followed by REST resource embedding (#1266).
 *
 * `$name` is what a client writes in `?select=orders(id,total)`, `?select=orders.count`
 * and `?orders.status=paid`; it is also what the generated client's `relationships`
 * module and the served OpenAPI document publish.
 *
 * `$foreignKey` and `$referencedKey` are SQL **column** names, and which side each is
 * read from swaps with the cardinality — `OneToMany` reads `$referencedKey` off the
 * declaring type and filters `$foreignKey` on the target; `ManyToOne` and `OneToOne` do
 * the reverse. Under the default `camelCase` naming convention the column `fk_user` is
 * published as the field `fkUser`, and the compiler resolves one to the other.
 *
 * Which relationships are *followable* is the compiler's business, not this SDK's: it
 * refuses a target type it does not declare, a join column no field on that side
 * publishes, and a target no list query returns. This SDK carries no second copy of
 * those rules; a copy is what drifts.
 *
 * Usage:
 * ```php
 * #[GraphQLType(name: 'User', sqlSource: 'v_user', relationships: [
 *     new Relationship('orders', 'Order', Relationship::ONE_TO_MANY, 'fk_user', 'id'),
 * ])]
 * class User { }
 * ```
 */
final readonly class Relationship
{
    public const ONE_TO_MANY = 'OneToMany';
    public const MANY_TO_ONE = 'ManyToOne';
    public const ONE_TO_ONE = 'OneToOne';

    public const CARDINALITIES = [self::ONE_TO_MANY, self::MANY_TO_ONE, self::ONE_TO_ONE];

    /**
     * @param string $name Relationship name — the key in `?select=` and in the response.
     * @param string $targetType Target type name. Must be a declared type some **list**
     *                           query returns: an embed sources its rows from that query.
     * @param string $cardinality One of the `CARDINALITIES` above.
     * @param string $foreignKey Foreign key **column** on the child table, e.g. `fk_user`.
     * @param string $referencedKey Referenced key **column** on the parent table, e.g. `id`.
     *
     * @throws FraiseQLException If any name is empty or the cardinality is not one of the three.
     */
    public function __construct(
        public string $name,
        public string $targetType,
        public string $cardinality,
        public string $foreignKey,
        public string $referencedKey,
    ) {
        foreach ([
            'name' => $name,
            'targetType' => $targetType,
            'foreignKey' => $foreignKey,
            'referencedKey' => $referencedKey,
        ] as $label => $value) {
            if ($value === '') {
                throw new FraiseQLException("Relationship $label must not be empty");
            }
        }

        if (!in_array($cardinality, self::CARDINALITIES, true)) {
            throw new FraiseQLException(sprintf(
                'Relationship cardinality must be one of %s (got "%s")',
                implode(', ', self::CARDINALITIES),
                $cardinality,
            ));
        }
    }

    /**
     * The relationship entry as the AuthoringIR spells it.
     *
     * @return array<string, string>
     */
    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'target_type' => $this->targetType,
            'cardinality' => $this->cardinality,
            'foreign_key' => $this->foreignKey,
            'referenced_key' => $this->referencedKey,
        ];
    }
}
