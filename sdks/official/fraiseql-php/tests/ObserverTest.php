<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\ObserverAction;
use FraiseQL\ObserverRetry;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

/**
 * Observer authoring (#2147): an observer per action kind, as the compiler reads it.
 */
final class ObserverTest extends TestCase
{
    protected function setUp(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    /** @return list<array<string, mixed>> */
    private static function observers(): array
    {
        return SchemaExporter::toArray()['observers'];
    }

    public function testAWebhookObserverCarriesEveryKeyItIsGiven(): void
    {
        StaticAPI::observer('orderPlacedWebhook')
            ->entity('Order')
            ->event('INSERT')
            ->action(ObserverAction::webhook(
                urlEnv: 'HOOK_URL',
                method: 'PUT',
                bodyTemplate: '{"total": {{ total }}}',
                signingSecretEnv: 'HOOK_KEY',
                headers: ['X-Source' => 'tests'],
                headersEnv: ['Authorization' => 'HOOK_TOKEN'],
            ))
            ->retry(new ObserverRetry(
                maxAttempts: 5,
                backoffStrategy: 'linear',
                initialDelayMs: 200,
                maxDelayMs: 5000,
            ))
            ->register();

        $this->assertSame([[
            'name' => 'orderPlacedWebhook',
            'entity' => 'Order',
            'event' => 'INSERT',
            'actions' => [[
                'type' => 'webhook',
                'url_env' => 'HOOK_URL',
                'method' => 'PUT',
                'body_template' => '{"total": {{ total }}}',
                'signing_secret_env' => 'HOOK_KEY',
                'headers' => ['X-Source' => 'tests'],
                'headers_env' => ['Authorization' => 'HOOK_TOKEN'],
            ]],
            'retry' => [
                'max_attempts' => 5,
                'backoff_strategy' => 'linear',
                'initial_delay_ms' => 200,
                'max_delay_ms' => 5000,
            ],
        ]], self::observers());
    }

    public function testAWebhookGivenOnlyItsUrlAuthorsOnlyItsUrl(): void
    {
        $this->assertSame(
            ['type' => 'webhook', 'url' => 'https://hooks.example/x'],
            ObserverAction::webhook('https://hooks.example/x')->toArray(),
        );
    }

    public function testAWebhookWithNoDestinationIsRefused(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('exactly one of $url and $urlEnv');
        ObserverAction::webhook();
    }

    public function testAWebhookWithTwoDestinationsIsRefused(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('exactly one of $url and $urlEnv');
        ObserverAction::webhook('https://hooks.example/x', urlEnv: 'HOOK_URL');
    }

    public function testASlackActionReadsTheDefaultEnvUnlessToldOtherwise(): void
    {
        $this->assertSame([
            'type' => 'slack',
            'channel' => '#orders',
            'message_template' => 'Order {{ id }} shipped',
            'webhook_url_env' => 'SLACK_WEBHOOK_URL',
        ], ObserverAction::slack('#orders', 'Order {{ id }} shipped')->toArray());
        $this->assertSame([
            'type' => 'slack',
            'channel' => '#o',
            'message_template' => 'm',
            'webhook_url_env' => 'OPS_SLACK',
        ], ObserverAction::slack('#o', 'm', webhookUrlEnv: 'OPS_SLACK')->toArray());
        // A written URL is the one destination: the default env is not emitted beside it.
        $this->assertSame([
            'type' => 'slack',
            'channel' => '#o',
            'message_template' => 'm',
            'webhook_url' => 'https://hooks.slack.example/x',
        ], ObserverAction::slack('#o', 'm', webhookUrl: 'https://hooks.slack.example/x')->toArray());
    }

    public function testASlackActionHasOneDestination(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('at most one of $webhookUrl and $webhookUrlEnv');
        ObserverAction::slack('#o', 'm', webhookUrl: 'https://x', webhookUrlEnv: 'OPS_SLACK');
    }

    public function testAnEmailActionNamesItsBodyTemplate(): void
    {
        $this->assertSame([
            'type' => 'email',
            'to' => 'ops@example.com',
            'subject' => 'Order deleted',
            'body_template' => 'Order {{ id }}',
        ], ObserverAction::email('ops@example.com', 'Order deleted', 'Order {{ id }}')->toArray());
    }

    public function testAConditionIsEmittedAndAnUnsetRetryIsLeftOut(): void
    {
        StaticAPI::observer('orderShipped')
            ->entity('Order')
            ->event('UPDATE')
            ->condition("status == 'shipped'")
            ->action(ObserverAction::email('ops@example.com', 's', 'b'))
            ->register();

        $definition = self::observers()[0];
        $this->assertSame("status == 'shipped'", $definition['condition']);
        // No retry: the server's own policy applies, so no block is written.
        $this->assertArrayNotHasKey('retry', $definition);
    }

    public function testNoObserverLeavesTheSectionOut(): void
    {
        $this->assertArrayNotHasKey('observers', SchemaExporter::toArray());
    }

    public function testAnObserverNameIsRegisteredOnce(): void
    {
        $declare = static fn () => StaticAPI::observer('orderCreated')
            ->entity('Order')
            ->event('INSERT')
            ->action(ObserverAction::email('a@example.com', 's', 'b'))
            ->register();
        $declare();
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage("observer 'orderCreated' is already registered");
        $declare();
    }
}
