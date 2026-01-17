<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\StaticAPI;
use FraiseQL\SchemaRegistry;
use FraiseQL\SubscriptionBuilder;
use FraiseQL\SubscriptionDefinition;

/**
 * Tests for GraphQL subscription support.
 * Subscriptions in FraiseQL are compiled projections of database events.
 * They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.
 */
final class SubscriptionTest extends TestCase
{
    protected function tearDown(): void
    {
        StaticAPI::clear();
        parent::tearDown();
    }

    public function testSimpleSubscription(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->description('Subscribe to new orders')
            ->build();

        $subscription = StaticAPI::getSubscription('orderCreated');
        $this->assertNotNull($subscription);
        $this->assertSame('orderCreated', $subscription->name);
        $this->assertSame('Order', $subscription->entityType);
        $this->assertSame('Subscribe to new orders', $subscription->description);
    }

    public function testSubscriptionWithTopic(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->topic('order_events')
            ->description('Subscribe to new orders')
            ->build();

        $subscription = StaticAPI::getSubscription('orderCreated');
        $this->assertNotNull($subscription);
        $this->assertSame('order_events', $subscription->topic);
    }

    public function testSubscriptionWithOperation(): void
    {
        StaticAPI::subscription('userUpdated')
            ->entityType('User')
            ->operation('UPDATE')
            ->description('Subscribe to user updates')
            ->build();

        $subscription = StaticAPI::getSubscription('userUpdated');
        $this->assertNotNull($subscription);
        $this->assertSame('UPDATE', $subscription->operation);
    }

    public function testSubscriptionWithArguments(): void
    {
        StaticAPI::subscription('orderStatusChanged')
            ->entityType('Order')
            ->argument('userId', 'String', true)
            ->argument('status', 'String', true)
            ->description('Subscribe to order status changes')
            ->build();

        $subscription = StaticAPI::getSubscription('orderStatusChanged');
        $this->assertNotNull($subscription);
        $this->assertCount(2, $subscription->arguments);
        $this->assertArrayHasKey('userId', $subscription->arguments);
        $this->assertArrayHasKey('status', $subscription->arguments);
    }

    public function testNullableSubscription(): void
    {
        StaticAPI::subscription('userDeleted')
            ->entityType('User')
            ->nullable(true)
            ->description('Subscribe to user deletions')
            ->build();

        $subscription = StaticAPI::getSubscription('userDeleted');
        $this->assertNotNull($subscription);
        $this->assertTrue($subscription->nullable);
    }

    public function testMultipleSubscriptions(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->build();

        StaticAPI::subscription('orderUpdated')
            ->entityType('Order')
            ->operation('UPDATE')
            ->build();

        StaticAPI::subscription('userCreated')
            ->entityType('User')
            ->build();

        $subscriptions = StaticAPI::getAllSubscriptions();
        $this->assertCount(3, $subscriptions);
        $this->assertTrue(StaticAPI::hasSubscription('orderCreated'));
        $this->assertTrue(StaticAPI::hasSubscription('orderUpdated'));
        $this->assertTrue(StaticAPI::hasSubscription('userCreated'));
    }

    public function testClearRemovesSubscriptions(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->build();

        $this->assertTrue(StaticAPI::hasSubscription('orderCreated'));

        StaticAPI::clear();

        $this->assertFalse(StaticAPI::hasSubscription('orderCreated'));
        $this->assertEmpty(StaticAPI::getAllSubscriptions());
    }

    public function testCompleteSubscription(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->argument('storeId', 'Int', false)
            ->topic('order_events')
            ->operation('CREATE')
            ->description('Subscribe to new orders')
            ->build();

        $subscription = StaticAPI::getSubscription('orderCreated');
        $this->assertNotNull($subscription);
        $this->assertSame('orderCreated', $subscription->name);
        $this->assertSame('Order', $subscription->entityType);
        $this->assertSame('order_events', $subscription->topic);
        $this->assertSame('CREATE', $subscription->operation);
        $this->assertSame('Subscribe to new orders', $subscription->description);
        $this->assertCount(1, $subscription->arguments);
    }

    public function testSubscriptionToArray(): void
    {
        StaticAPI::subscription('orderCreated')
            ->entityType('Order')
            ->topic('order_events')
            ->operation('CREATE')
            ->description('Subscribe to new orders')
            ->build();

        $subscription = StaticAPI::getSubscription('orderCreated');
        $this->assertNotNull($subscription);

        $array = $subscription->toArray();
        $this->assertSame('orderCreated', $array['name']);
        $this->assertSame('Order', $array['entity_type']);
        $this->assertFalse($array['nullable']);
        $this->assertSame('order_events', $array['topic']);
        $this->assertSame('CREATE', $array['operation']);
        $this->assertSame('Subscribe to new orders', $array['description']);
    }

    public function testSubscriptionBuilderDirectly(): void
    {
        $definition = SubscriptionBuilder::subscription('testSub')
            ->entityType('TestEntity')
            ->topic('test_channel')
            ->build();

        $this->assertInstanceOf(SubscriptionDefinition::class, $definition);
        $this->assertSame('testSub', $definition->name);
        $this->assertSame('TestEntity', $definition->entityType);
    }
}
