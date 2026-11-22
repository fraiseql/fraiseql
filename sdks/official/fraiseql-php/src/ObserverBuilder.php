<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Builds an observer (#2147): actions run when a row of an entity is inserted, updated
 * or deleted.
 *
 * ```php
 * StaticAPI::observer('orderShipped')
 *     ->entity('Order')
 *     ->event('UPDATE')
 *     ->condition("status == 'shipped'")
 *     ->action(ObserverAction::slack('#orders', 'Order {{ id }} shipped'))
 *     ->register();
 * ```
 */
final class ObserverBuilder
{
    private string $entity = '';
    private string $event = '';
    private ?string $condition = null;
    /** @var list<ObserverAction> */
    private array $actions = [];
    private ?ObserverRetry $retry = null;

    private function __construct(public readonly string $name)
    {
    }

    public static function observer(string $name): self
    {
        return new self($name);
    }

    /** The type whose rows are observed. */
    public function entity(string $entity): self
    {
        $this->entity = $entity;
        return $this;
    }

    /** `INSERT`, `UPDATE` or `DELETE`. */
    public function event(string $event): self
    {
        $this->event = $event;
        return $this;
    }

    /** An expression on the row; the actions run only when it holds. */
    public function condition(string $condition): self
    {
        $this->condition = $condition;
        return $this;
    }

    public function action(ObserverAction $action): self
    {
        $this->actions[] = $action;
        return $this;
    }

    /** How a failed action is retried; with none, the server's own policy applies. */
    public function retry(ObserverRetry $retry): self
    {
        $this->retry = $retry;
        return $this;
    }

    /**
     * @throws \InvalidArgumentException An observer of this name is already registered.
     */
    public function register(): void
    {
        SchemaRegistry::getInstance()->registerObserver($this);
    }

    /** @return array<string, mixed> */
    public function toArray(): array
    {
        $definition = ['name' => $this->name, 'entity' => $this->entity, 'event' => $this->event];
        if ($this->condition !== null) {
            $definition['condition'] = $this->condition;
        }
        $definition['actions'] = array_map(
            static fn (ObserverAction $action): array => $action->toArray(),
            $this->actions,
        );
        if ($this->retry !== null) {
            $definition['retry'] = $this->retry->toArray();
        }
        return $definition;
    }
}
