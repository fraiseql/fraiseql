<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Fluent builder for creating FraiseQL observers.
 *
 * Example:
 * ```php
 * ObserverBuilder::create('onHighValueOrder')
 *     ->entity('Order')
 *     ->event('INSERT')
 *     ->condition('total > 1000')
 *     ->addAction(Webhook::create('https://api.example.com/orders'))
 *     ->addAction(SlackAction::create('#sales', 'New order: {id}'))
 *     ->retry(['max_attempts' => 5, 'backoff_strategy' => 'exponential'])
 *     ->register();
 * ```
 */
final class ObserverBuilder
{
    private string $name;
    private ?string $entity = null;
    private ?string $event = null;
    private ?string $condition = null;

    /** @var array<int, array<string, mixed>> */
    private array $actions = [];

    /** @var array<string, mixed> */
    private array $retry = [
        'max_attempts' => 3,
        'backoff_strategy' => 'exponential',
        'initial_delay_ms' => 100,
        'max_delay_ms' => 60000,
    ];

    private function __construct(string $name)
    {
        $this->name = $name;
    }

    public static function create(string $name): self
    {
        return new self($name);
    }

    public function entity(string $entity): self
    {
        $this->entity = $entity;
        return $this;
    }

    public function event(string $event): self
    {
        $this->event = strtoupper($event);
        return $this;
    }

    public function condition(string $condition): self
    {
        $this->condition = $condition;
        return $this;
    }

    /**
     * @param array<string, mixed> $action
     */
    public function addAction(array $action): self
    {
        $this->actions[] = $action;
        return $this;
    }

    /**
     * @param array<string, mixed> $retry
     */
    public function retry(array $retry): self
    {
        $this->retry = array_merge($this->retry, $retry);
        return $this;
    }

    public function register(): void
    {
        if ($this->entity === null || $this->event === null) {
            throw new FraiseQLException('Observer must have entity and event');
        }

        if (empty($this->actions)) {
            throw new FraiseQLException('Observer must have at least one action');
        }

        $definition = new ObserverDefinition(
            name: $this->name,
            entity: $this->entity,
            event: $this->event,
            actions: $this->actions,
            condition: $this->condition,
            retry: $this->retry,
        );

        SchemaRegistry::getInstance()->registerObserver($definition);
    }
}

/**
 * Helper functions for creating observer actions.
 */
final class Webhook
{
    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function create(string $url, array $options = []): array
    {
        return array_merge([
            'type' => 'webhook',
            'url' => $url,
            'headers' => ['Content-Type' => 'application/json'],
        ], $options);
    }

    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function withEnv(string $urlEnv, array $options = []): array
    {
        $action = array_merge([
            'type' => 'webhook',
            'url_env' => $urlEnv,
            'headers' => ['Content-Type' => 'application/json'],
        ], $options);

        unset($action['url']);
        return $action;
    }
}

final class SlackAction
{
    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function create(string $channel, string $message, array $options = []): array
    {
        return array_merge([
            'type' => 'slack',
            'channel' => $channel,
            'message' => $message,
            'webhook_url_env' => 'SLACK_WEBHOOK_URL',
        ], $options);
    }

    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function withWebhookUrl(string $channel, string $message, string $webhookUrl, array $options = []): array
    {
        $action = self::create($channel, $message, $options);
        $action['webhook_url'] = $webhookUrl;
        unset($action['webhook_url_env']);
        return $action;
    }
}

final class EmailAction
{
    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function create(string $to, string $subject, string $body, array $options = []): array
    {
        return array_merge([
            'type' => 'email',
            'to' => $to,
            'subject' => $subject,
            'body' => $body,
        ], $options);
    }

    /**
     * @param array<string, mixed> $options
     * @return array<string, mixed>
     */
    public static function withFrom(string $to, string $subject, string $body, string $fromEmail, array $options = []): array
    {
        return self::create($to, $subject, $body, array_merge(['from' => $fromEmail], $options));
    }
}
