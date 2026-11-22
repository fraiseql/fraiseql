<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Scalars;
use FraiseQL\SchemaExporter;
use FraiseQL\SchemaRegistry;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

/**
 * `Scalars::LOCAL_DATE_TIME` declares a date and time with no offset, a PostgreSQL
 * `timestamp`. PHP's date-time classes always carry a zone, so they stay `DateTime`.
 */
final class LocalDateTimeTest extends TestCase
{
    protected function setUp(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
    }

    public function testTheMarkerIsTheBuiltInScalar(): void
    {
        self::assertSame('LocalDateTime', Scalars::LOCAL_DATE_TIME);
        self::assertTrue(Scalars::isScalarType('LocalDateTime'));
        self::assertContains('LocalDateTime', Scalars::getScalarNames());
    }

    public function testAMarkedFieldAndArgumentExportAsLocalDateTimeAndDateTimeStaysDateTime(): void
    {
        StaticAPI::register(LocalDateTimeShift::class);
        StaticAPI::query('shifts')
            ->returnType('LocalDateTimeShift')
            ->returnsList(true)
            ->sqlSource('v_shift')
            ->argument('startsAfter', Scalars::LOCAL_DATE_TIME, nullable: false)
            ->register();

        $schema = SchemaExporter::toArray();
        $fields = array_column($schema['types'][0]['fields'], 'type', 'name');
        self::assertSame(
            ['startsAt' => 'LocalDateTime', 'endsAt' => 'LocalDateTime', 'loggedAt' => 'DateTime'],
            $fields,
        );
        self::assertSame('LocalDateTime', $schema['queries'][0]['arguments'][0]['type']);
    }
}

#[GraphQLType(name: 'LocalDateTimeShift', sqlSource: 'v_shift')]
final class LocalDateTimeShift
{
    #[GraphQLField(type: Scalars::LOCAL_DATE_TIME, nullable: false)]
    public \DateTimeImmutable $startsAt;

    #[GraphQLField(type: Scalars::LOCAL_DATE_TIME, nullable: true)]
    public ?\DateTimeImmutable $endsAt;

    #[GraphQLField]
    public \DateTimeImmutable $loggedAt;
}
