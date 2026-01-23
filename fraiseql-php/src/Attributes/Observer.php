<?php

declare(strict_types=1);

namespace FraiseQL\Attributes;

use Attribute;

/**
 * Marks a class as defining a FraiseQL observer.
 *
 * Observers listen to database change events (INSERT/UPDATE/DELETE) and execute
 * actions (webhooks, Slack, email) when conditions are met.
 *
 * Example:
 * ```php
 * #[Observer(
 *     name: 'onHighValueOrder',
 *     entity: 'Order',
 *     event: 'INSERT',
 *     condition: 'total > 1000'
 * )]
 * class HighValueOrderObserver {
 *     // Actions are registered separately using ObserverBuilder
 * }
 * ```
 */
#[Attribute(Attribute::TARGET_CLASS)]
final class Observer
{
    public function __construct(
        public readonly string $name,
        public readonly string $entity,
        public readonly string $event,
        public readonly ?string $condition = null,
        public readonly ?string $description = null,
    ) {
    }
}
