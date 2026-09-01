<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use PHPUnit\Framework\TestCase;

#[GraphQLType(name: 'User', sqlSource: 'v_user')]
final class ConformanceKeysUser
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'String', nullable: true, description: 'The display name')]
    public ?string $name;

    #[GraphQLField(type: 'Float', nullable: true, scope: 'read:User.salary')]
    public ?float $salary;
}

/**
 * A `crud` type and a plain one, for the second-registration guard below. They are
 * attribute-declared because `StaticAPI::register()` — the attribute path — is where the
 * defect lived: it looped over every registered type name and re-expanded each, so the
 * second call re-expanded the first type's `crud` and `registerInputType` threw on the
 * duplicate name (#1248).
 */
#[GraphQLType(name: 'SecondRegTicket', sqlSource: 'v_second_reg_ticket', crud: true)]
final class SecondRegistrationTicket
{
    #[GraphQLField(type: 'Int', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $title;
}

#[GraphQLType(name: 'SecondRegPlain', sqlSource: 'v_second_reg_plain')]
final class SecondRegistrationPlain
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;
}

/**
 * `toIntermediateArray()` is the exporter `SchemaExporter` — and therefore
 * `bin/fraiseql export` — actually calls. It wrote the cache-invalidation list under
 * `invalidates` (the compiler reads `invalidates_views`), never wrote
 * `invalidates_fact_tables` at all, and wrote server-side injection under `inject` (the
 * compiler reads `inject_params`).
 *
 * The sibling `toArray()` on the same class used the correct keys throughout — so the
 * SDK contradicted itself, and the canonical path was the broken one. Every PHP-authored
 * mutation compiled with no invalidation targets and no injected predicate, and the
 * compile printed `✓ Schema compiled successfully` (#852, #806).
 *
 * The runtime consequence is what makes this worse than a hard failure: a PHP author who
 * sets `cacheTtlSeconds(300)` on a query and `invalidatesViews([...])` on the mutation
 * that writes it gets the caching armed and the invalidation silently disarmed, so the
 * newly written row is invisible to every reader for the rest of the TTL.
 *
 * These assertions are on the exported document, not on the builder's internals,
 * because the key names are the entire defect.
 */
final class IntermediateExportKeysTest extends TestCase
{
    protected function setUp(): void
    {
        StaticAPI::clear();
    }

    /** @return array<string, mixed> */
    private function exportedMutation(): array
    {
        StaticAPI::mutation('createOrder')
            ->returnType('Order')
            ->sqlSource('fn_create_order')
            ->operation('insert')
            ->argument('total', 'Float', nullable: false)
            ->inject(['user_id' => 'jwt:sub'])
            ->invalidatesViews(['v_order', 'v_order_summary'])
            ->invalidatesFactTables(['tf_sales'])
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);
        self::assertIsArray($schema);
        self::assertArrayHasKey('mutations', $schema);
        self::assertCount(1, $schema['mutations']);

