<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

#[GraphQLType(name: 'Order', sqlSource: 'v_order')]
final class SubscriptionExportOrder
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'Float', nullable: false)]
    public float $total;
}

/**
 * The PHP subscription surface had no test at all, in any file, and it was broken in
 * three independent ways at once (#1024):
 *
 * 1. `SchemaExporter::toArray()` emitted no `subscriptions` key. `SubscriptionBuilder`
 *    registered, `SchemaRegistry` stored, and nothing ever read them back — so a
 *    registered subscription was **silently absent** from the exported document. Not a
 *    compile error: a compile that succeeds having dropped what the author declared.
 * 2. `SubscriptionBuilder::argument()` was a **fatal error**. It constructed
 *    `FraiseQL\ArgumentDefinition`, which lived in `src/ArgumentBuilder.php` and was
 *    therefore not PSR-4 autoloadable — reachable only as a side effect of having
 *    loaded `ArgumentBuilder` first.
 * 3. `SubscriptionDefinition::toArray()` emitted `entity_type`/`nullable`/`operation`,
 *    none of which `IntermediateSubscription` has. It denies unknown fields, so had the
 *    export ever reached the compiler it would have failed the whole document.
 *
 * The assertions are on the exported document, because the document is what
 * `fraiseql compile` reads and every one of the three defects is invisible from inside
 * the builder.
 */
final class SubscriptionExportTest extends TestCase
{
    /** Members of `IntermediateSubscription`, which is `deny_unknown_fields`. */
    private const COMPILER_MEMBERS = [
        'name',
        'return_type',
        'arguments',
        'description',
        'topic',
        'filter',
        'fields',
        'deprecated',
    ];

    protected function setUp(): void
    {
        StaticAPI::clear();
    }

    /** @return array<string, mixed> */
    private function exportedSubscription(): array
    {
        $schema = SchemaExporter::toArray();
        self::assertArrayHasKey('subscriptions', $schema, 'subscriptions absent from export');
        self::assertCount(1, $schema['subscriptions']);
        return $schema['subscriptions'][0];
    }

    public function testRegisteredSubscriptionReachesTheExportedDocument(): void
    {
        StaticAPI::register(SubscriptionExportOrder::class);
        StaticAPI::subscription('orderUpdated')
            ->entityType('Order')
            ->description('Stream of order update events')
            ->build();

        $subscription = $this->exportedSubscription();

        self::assertSame('orderUpdated', $subscription['name']);
        self::assertSame('Order', $subscription['return_type']);
        self::assertSame('Stream of order update events', $subscription['description']);
    }

    public function testArgumentsAreCarriedAndDoNotFatal(): void
    {
        StaticAPI::subscription('orderUpdated')
            ->entityType('Order')
            ->argument('orderId', 'ID', true)
            ->argument('status', 'String', false)
            ->build();

        $subscription = $this->exportedSubscription();

        self::assertCount(2, $subscription['arguments']);
        self::assertSame('orderId', $subscription['arguments'][0]['name']);
        self::assertSame('ID', $subscription['arguments'][0]['type']);
        self::assertTrue($subscription['arguments'][0]['nullable']);
        self::assertFalse($subscription['arguments'][1]['nullable']);
    }

    public function testEmitsNoKeyOutsideTheCompilerMemberList(): void
    {
        StaticAPI::subscription('orderUpdated')
            ->entityType('Order')
            ->description('Stream of order update events')
            ->topic('order_events')
            ->argument('orderId', 'ID', true)
            ->filterCondition('orderId', '$.id')
            ->fields(['id', 'total'])
            ->deprecated('use orderEvents')
            ->build();

        $subscription = $this->exportedSubscription();

        self::assertSame(
            self::COMPILER_MEMBERS,
            array_keys($subscription),
            'emitted keys diverge from IntermediateSubscription',
        );
    }

    public function testOmitsEveryOptionTheAuthorDidNotSet(): void
    {
        StaticAPI::subscription('orderUpdated')->entityType('Order')->build();

        $subscription = $this->exportedSubscription();

        self::assertSame(['name', 'return_type', 'arguments'], array_keys($subscription));
    }

    public function testFilterConditionsBecomeTheCompilersFilterShape(): void
    {
        StaticAPI::subscription('orderNarrowed')
            ->entityType('Order')
            ->argument('orderId', 'ID', true)
            ->argument('status', 'String', true)
            ->filterCondition('orderId', '$.id')
            ->filterCondition('status', '$.order_status')
            ->build();

        $subscription = $this->exportedSubscription();

        self::assertSame(
            ['conditions' => [
                ['argument' => 'orderId', 'path' => '$.id'],
                ['argument' => 'status', 'path' => '$.order_status'],
            ]],
            $subscription['filter'],
        );
    }

    public function testDeprecationIsCanonicalized(): void
    {
        StaticAPI::subscription('withReason')->entityType('Order')->deprecated('gone soon')->build();
        StaticAPI::subscription('bare')->entityType('Order')->deprecated()->build();
        StaticAPI::subscription('live')->entityType('Order')->deprecated(false)->build();

        $subscriptions = [];
        foreach (SchemaExporter::toArray()['subscriptions'] as $subscription) {
            $subscriptions[$subscription['name']] = $subscription;
        }

        self::assertSame(['reason' => 'gone soon'], $subscriptions['withReason']['deprecated']);
        self::assertSame([], $subscriptions['bare']['deprecated']);
        self::assertArrayNotHasKey('deprecated', $subscriptions['live']);
    }

    public function testArgumentDefinitionIsAutoloadableOnItsOwn(): void
    {
        // Pins the PSR-4 fix directly: before it, this class was only reachable once
        // `FraiseQL\ArgumentBuilder` had been loaded, so `argument()` fatalled in any
        // process that had not happened to touch the builder.
        self::assertTrue(
            class_exists(\FraiseQL\ArgumentDefinition::class, autoload: true),
            'FraiseQL\ArgumentDefinition is not PSR-4 autoloadable',
        );
    }
}
