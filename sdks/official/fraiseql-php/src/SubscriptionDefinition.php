<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Definition for a GraphQL subscription.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * `toArray()` emits the compiler's `IntermediateSubscription`, member for member. It
 * used to emit `entity_type`/`nullable`/`operation`, none of which that struct has —
 * and it denies unknown fields — but no PHP schema ever carried a subscription to the
 * compiler to find out: `SchemaExporter` emitted no `subscriptions` key at all, so a
 * registered subscription was silently dropped from the document (#1024).
 *
 * `entityType` survives as the authoring spelling of `return_type`.
 *
 * Usage:
 * ```php
 * $subscription = new SubscriptionDefinition(
 *     name: 'orderCreated',
 *     entityType: 'Order',
 *     description: 'Subscribe to new orders',
 *     topic: 'order_events',
 *     filter: ['conditions' => [['argument' => 'orderId', 'path' => '$.id']]],
 * );
 * ```
 */
final class SubscriptionDefinition
{
    /**
     * @param string $name The subscription name
     * @param string $entityType The entity type being subscribed to (the return type)
     * @param array<string, ArgumentDefinition> $arguments Subscription filter arguments
     * @param string|null $description Optional description
     * @param string|null $topic The LISTEN/NOTIFY channel or CDC topic
     * @param array{conditions: array<int, array{argument: string, path: string}>}|null $filter
     *        Maps arguments onto JSON paths in the event payload
     * @param array<int, string> $fields Subset of event fields to project; all if empty
     * @param bool|string|null $deprecated True, or the reason
     */
    public function __construct(
        public readonly string $name,
        public readonly string $entityType,
        public readonly array $arguments = [],
        public readonly ?string $description = null,
        public readonly ?string $topic = null,
        public readonly ?array $filter = null,
        public readonly array $fields = [],
        public readonly bool|string|null $deprecated = null,
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
            'name'        => $this->name,
            'return_type' => $this->entityType,
            'arguments'   => array_map(
                fn(ArgumentDefinition $arg) => [
                    'name'     => $arg->name,
                    'type'     => $arg->type,
                    'nullable' => $arg->nullable,
                ],
                array_values($this->arguments),
            ),
        ];

        if ($this->description !== null) {
            $data['description'] = $this->description;
        }

        if ($this->topic !== null) {
            $data['topic'] = $this->topic;
        }

        if ($this->filter !== null) {
            $data['filter'] = $this->filter;
        }

        if (!empty($this->fields)) {
            $data['fields'] = array_values($this->fields);
        }

        // `true` means deprecated with no stated reason, which the compiler models as an
        // absent `reason`. `false`/null means not deprecated, so the key is dropped
        // rather than emitted as an empty deprecation.
        if ($this->deprecated !== null && $this->deprecated !== false) {
            $data['deprecated'] = is_string($this->deprecated)
                ? ['reason' => $this->deprecated]
                : [];
        }

        return $data;
    }
}
