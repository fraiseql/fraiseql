<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Definition for a GraphQL subscription.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * Usage:
 * ```php
 * $subscription = new SubscriptionDefinition(
 *     name: 'orderCreated',
 *     entityType: 'Order',
 *     description: 'Subscribe to new orders',
 *     topic: 'order_events',
 *     operation: 'CREATE',
 * );
 * ```
 */
final class SubscriptionDefinition
{
    /**
     * @param string $name The subscription name
     * @param string $entityType The entity type being subscribed to
     * @param bool $nullable Whether the subscription can return null
     * @param array<string, ArgumentDefinition> $arguments Subscription filter arguments
     * @param string|null $description Optional description
     * @param string|null $topic The LISTEN/NOTIFY channel or CDC topic
     * @param string|null $operation The operation filter (CREATE, UPDATE, DELETE)
     */
    public function __construct(
        public readonly string $name,
        public readonly string $entityType,
        public readonly bool $nullable = false,
        public readonly array $arguments = [],
        public readonly ?string $description = null,
        public readonly ?string $topic = null,
        public readonly ?string $operation = null,
    ) {
    }

    /**
     * Convert to array for JSON serialization.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $data = [
            'name' => $this->name,
            'entity_type' => $this->entityType,
            'nullable' => $this->nullable,
        ];

        if (!empty($this->arguments)) {
            $data['arguments'] = array_map(
                fn(ArgumentDefinition $arg) => [
                    'name' => $arg->name,
                    'type' => $arg->type,
                    'nullable' => $arg->nullable,
                ],
                array_values($this->arguments),
            );
        }

        if ($this->description !== null) {
            $data['description'] = $this->description;
        }

        if ($this->topic !== null) {
            $data['topic'] = $this->topic;
        }

        if ($this->operation !== null) {
            $data['operation'] = $this->operation;
        }

        return $data;
    }
}
