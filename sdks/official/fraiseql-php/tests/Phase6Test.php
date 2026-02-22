<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaCache;
use FraiseQL\PerformanceMonitor;
use FraiseQL\LazyLoader;
use FraiseQL\SchemaRegistry;
use FraiseQL\TypeBuilder;
use FraiseQL\SchemaFormatter;
use FraiseQL\JsonSchema;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\GraphQLField;

/**
 * Tests for Phase 6: Optimization
 * - SchemaCache for caching compiled schemas
 * - PerformanceMonitor for tracking metrics
 * - LazyLoader for on-demand type loading
 */
final class Phase6Test extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    // ============ SchemaCache Tests ============

    public function testSchemaCacheBasic(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $cache->cacheFormattedSchema(SchemaRegistry::getInstance(), $schema);
        $retrieved = $cache->getFormattedSchema(SchemaRegistry::getInstance());

        $this->assertNotNull($retrieved);
        $this->assertSame($schema->version, $retrieved->version);
    }

    public function testSchemaCacheMiss(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        // Don't cache anything
        $retrieved = $cache->getFormattedSchema(SchemaRegistry::getInstance());
        $this->assertNull($retrieved);
    }

    public function testSchemaCacheStats(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        // First access: miss
        $cache->getFormattedSchema(SchemaRegistry::getInstance());
        // Cache it
        $cache->cacheFormattedSchema(SchemaRegistry::getInstance(), $schema);
        // Second access: hit
        $cache->getFormattedSchema(SchemaRegistry::getInstance());

        $this->assertSame(1, $cache->getHits());
        $this->assertSame(1, $cache->getMisses());
        $this->assertGreaterThan(0, $cache->getHitRatio());
    }

    public function testSchemaCacheClear(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $cache->cacheFormattedSchema(SchemaRegistry::getInstance(), $schema);
        $cache->clear();

        $retrieved = $cache->getFormattedSchema(SchemaRegistry::getInstance());
        $this->assertNull($retrieved);
    }

    public function testSchemaCacheJsonCaching(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());
        $json = $schema->toJson();

        $cache->cacheJson($schema, $json);
        $retrieved = $cache->getJson($schema);

        $this->assertNotNull($retrieved);
        $this->assertSame($json, $retrieved);
    }

    public function testSchemaCacheValidation(): void
    {
        $cache = new SchemaCache();
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $cache->cacheValidation(SchemaRegistry::getInstance(), true);
        $result = $cache->getValidation(SchemaRegistry::getInstance());

        $this->assertTrue($result);
    }

    public function testSchemaCacheTTL(): void
    {
        $cache = new SchemaCache(ttl: 1); // 1 second TTL
        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());

        $cache->cacheFormattedSchema(SchemaRegistry::getInstance(), $schema);
        $this->assertNotNull($cache->getFormattedSchema(SchemaRegistry::getInstance()));

        // Sleep to let TTL expire
        sleep(2);

        // Should be expired now
        $this->assertNull($cache->getFormattedSchema(SchemaRegistry::getInstance()));
    }

    // ============ PerformanceMonitor Tests ============

    public function testPerformanceMonitorBasic(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('test_op');
        usleep(100000); // 0.1 seconds
        $duration = $monitor->endOperation('test_op');

        $this->assertGreaterThan(0.09, $duration);
        $this->assertLessThan(0.2, $duration);
    }

    public function testPerformanceMonitorMultiple(): void
    {
        $monitor = new PerformanceMonitor();

        for ($i = 0; $i < 5; $i++) {
            $monitor->startOperation('fast_op');
            usleep(10000); // 0.01 seconds
            $monitor->endOperation('fast_op');
        }

        $metrics = $monitor->getOperationMetrics('fast_op');
        $this->assertSame(5, $metrics['count']);
        $this->assertGreaterThan(0, $metrics['total_time']);
    }

    public function testPerformanceMonitorGetMetrics(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('operation_a');
        usleep(50000);
        $monitor->endOperation('operation_a');

        $metrics = $monitor->getOperationMetrics('operation_a');
        $this->assertIsArray($metrics);
        $this->assertArrayHasKey('name', $metrics);
        $this->assertArrayHasKey('count', $metrics);
        $this->assertArrayHasKey('average_time', $metrics);
        $this->assertArrayHasKey('min_time', $metrics);
        $this->assertArrayHasKey('max_time', $metrics);
    }

    public function testPerformanceMonitorSlowest(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('fast');
        usleep(10000);
        $monitor->endOperation('fast');

        $monitor->startOperation('slow');
        usleep(100000);
        $monitor->endOperation('slow');

        $slowest = $monitor->getSlowestOperation();
        $this->assertSame('slow', $slowest['name']);
    }

    public function testPerformanceMonitorFastest(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('fast');
        usleep(10000);
        $monitor->endOperation('fast');

        $monitor->startOperation('slow');
        usleep(100000);
        $monitor->endOperation('slow');

        $fastest = $monitor->getFastestOperation();
        $this->assertSame('fast', $fastest['name']);
    }

    public function testPerformanceMonitorTopSlow(): void
    {
        $monitor = new PerformanceMonitor();

        for ($i = 0; $i < 10; $i++) {
            $monitor->startOperation("op_$i");
            usleep($i * 10000);
            $monitor->endOperation("op_$i");
        }

        $top = $monitor->getTopSlowOperations(3);
        $this->assertCount(3, $top);
        // First should be slowest
        $this->assertSame('op_9', $top[0]['name']);
    }

    public function testPerformanceMonitorTotalTime(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('op1');
        usleep(50000);
        $monitor->endOperation('op1');

        $monitor->startOperation('op2');
        usleep(50000);
        $monitor->endOperation('op2');

        $total = $monitor->getTotalTime();
        $this->assertGreaterThan(0.09, $total);
    }

    public function testPerformanceMonitorDisable(): void
    {
        $monitor = new PerformanceMonitor();
        $monitor->disable();

        $monitor->startOperation('should_not_record');
        $duration = $monitor->endOperation('should_not_record');

        $this->assertSame(0.0, $duration);
        $this->assertNull($monitor->getOperationMetrics('should_not_record'));
    }

    public function testPerformanceMonitorEnable(): void
    {
        $monitor = new PerformanceMonitor();
        $monitor->disable();
        $monitor->enable();

        $monitor->startOperation('recorded');
        usleep(10000);
        $monitor->endOperation('recorded');

        $this->assertNotNull($monitor->getOperationMetrics('recorded'));
    }

    public function testPerformanceMonitorReport(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('test');
        usleep(10000);
        $monitor->endOperation('test');

        $report = $monitor->getReport();
        $this->assertIsString($report);
        $this->assertStringContainsString('Performance Report', $report);
        $this->assertStringContainsString('test', $report);
    }

    public function testPerformanceMonitorSummary(): void
    {
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('op1');
        usleep(50000);
        $monitor->endOperation('op1');

        $summary = $monitor->getSummary();
        $this->assertArrayHasKey('total_time', $summary);
        $this->assertArrayHasKey('total_operations', $summary);
        $this->assertArrayHasKey('average_time', $summary);
    }

    // ============ LazyLoader Tests ============

    public function testLazyLoaderBasic(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $registered = false;
        $loader->registerLoader('TestType', function () use (&$registered) {
            $registered = true;
        });

        $this->assertFalse($registered);

        $loader->ensureLoaded('TestType');
        $this->assertTrue($registered);
    }

    public function testLazyLoaderMultipleTypes(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Type1', function () {});
        $loader->registerLoader('Type2', function () {});
        $loader->registerLoader('Type3', function () {});

        $this->assertCount(3, $loader->getRegisteredTypes());
        $this->assertSame(0, $loader->getLoadedCount());
    }

    public function testLazyLoaderEnsureAllLoaded(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $count = 0;
        $loader->registerLoader('Type1', function () use (&$count) {
            $count++;
        });
        $loader->registerLoader('Type2', function () use (&$count) {
            $count++;
        });
        $loader->registerLoader('Type3', function () use (&$count) {
            $count++;
        });

        $loaded = $loader->ensureAllLoaded();
        $this->assertSame(3, $loaded);
        $this->assertSame(3, $count);
    }

    public function testLazyLoaderLoadingProgress(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Type1', function () {});
        $loader->registerLoader('Type2', function () {});
        $loader->registerLoader('Type3', function () {});

        $this->assertSame(0.0, $loader->getLoadingProgress());

        $loader->ensureLoaded('Type1');
        $this->assertGreaterThan(0, $loader->getLoadingProgress());
        $this->assertLessThan(100, $loader->getLoadingProgress());

        $loader->ensureAllLoaded();
        $this->assertSame(100.0, $loader->getLoadingProgress());
    }

    public function testLazyLoaderStats(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Type1', function () {});
        $loader->registerLoader('Type2', function () {});

        $loader->ensureLoaded('Type1');

        $stats = $loader->getStats();
        $this->assertSame(2, $stats['total_types']);
        $this->assertSame(1, $stats['loaded_types']);
        $this->assertSame(1, $stats['unloaded_types']);
    }

    public function testLazyLoaderUnloadedTypes(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Type1', function () {});
        $loader->registerLoader('Type2', function () {});
        $loader->registerLoader('Type3', function () {});

        $unloaded = $loader->getUnloadedTypes();
        $this->assertContains('Type1', $unloaded);
        $this->assertContains('Type2', $unloaded);
        $this->assertContains('Type3', $unloaded);
    }

    public function testLazyLoaderHasLoader(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Exists', function () {});

        $this->assertTrue($loader->hasLoader('Exists'));
        $this->assertFalse($loader->hasLoader('DoesNotExist'));
    }

    public function testLazyLoaderClear(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());

        $loader->registerLoader('Type1', function () {});
        $loader->ensureLoaded('Type1');

        $loader->clear();

        $this->assertCount(0, $loader->getRegisteredTypes());
        $this->assertCount(0, $loader->getLoadedTypes());
    }

    // ============ Integration Tests ============

    public function testCacheAndMonitorIntegration(): void
    {
        $cache = new SchemaCache();
        $monitor = new PerformanceMonitor();

        SchemaRegistry::getInstance()->register(CacheTestUser::class);

        $monitor->startOperation('format_schema');
        $formatter = new SchemaFormatter();
        $schema = $formatter->formatRegistry(SchemaRegistry::getInstance());
        $monitor->endOperation('format_schema');

        $monitor->startOperation('cache_schema');
        $cache->cacheFormattedSchema(SchemaRegistry::getInstance(), $schema);
        $monitor->endOperation('cache_schema');

        $monitor->startOperation('retrieve_from_cache');
        $retrieved = $cache->getFormattedSchema(SchemaRegistry::getInstance());
        $monitor->endOperation('retrieve_from_cache');

        $this->assertNotNull($retrieved);
        $metrics = $monitor->getAllMetrics();
        $this->assertCount(3, $metrics);
    }

    public function testLazyLoaderWithMonitor(): void
    {
        $loader = new LazyLoader(SchemaRegistry::getInstance());
        $monitor = new PerformanceMonitor();

        $monitor->startOperation('register_loaders');

        $loader->registerLoader('Type1', function () {});
        $loader->registerLoader('Type2', function () {});
        $loader->registerLoader('Type3', function () {});

        $monitor->endOperation('register_loaders');

        $monitor->startOperation('load_all');
        $loader->ensureAllLoaded();
        $monitor->endOperation('load_all');

        $this->assertGreaterThan(0, $monitor->getTotalTime());
        $this->assertSame(100.0, $loader->getLoadingProgress());
    }
}

// Test fixtures
#[GraphQLType(name: 'CacheTestUser')]
final class CacheTestUser
{
    #[GraphQLField(type: 'Int')]
    public int $id;

    #[GraphQLField(type: 'String')]
    public string $name;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $email;
}
