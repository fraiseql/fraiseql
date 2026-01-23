<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Represents an observer definition in the schema.
 *
 * Observers react to database change events with configurable actions and retry logic.
 */
final class ObserverDefinition
{
    /**
     * @param string $name Observer unique identifier
     * @param string $entity Entity type to observe (e.g., "Order")
     * @param string $event Event type (INSERT, UPDATE, DELETE)
     * @param array<int, array<string, mixed>> $actions Actions to execute (webhooks, Slack, email)
     * @param string|null $condition Optional condition expression in FraiseQL DSL
     * @param array<string, mixed> $retry Retry configuration
     */
    public function __construct(
        public readonly string $name,
        public readonly string $entity,
        public readonly string $event,
        public readonly array $actions,
        public readonly ?string $condition = null,
        public readonly array $retry = [
            'max_attempts' => 3,
            'backoff_strategy' => 'exponential',
            'initial_delay_ms' => 100,
            'max_delay_ms' => 60000,
        ],
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
            'entity' => $this->entity,
            'event' => $this->event,
            'actions' => $this->actions,
            'retry' => $this->retry,
        ];

        if ($this->condition !== null) {
            $data['condition'] = $this->condition;
        }

        return $data;
    }
}
