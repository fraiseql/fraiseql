<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;

/**
 * Marks a type or field with a custom authorization rule.
 *
 * Can be used on classes or properties to enforce authorization logic
 * through rule expressions with context variables.
 *
 * Example:
 * ```php
 * #[Authorize(
 *     rule: "isOwner(\$context.userId, \$field.ownerId)",
 *     description: "Users can only access their own notes"
 * )]
 * #[GraphQLType(name: 'Note')]
 * class Note { }
 * ```
 *
 * @package FraiseQL\Attributes
 */
#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_PROPERTY)]
final readonly class Authorize
{
    public function __construct(
        public string $rule = '',
        public string $policy = '',
        public string $description = '',
        public string $errorMessage = '',
        public bool $recursive = false,
        public string $operations = '',
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 300,
    ) {
    }
}
