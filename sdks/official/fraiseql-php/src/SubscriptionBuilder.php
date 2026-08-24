<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for constructing GraphQL subscriptions.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * There is no `nullable()` and no `operation()`: the runtime subscription model has
 * neither member. Where a CREATE/UPDATE/DELETE filter was wanted, the event payload
 * carries the verb and a `filter()` condition selects on it (#1024).
 *
 * Usage:
 * ```php
 * SubscriptionBuilder::subscription('orderCreated')
 *     ->entityType('Order')
 *     ->topic('order_events')
 *     ->argument('orderId', 'ID')
 *     ->filterCondition('orderId', '$.id')
 *     ->description('Subscribe to new orders')
 *     ->build();
 * ```
 */
final class SubscriptionBuilder
{
    private string $name;
    private string $entityType = '';
    /** @var array<string, ArgumentDefinition> */
    private array $arguments = [];
    private ?string $description = null;
    private ?string $topic = null;
    /** @var array<int, array{argument: string, path: string}> */
    private array $filterConditions = [];
    /** @var array<int, string> */
    private array $fields = [];
    private bool|string|null $deprecated = null;

    private function __construct(string $name)
    {
        $this->name = $name;
    }

    /**
     * Create a new subscription builder.
     *
     * @param string $name The subscription name
     * @return self
     */
    public static function subscription(string $name): self
    {
        return new self($name);
    }

    /**
     * Set the entity type for this subscription — the compiler's `return_type`.
     *
     * @param string $entityType The GraphQL type name
     * @return self
     */
    public function entityType(string $entityType): self
    {
        $this->entityType = $entityType;
        return $this;
    }

    /**
     * Add an argument for filtering subscription events.
     *
     * @param string $name Argument name
     * @param string $type GraphQL type
     * @param bool $argNullable Whether argument is nullable
     * @return self
     */
    public function argument(string $name, string $type, bool $argNullable = true): self
    {
        $this->arguments[$name] = new ArgumentDefinition(
            name: $name,
            type: $type,
            nullable: $argNullable,
        );
        return $this;
    }

    /**
     * Set the description for this subscription.
     *
     * @param string $description The description
     * @return self
     */
    public function description(string $description): self
    {
        $this->description = $description;
        return $this;
    }

    /**
     * Set the topic/channel for this subscription.
     *
     * @param string $topic The LISTEN/NOTIFY channel or CDC topic
     * @return self
     */
    public function topic(string $topic): self
    {
        $this->topic = $topic;
        return $this;
    }

    /**
     * Narrow delivered events by matching one argument against a path in the payload.
     *
     * @param string $argument Name of a declared subscription argument
     * @param string $path JSON path into the event data, e.g. '$.id'
     * @return self
     */
    public function filterCondition(string $argument, string $path): self
    {
        $this->filterConditions[] = ['argument' => $argument, 'path' => $path];
        return $this;
    }

    /**
     * Project a subset of the event's fields. Every field is delivered if unset.
     *
     * @param array<int, string> $fields Field names
     * @return self
     */
    public function fields(array $fields): self
    {
        $this->fields = $fields;
        return $this;
    }

    /**
     * Mark this subscription deprecated.
     *
     * @param bool|string $deprecated True, or the reason
     * @return self
     */
    public function deprecated(bool|string $deprecated = true): self
    {
        $this->deprecated = $deprecated;
        return $this;
    }

    /**
     * Build and register the subscription definition.
     *
     * @return SubscriptionDefinition
     */
    public function build(): SubscriptionDefinition
    {
        $definition = new SubscriptionDefinition(
            name: $this->name,
            entityType: $this->entityType,
            arguments: $this->arguments,
            description: $this->description,
            topic: $this->topic,
            filter: $this->filterConditions === []
                ? null
                : ['conditions' => $this->filterConditions],
            fields: $this->fields,
            deprecated: $this->deprecated,
        );

        SchemaRegistry::getInstance()->registerSubscription($definition);

        return $definition;
    }

    /**
     * Get the subscription name.
     *
     * @return string
     */
    public function getName(): string
    {
        return $this->name;
    }

    /**
     * Get the entity type.
     *
     * @return string
     */
    public function getEntityType(): string
    {
        return $this->entityType;
    }
}
