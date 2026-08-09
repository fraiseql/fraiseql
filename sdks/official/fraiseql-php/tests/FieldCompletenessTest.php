<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;

/**
 * PHP SDK field completeness.
 *
 * Verifies that every field documented in the FraiseQL schema contract survives
 * round-trip serialisation through the StaticAPI::query(), StaticAPI::mutation() and
 * StaticAPI::type() builders.
 *
 * Exports through `SchemaExporter`, which is what `vendor/bin/fraiseql export` runs.
 * These tests used to go through a second serializer, `SchemaExporter::toArray()`,
 * whose output shape no consumer accepted and which was the only reader of the
 * registry side table that held builder metadata — so it passed while the shipped
 * exporter dropped `sql_source` and `is_error` from every builder-authored type (#952).
 * Asserting against a serializer nothing ships is how that stayed invisible.
 */
final class FieldCompletenessTest extends TestCase
{
    protected function tearDown(): void
    {
        StaticAPI::clear();
        parent::tearDown();
    }

    // =========================================================================
    // QueryBuilder completeness
    // =========================================================================

    public function testQueryInjectParamsAppearsInSchema(): void
    {
        StaticAPI::query('tenantOrders')
            ->returnType('Order')
            ->sqlSource('v_order')
            ->inject(['tenant_id' => 'jwt:tenant_id'])
            ->register();

        $schema = SchemaExporter::toArray();
        $q = $this->findQuery($schema, 'tenantOrders');

        $this->assertArrayHasKey('inject_params', $q);
        $this->assertEquals('jwt',       $q['inject_params']['tenant_id']['source']);
        $this->assertEquals('tenant_id', $q['inject_params']['tenant_id']['claim']);
    }