        return $schema['mutations'][0];
    }

    public function testMutationEmitsInvalidatesViewsUnderTheKeyTheCompilerReads(): void
    {
        $mutation = $this->exportedMutation();

        self::assertSame(['v_order', 'v_order_summary'], $mutation['invalidates_views']);
        self::assertArrayNotHasKey(
            'invalidates',
            $mutation,
            'the compiler denies unknown fields, so a stray `invalidates` now fails the compile',
        );
    }

    public function testMutationEmitsInvalidatesFactTables(): void
    {
        $mutation = $this->exportedMutation();

        self::assertSame(['tf_sales'], $mutation['invalidates_fact_tables']);
    }

    public function testMutationEmitsInjectParamsUnderTheKeyTheCompilerReads(): void
    {
        $mutation = $this->exportedMutation();

        self::assertSame(['user_id' => ['source' => 'jwt', 'claim' => 'sub']], $mutation['inject_params']);
        self::assertArrayNotHasKey('inject', $mutation);
    }

    public function testMutationCarriesReturnsListAndNullable(): void
    {
        $mutation = $this->exportedMutation();

        // Both are required by `IntermediateMutation` and were simply never written, so
        // every PHP mutation compiled as non-null and single-valued regardless of what
        // the author declared.
        self::assertArrayHasKey('returns_list', $mutation);
        self::assertArrayHasKey('nullable', $mutation);
        self::assertFalse($mutation['returns_list']);
        self::assertFalse($mutation['nullable']);
    }

    public function testQueryEmitsInjectParamsUnderTheKeyTheCompilerReads(): void
    {
        StaticAPI::query('tenantOrders')
            ->returnType('Order')
            ->returnsList()
            ->nullable(false)
            ->sqlSource('v_order')
            ->inject(['tenant_id' => 'jwt:tenant_id'])
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);
        $query  = $schema['queries'][0];

        self::assertSame(
            ['tenant_id' => ['source' => 'jwt', 'claim' => 'tenant_id']],
            $query['inject_params'],
        );
        self::assertArrayNotHasKey('inject', $query);
    }

    /**
     * `TypeConverter` parses `#[GraphQLField(scope: ...)]` off the attribute and
     * validates it, and `SchemaExporter::buildTypes` reads `FieldDefinition::$scope` to
     * emit `requires_scope` — but `SchemaRegistry::extractFieldDefinition` never passed
     * the value between them, so the property was always null and the exporter's scope
     * branch was unreachable. A field the author gated compiled ungated (#807), and the
     * fix filed against the exporter could never have taken effect.
     */
    public function testFieldScopeAndDescriptionSurviveExport(): void
    {
        StaticAPI::register(ConformanceKeysUser::class);

        $schema = json_decode(SchemaExporter::export(), true);
        $fields = [];
        foreach ($schema['types'][0]['fields'] as $field) {
            $fields[$field['name']] = $field;
        }

        self::assertSame('read:User.salary', $fields['salary']['requires_scope'] ?? null);
        self::assertSame('The display name', $fields['name']['description'] ?? null);
        self::assertArrayNotHasKey('requires_scope', $fields['name']);
    }

    /**
     * `autoParams()`, `deprecated()` and `relayCursorType()` were three fluent setters whose
     * values never reached `toIntermediateArray()` — the serializer the shipped
     * `vendor/bin/fraiseql export` path actually calls. The sibling `toArray()` did emit
     * them, under `deprecation` and `relay_cursor_type`, which is a second reason they were
     * invisible: the keys looked present in a serializer no consumer used, and neither name
     * is a member of `IntermediateQuery`, which denies unknown fields.
     *
     * Dropping `auto_params` is not the no-op it appears to be. An absent block inherits
     * `[query_defaults]`, which is all-true only by default — so a project that disables
     * `limit` project-wide and opts one query back in with `autoParams(true)` silently got
     * no limit parameter (#1021).
     */
    public function testQueryAutoParamsReachTheCompilerAsPerParameterBooleans(): void
    {
        StaticAPI::query('widgets')
            ->returnType('Widget')
            ->returnsList(true)
            ->sqlSource('v_widget')
            ->autoParams(true)
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);
        $query  = $schema['queries'][0];

        // `IntermediateAutoParams` is an object of booleans, not a bare `true`, and a bare
        // `true` fails to deserialize rather than degrading.
        self::assertSame(
            ['where' => true, 'order_by' => true, 'limit' => true, 'offset' => true],
            $query['auto_params'],
        );
    }

    public function testQueryDeprecationReachesTheCompilerUnderTheKeyItReads(): void
    {
        StaticAPI::query('oldWidgets')
            ->returnType('Widget')
            ->returnsList(true)
            ->sqlSource('v_widget')
            ->deprecated('Use widgets instead')
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);
        $query  = $schema['queries'][0];

        self::assertSame(['reason' => 'Use widgets instead'], $query['deprecated']);
        self::assertArrayNotHasKey('deprecation', $query);
    }

    /**
     * `CrudGenerator` is fully implemented and had **no callers**, so `crud: true`
     * generated nothing through either authoring idiom (#1022). The flags are expanded at
     * registration rather than emitted: `IntermediateType` has no `crud`/`cascade` member
     * and denies unknown fields, so writing them would fail the compile.
     */
    public function testCrudOnTheFluentBuilderGeneratesOperations(): void
    {
        StaticAPI::type('Widget')
            ->sqlSource('v_widget')
            ->field('id', 'ID', nullable: false)
            ->field('label', 'String', nullable: true)
            ->crud(true)
            ->cascade(true)
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);

        $queryNames    = array_column($schema['queries'], 'name');
        $mutationNames = array_column($schema['mutations'], 'name');

        // camelCase, like every hand-authored operation beside them. The generator used
        // to emit the snake_case name verbatim, so one PHP schema carried `createUser`
        // and `create_widget` side by side, and a client generated against the Python
        // schema for the same declaration called a field that did not exist (#1247). A
        // one-word type name cannot show this — `widget` spells the same either way — so
        // the assertions below use a two-word one.
        self::assertContains('widget', $queryNames, 'the get-by-id query must be generated');
        self::assertContains('widgets', $queryNames, 'the list query must be generated');
        self::assertContains('createWidget', $mutationNames);
        self::assertContains('updateWidget', $mutationNames);
        self::assertContains('deleteWidget', $mutationNames);

        // `cascade` belongs on the generated mutations — that is where
        // `IntermediateMutation::cascade` lives — and never on the type.
        foreach ($schema['mutations'] as $mutation) {
            self::assertTrue($mutation['cascade'], "{$mutation['name']} must carry cascade");
        }
        self::assertArrayNotHasKey('crud', $schema['types'][0]);
        self::assertArrayNotHasKey('cascade', $schema['types'][0]);
    }

    /**
     * The naming assertion above, on a type name that can tell the two conventions apart.
     * `Widget` cannot: `widget` is both its snake_case and its camelCase spelling, so the
     * case that used it would have passed under either implementation (#1247).
     */
    public function testGeneratedOperationNamesAreCamelCase(): void
    {
        StaticAPI::type('SupportTicket')
            ->sqlSource('v_support_ticket')
            ->field('id', 'Int', nullable: false)
            ->field('title', 'String', nullable: false)
            ->crud(true)
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);

        self::assertSame(
            ['supportTicket', 'supportTickets'],
            array_column($schema['queries'], 'name'),
        );
        self::assertSame(
            ['createSupportTicket', 'updateSupportTicket', 'deleteSupportTicket'],
            array_column($schema['mutations'], 'name'),
        );
    }

    /**
     * Registering any second type after a `crud` type must not re-expand the first.
     *
     * `StaticAPI::register()` used to loop over every registered type name and expand
     * each one that declared `crud`, so the second call re-ran the first type's
     * expansion. Queries and mutations are keyed by name and would merely overwrite, but
     * `SchemaRegistry::registerInputType` throws on a duplicate name — so the second
     * `register()` call was a hard failure, not a silent duplication (#1248).
     *
     * Nothing had two types where one declared `crud`, which is why #1022's fix shipped
     * with the loop and with the docblock already describing the invariant it broke. The
     * fix landed without a test; this is it. A single-type case cannot see the defect,
     * so the assertion that matters is that a *second* registration happens at all.
     */
    public function testRegisteringASecondTypeAfterACrudTypeDoesNotReExpandTheFirst(): void
    {
        StaticAPI::register(SecondRegistrationTicket::class);
        StaticAPI::register(SecondRegistrationPlain::class);

        $schema = json_decode(SchemaExporter::export(), true);
        self::assertIsArray($schema);

        // Expanded exactly once. Under the loop this line was never reached — the second
        // register() threw — so a duplicate-count assertion is what pins the fix rather
        // than merely observing that the export succeeded.
        self::assertSame(
            ['CreateSecondRegTicketInput', 'UpdateSecondRegTicketInput'],
            array_column($schema['input_types'], 'name'),
        );
        self::assertSame(
            ['createSecondRegTicket', 'updateSecondRegTicket', 'deleteSecondRegTicket'],
            array_column($schema['mutations'], 'name'),
        );
        self::assertSame(
            ['secondRegTicket', 'secondRegTickets'],
            array_column($schema['queries'], 'name'),
        );

        // And the second type is actually in the schema, rather than the test passing
        // because registration quietly did nothing.
        self::assertSame(
            ['SecondRegTicket', 'SecondRegPlain'],
            array_column($schema['types'], 'name'),
        );
    }

    /**
     * The same guard across the two authoring idioms: a `crud` type declared on the
     * fluent builder, then an attribute-declared type. `TypeBuilder::register()` routes
     * to `registerTypeBuilder`, which expands only its own builder, but the attribute
     * path's loop read the *registry*, so a builder-authored crud type registered before
     * it was re-expanded just the same. One idiom's guard does not cover the other.
     */
    public function testRegisteringAnAttributeTypeAfterAFluentCrudTypeDoesNotReExpandIt(): void
    {
        StaticAPI::type('SecondRegTicket')
            ->sqlSource('v_second_reg_ticket')
            ->field('id', 'Int', nullable: false)
            ->field('title', 'String', nullable: false)
            ->crud(true)
            ->register();

        StaticAPI::register(SecondRegistrationPlain::class);

        $schema = json_decode(SchemaExporter::export(), true);
        self::assertIsArray($schema);

        self::assertSame(
            ['CreateSecondRegTicketInput', 'UpdateSecondRegTicketInput'],
            array_column($schema['input_types'], 'name'),
        );
        self::assertSame(
            ['SecondRegTicket', 'SecondRegPlain'],
            array_column($schema['types'], 'name'),
        );
    }

    public function testMutationRequiresRoleSurvivesExport(): void
    {
        StaticAPI::mutation('deleteOrder')
            ->returnType('Order')
            ->sqlSource('fn_delete_order')
            ->operation('delete')
            ->requiresRole('admin')
            ->register();

        $schema = json_decode(SchemaExporter::export(), true);

        self::assertSame('admin', $schema['mutations'][0]['requires_role']);
    }
}
