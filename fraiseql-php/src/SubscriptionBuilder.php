<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for constructing GraphQL subscriptions.
 *
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 *
 * Usage:
 * ```php
 * SubscriptionBuilder::subscription('orderCreated')
 *     ->entityType('Order')
 *     ->topic('order_events')
 *     ->operation('CREATE')
 *     ->description('Subscribe to new orders')
 *     ->build();
 * ```
 */
final class SubscriptionBuilder
{
    private string $name;
    private string $entityType = '';
    private bool $nullable = false;
    /** @var array<string, ArgumentDefinition> */
    private array $arguments = [];
    private ?string $description = null;
    private ?string $topic = null;
    private ?string $operation = null;

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
     * Set the entity type for this subscription.
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
     * Set whether the subscription can return null.
     *
     * @param bool $nullable Whether null is allowed
     * @return self
     */
    public function nullable(bool $nullable = true): self
    {
        $this->nullable = $nullable;
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
     * Set the operation filter for this subscription.
     *
     * @param string $operation The operation (CREATE, UPDATE, DELETE)
     * @return self
     */
    public function operation(string $operation): self
    {
        $this->operation = $operation;
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
            nullable: $this->nullable,
            arguments: $this->arguments,
            description: $this->description,
            topic: $this->topic,
            operation: $this->operation,
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