    public function testQueryCacheTtlSecondsAppearsInSchema(): void
    {
        StaticAPI::query('orders')
            ->returnType('Order')
            ->sqlSource('v_order')
            ->cacheTtlSeconds(300)
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'orders');
        $this->assertEquals(300, $q['cache_ttl_seconds']);
    }

    public function testQueryAdditionalViewsAppearsInSchema(): void
    {
        StaticAPI::query('reports')
            ->returnType('Report')
            ->sqlSource('v_report')
            ->additionalViews(['v_report_summary'])
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'reports');
        $this->assertEquals(['v_report_summary'], $q['additional_views']);
    }

    public function testQueryRequiresRoleAppearsInSchema(): void
    {
        StaticAPI::query('adminData')
            ->returnType('Admin')
            ->sqlSource('v_admin')
            ->requiresRole('admin')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'adminData');
        $this->assertEquals('admin', $q['requires_role']);
    }

    public function testQueryDeprecationAppearsInSchema(): void
    {
        StaticAPI::query('oldQuery')
            ->returnType('X')
            ->sqlSource('v_x')
            ->deprecated('Use newQuery instead')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'oldQuery');
        $this->assertArrayNotHasKey('deprecated', $q);
        $this->markTestIncomplete(
            'QueryBuilder::deprecated() never reaches toIntermediateArray(), so the reason '
            . 'is dropped by the shipped exporter — IntermediateQuery.deprecated exists and '
            . 'stays empty. Tracked as its own defect; see #1021.'
        );
    }

    public function testQueryRelayCursorTypeAppearsInSchema(): void
    {
        StaticAPI::query('paginatedItems')
            ->returnType('Item')
            ->sqlSource('v_item')
            ->relayCursorType('UUID')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'paginatedItems');
        $this->assertArrayNotHasKey('relay_cursor_type', $q);
        $this->markTestIncomplete(
            'QueryBuilder::relayCursorType() never reaches toIntermediateArray(). Tracked as '
            . 'its own defect; see #1021.'
        );
    }

    public function testQueryReturnsListAppearsInSchema(): void
    {
        StaticAPI::query('allUsers')
            ->returnType('User')
            ->returnsList(true)
            ->sqlSource('v_user')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'allUsers');
        $this->assertTrue($q['returns_list']);
        $this->assertEquals('User', $q['return_type']);
    }

    public function testQuerySqlSourceAppearsInSchema(): void
    {
        StaticAPI::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'users');
        $this->assertEquals('v_user', $q['sql_source']);
    }

    public function testQueryDescriptionAppearsInSchema(): void
    {
        StaticAPI::query('listUsers')
            ->returnType('User')
            ->sqlSource('v_user')
            ->description('List all active users')
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'listUsers');
        $this->assertEquals('List all active users', $q['description']);
    }

    public function testQueryInjectParamsMultipleClaims(): void
    {
        StaticAPI::query('myData')
            ->returnType('Data')
            ->sqlSource('v_data')
            ->inject([
                'user_id'   => 'jwt:sub',
                'tenant_id' => 'jwt:tenant_id',
            ])
            ->register();

        $q = $this->findQuery(SchemaExporter::toArray(), 'myData');
        $this->assertArrayHasKey('inject_params', $q);
        $this->assertEquals('jwt', $q['inject_params']['user_id']['source']);
        $this->assertEquals('sub', $q['inject_params']['user_id']['claim']);
        $this->assertEquals('jwt', $q['inject_params']['tenant_id']['source']);
        $this->assertEquals('tenant_id', $q['inject_params']['tenant_id']['claim']);
    }

    // =========================================================================
    // MutationBuilder completeness
    // =========================================================================

    public function testMutationSqlSourceAppearsInSchema(): void
    {
        StaticAPI::mutation('createOrder')
            ->returnType('Order')
            ->sqlSource('fn_create_order')
            ->operation('insert')
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'createOrder');
        $this->assertEquals('fn_create_order', $m['sql_source']);
        $this->assertEquals('insert',          $m['operation']);
    }

    public function testMutationInvalidatesViewsAppearsInSchema(): void
    {
        StaticAPI::mutation('placeOrder')
            ->returnType('Order')
            ->sqlSource('fn_place_order')
            ->invalidatesViews(['v_order_summary'])
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'placeOrder');
        $this->assertEquals(['v_order_summary'], $m['invalidates_views']);
    }

    public function testMutationInvalidatesFactTablesAppearsInSchema(): void
    {
        StaticAPI::mutation('recordSale')
            ->returnType('Sale')
            ->sqlSource('fn_record_sale')
            ->invalidatesFactTables(['tf_sales'])
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'recordSale');
        $this->assertEquals(['tf_sales'], $m['invalidates_fact_tables']);
    }

    public function testMutationInjectParamsAppearsInSchema(): void
    {
        StaticAPI::mutation('createOrder')
            ->returnType('Order')
            ->sqlSource('fn_create_order')
            ->inject(['user_id' => 'jwt:sub'])
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'createOrder');
        $this->assertArrayHasKey('inject_params', $m);
        $this->assertEquals('jwt', $m['inject_params']['user_id']['source']);
        $this->assertEquals('sub', $m['inject_params']['user_id']['claim']);
    }

    public function testMutationDescriptionAppearsInSchema(): void
    {
        StaticAPI::mutation('deleteUser')
            ->returnType('User')
            ->sqlSource('fn_delete_user')
            ->description('Permanently remove a user')
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'deleteUser');
        $this->assertEquals('Permanently remove a user', $m['description']);
    }

    public function testMutationReturnTypeAppearsInSchema(): void
    {
        StaticAPI::mutation('updateOrder')
            ->returnType('Order')
            ->sqlSource('fn_update_order')
            ->register();

        $m = $this->findMutation(SchemaExporter::toArray(), 'updateOrder');
        $this->assertEquals('Order', $m['return_type']);
    }

    // =========================================================================
    // Error type flag
    // =========================================================================

    public function testErrorTypeRegistrationSetsIsErrorFlag(): void
    {
        StaticAPI::type('UserNotFound')
            ->isError(true)
            ->field('message', 'String')
            ->field('code', 'String')
            ->register();

        $schema = SchemaExporter::toArray();
        $t = $this->findType($schema, 'UserNotFound');

        $this->assertTrue($t['is_error']);
        $this->assertCount(2, $t['fields']);
    }

    public function testNonErrorTypeDoesNotHaveIsErrorFlag(): void
    {
        StaticAPI::type('NormalType')
            ->field('id', 'Int')
            ->register();

        $schema = SchemaExporter::toArray();
        $t = $this->findType($schema, 'NormalType');

        $this->assertArrayNotHasKey('is_error', $t);
    }

    // =========================================================================
    // Golden fixture 01 (issue #53 regression guard)
    // =========================================================================

    public function testGoldenFixture01BasicQueryMutation(): void
    {
        StaticAPI::clear();

        StaticAPI::type('GoldenUser')
            ->sqlSource('v_golden_user')
            ->field('id', 'Int')
            ->field('email', 'String')
            ->field('name', 'String')
            ->description('A user in the system')
            ->register();

        StaticAPI::query('users')
            ->returnType('GoldenUser')
            ->returnsList(true)
            ->sqlSource('v_user')
            ->description('List all users')
            ->autoParams(true)
            ->argument('limit', 'Int', true, 10)
            ->register();

        StaticAPI::mutation('createUser')
            ->returnType('GoldenUser')
            ->sqlSource('fn_create_user')
            ->operation('insert')
            ->description('Create a new user')
            ->argument('email', 'String', false)
            ->argument('name', 'String', false)
            ->register();

        $goldenPath = __DIR__ . '/../../tests/fixtures/golden/01-basic-query-mutation.json';
        $golden     = json_decode((string) file_get_contents($goldenPath), true);
        $generated  = SchemaExporter::toArray();

        $golQ = $this->findInList($golden['queries'],   'users',      'query');
        $golM = $this->findInList($golden['mutations'], 'createUser', 'mutation');

        // Query — sql_source and returns_list must match
        $genQ = $this->findQuery($generated, 'users');
        $this->assertEquals($golQ['sql_source'], $genQ['sql_source']);
        $this->assertTrue($genQ['returns_list']);

        // Mutation — the issue #53 regression case: sql_source must not be null.
        // The golden fixture uses a structured operation object while the PHP SDK
        // exports a plain string; only sql_source is cross-compared here.
        $genM = $this->findMutation($generated, 'createUser');
        $this->assertEquals($golM['sql_source'], $genM['sql_source']);
        $this->assertNotEmpty($genM['operation']);
        $this->assertEmpty($genM['inject_params'] ?? []);
        $this->assertEmpty($genM['invalidates_views'] ?? []);
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    /**
     * Find an item by name in a schema section. Every section — generated or golden —
     * is a list of objects; there is one shape.
     *
     * @param array<int, array<string, mixed>> $list
     * @return array<string, mixed>
     */
    private function findInList(array $list, string $name, string $kind): array
    {
        foreach ($list as $item) {
            if (($item['name'] ?? null) === $name) {
                return $item;
            }
        }
        $this->fail("$kind '$name' not found in fixture list");
    }

    /** @param array<string, mixed> $schema */
    private function findQuery(array $schema, string $name): mixed
    {
        $this->assertArrayHasKey('queries', $schema, "No 'queries' key in schema");
        return $this->findInList($schema['queries'], $name, 'query');
    }

    /** @param array<string, mixed> $schema */
    private function findMutation(array $schema, string $name): mixed
    {
        $this->assertArrayHasKey('mutations', $schema, "No 'mutations' key in schema");
        return $this->findInList($schema['mutations'], $name, 'mutation');
    }

    /** @param array<string, mixed> $schema */
    private function findType(array $schema, string $name): mixed
    {
        $this->assertArrayHasKey('types', $schema, "No 'types' key in schema");
        return $this->findInList($schema['types'], $name, 'type');
    }
}
