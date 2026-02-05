<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaRegistry;
use FraiseQL\Schema;
use PHPUnit\Framework\TestCase;

/**
 * Phase 18 Cycle 11: Field-Level RBAC for PHP SDK
 *
 * Tests that field scopes are properly extracted from GraphQLField attributes,
 * stored in registry, and exported to JSON for compiler consumption.
 *
 * RED Phase: 21 comprehensive test cases
 * - 15 happy path tests for scope extraction and export
 * - 6 validation tests for error handling
 *
 * Attribute format:
 * - Single scope: #[GraphQLField(scope: 'read:user.email')]
 * - Multiple scopes: #[GraphQLField(scopes: ['admin', 'auditor'])]
 */
final class Phase18Cycle11ScopeExtractionTest extends TestCase
{
    private SchemaRegistry $registry;

    protected function setUp(): void
    {
        $this->registry = SchemaRegistry::getInstance();
        $this->registry->clear();
    }

    protected function tearDown(): void
    {
        $this->registry->clear();
    }

    // =========================================================================
    // HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
    // =========================================================================

    public function testSingleScopeExtraction(): void
    {
        // RED: This test fails because FieldDefinition doesn't store scope
        #[GraphQLType(name: 'UserWithScope')]
        final class UserWithScope
        {
            #[GraphQLField(type: 'Int')]
            public int $id;

            #[GraphQLField(type: 'Float', scope: 'read:user.salary')]
            public float $salary;
        }

        $this->registry->register(UserWithScope::class);

        $typeFields = $this->registry->getTypeFields('UserWithScope');
        $this->assertNotNull($typeFields);
        $this->assertArrayHasKey('salary', $typeFields);

        $salaryField = $typeFields['salary'];
        $this->assertEquals('read:user.salary', $salaryField->getScope(),
            'Salary field should have single scope extracted');
    }

    public function testMultipleDifferentScopesExtraction(): void
    {
        // RED: Tests extraction of different scopes on different fields
        #[GraphQLType(name: 'UserWithMultipleScopes')]
        final class UserWithMultipleScopes
        {
            #[GraphQLField(type: 'Int')]
            public int $id;

            #[GraphQLField(type: 'String', scope: 'read:user.email')]
            public string $email;

            #[GraphQLField(type: 'String', scope: 'read:user.phone')]
            public string $phone;

            #[GraphQLField(type: 'String', scope: 'read:user.ssn')]
            public string $ssn;
        }

        $this->registry->register(UserWithMultipleScopes::class);

        $typeFields = $this->registry->getTypeFields('UserWithMultipleScopes');
        $this->assertEquals('read:user.email', $typeFields['email']->getScope());
        $this->assertEquals('read:user.phone', $typeFields['phone']->getScope());
        $this->assertEquals('read:user.ssn', $typeFields['ssn']->getScope());
    }

    public function testPublicFieldNoScopeExtraction(): void
    {
        // RED: Public fields should have null/empty scope
        #[GraphQLType(name: 'UserWithMixedFields')]
        final class UserWithMixedFields
        {
            #[GraphQLField(type: 'Int')]
            public int $id;

            #[GraphQLField(type: 'String')]
            public string $name;

            #[GraphQLField(type: 'String', scope: 'read:user.email')]
            public string $email;
        }

        $this->registry->register(UserWithMixedFields::class);

        $typeFields = $this->registry->getTypeFields('UserWithMixedFields');
        $this->assertNull($typeFields['id']->getScope(),
            'Public id field should have no scope requirement');
    }

    // =========================================================================
    // HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
    // =========================================================================

    public function testMultipleScopesOnSingleField(): void
    {
        // RED: Field with scopes=['scope1', 'scope2'] array
        #[GraphQLType(name: 'AdminWithMultipleScopes')]
        final class AdminWithMultipleScopes
        {
            #[GraphQLField(type: 'Int')]
            public int $id;

            #[GraphQLField(type: 'String', scopes: ['admin', 'auditor'])]
            public string $adminNotes;
        }

        $this->registry->register(AdminWithMultipleScopes::class);

        $typeFields = $this->registry->getTypeFields('AdminWithMultipleScopes');
        $adminNotesField = $typeFields['adminNotes'];

        $scopes = $adminNotesField->getScopes();
        $this->assertNotNull($scopes, 'Field should have multiple scopes array');
        $this->assertCount(2, $scopes, 'adminNotes should require 2 scopes');
        $this->assertContains('admin', $scopes);
        $this->assertContains('auditor', $scopes);
    }

    public function testMixedSingleAndMultipleScopes(): void
    {
        // RED: Type with both single-scope and multi-scope fields
        #[GraphQLType(name: 'MixedScopeTypes')]
        final class MixedScopeTypes
        {
            #[GraphQLField(type: 'String', scope: 'read:basic')]
            public string $basicField;

            #[GraphQLField(type: 'String', scopes: ['read:advanced', 'admin'])]
            public string $advancedField;
        }

        $this->registry->register(MixedScopeTypes::class);

        $typeFields = $this->registry->getTypeFields('MixedScopeTypes');

        $this->assertEquals('read:basic', $typeFields['basicField']->getScope());
        $this->assertCount(2, $typeFields['advancedField']->getScopes());
    }

    public function testScopeArrayOrder(): void
    {
        // RED: Scopes array order must be preserved
        #[GraphQLType(name: 'OrderedScopes')]
        final class OrderedScopes
        {
            #[GraphQLField(type: 'String', scopes: ['first', 'second', 'third'])]
            public string $restricted;
        }

        $this->registry->register(OrderedScopes::class);

        $typeFields = $this->registry->getTypeFields('OrderedScopes');
        $scopes = $typeFields['restricted']->getScopes();

        $this->assertCount(3, $scopes);
        $this->assertEquals('first', $scopes[0]);
        $this->assertEquals('second', $scopes[1]);
        $this->assertEquals('third', $scopes[2]);
    }

    // =========================================================================
    // HAPPY PATH: SCOPE PATTERNS (3 tests)
    // =========================================================================

    public function testResourceBasedScopePattern(): void
    {
        // RED: Resource pattern like read:User.email
        #[GraphQLType(name: 'ResourcePatternScopes')]
        final class ResourcePatternScopes
        {
            #[GraphQLField(type: 'String', scope: 'read:User.email')]
            public string $email;

            #[GraphQLField(type: 'String', scope: 'read:User.phone')]
            public string $phone;
        }

        $this->registry->register(ResourcePatternScopes::class);

        $typeFields = $this->registry->getTypeFields('ResourcePatternScopes');
        $this->assertEquals('read:User.email', $typeFields['email']->getScope());
    }

    public function testActionBasedScopePattern(): void
    {
        // RED: Action patterns like read:*, write:*, admin:*
        #[GraphQLType(name: 'ActionPatternScopes')]
        final class ActionPatternScopes
        {
            #[GraphQLField(type: 'String', scope: 'read:User.*')]
            public string $readableField;

            #[GraphQLField(type: 'String', scope: 'write:User.*')]
            public string $writableField;
        }

        $this->registry->register(ActionPatternScopes::class);

        $typeFields = $this->registry->getTypeFields('ActionPatternScopes');
        $this->assertEquals('read:User.*', $typeFields['readableField']->getScope());
        $this->assertEquals('write:User.*', $typeFields['writableField']->getScope());
    }

    public function testGlobalWildcardScope(): void
    {
        // RED: Global wildcard matching all scopes
        #[GraphQLType(name: 'GlobalWildcardScope')]
        final class GlobalWildcardScope
        {
            #[GraphQLField(type: 'String', scope: '*')]
            public string $adminOverride;
        }

        $this->registry->register(GlobalWildcardScope::class);

        $typeFields = $this->registry->getTypeFields('GlobalWildcardScope');
        $this->assertEquals('*', $typeFields['adminOverride']->getScope(),
            'Admin override should use global wildcard');
    }

    // =========================================================================
    // HAPPY PATH: JSON EXPORT (3 tests)
    // =========================================================================

    public function testScopeExportToJsonSingleScope(): void
    {
        // RED: Scope must appear in JSON export
        #[GraphQLType(name: 'ExportTestSingleScope')]
        final class ExportTestSingleScope
        {
            #[GraphQLField(type: 'Float', scope: 'read:user.salary')]
            public float $salary;
        }

        $this->registry->register(ExportTestSingleScope::class);

        $json = Schema::exportTypes();
        $schema = \json_decode($json, true, 512, JSON_THROW_ON_ERROR);

        $this->assertIsArray($schema['types']);
        $this->assertCount(1, $schema['types']);

        $type = $schema['types'][0];
        $salaryField = $type['fields'][0];

        $this->assertArrayHasKey('scope', $salaryField,
            'JSON should contain scope field');
        $this->assertEquals('read:user.salary', $salaryField['scope']);
    }

    public function testScopeExportToJsonMultipleScopes(): void
    {
        // RED: scopes array exported as scopes field in JSON
        #[GraphQLType(name: 'ExportTestMultipleScopes')]
        final class ExportTestMultipleScopes
        {
            #[GraphQLField(type: 'String', scopes: ['scope1', 'scope2'])]
            public string $restricted;
        }

        $this->registry->register(ExportTestMultipleScopes::class);

        $json = Schema::exportTypes();
        $schema = \json_decode($json, true, 512, JSON_THROW_ON_ERROR);

        $type = $schema['types'][0];
        $field = $type['fields'][0];

        $this->assertArrayHasKey('scopes', $field,
            'JSON should contain scopes array');
        $this->assertCount(2, $field['scopes']);
    }

    public function testPublicFieldJsonExport(): void
    {
        // RED: Public fields without scope should not have scope in JSON
        #[GraphQLType(name: 'ExportTestPublicField')]
        final class ExportTestPublicField
        {
            #[GraphQLField(type: 'Int')]
            public int $id;

            #[GraphQLField(type: 'String')]
            public string $name;
        }

        $this->registry->register(ExportTestPublicField::class);

        $json = Schema::exportTypes();
        $schema = \json_decode($json, true, 512, JSON_THROW_ON_ERROR);

        $type = $schema['types'][0];
        $idField = $type['fields'][0];

        $this->assertArrayNotHasKey('scope', $idField,
            'Public field should not have scope in JSON');
        $this->assertArrayNotHasKey('scopes', $idField,
            'Public field should not have scopes in JSON');
    }

    // =========================================================================
    // HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
    // =========================================================================

    public function testScopePreservedWithMetadata(): void
    {
        // RED: Scope doesn't interfere with type, nullable, etc.
        #[GraphQLType(name: 'ScopeWithMetadata')]
        final class ScopeWithMetadata
        {
            #[GraphQLField(type: 'Float', scope: 'read:user.salary')]
            public float $salary;
        }

        $this->registry->register(ScopeWithMetadata::class);

        $typeFields = $this->registry->getTypeFields('ScopeWithMetadata');
        $salaryField = $typeFields['salary'];

        $this->assertEquals('Float', $salaryField->getType());
        $this->assertEquals('read:user.salary', $salaryField->getScope());
    }

    public function testScopeWithNullableField(): void
    {
        // RED: Scope works on nullable fields
        #[GraphQLType(name: 'ScopeWithNullable')]
        final class ScopeWithNullable
        {
            #[GraphQLField(type: 'String', nullable: true, scope: 'read:user.email')]
            public ?string $optionalEmail;
        }

        $this->registry->register(ScopeWithNullable::class);

        $typeFields = $this->registry->getTypeFields('ScopeWithNullable');
        $emailField = $typeFields['optionalEmail'];

        $this->assertTrue($emailField->isNullable());
        $this->assertEquals('read:user.email', $emailField->getScope());
    }

    public function testMultipleScopedFieldsMetadataIndependence(): void
    {
        // RED: Each field's metadata is independent
        #[GraphQLType(name: 'MetadataIndependence')]
        final class MetadataIndependence
        {
            #[GraphQLField(type: 'String', scope: 'scope1')]
            public string $field1;

            #[GraphQLField(type: 'String', scope: 'scope2')]
            public string $field2;
        }

        $this->registry->register(MetadataIndependence::class);

        $typeFields = $this->registry->getTypeFields('MetadataIndependence');

        $this->assertEquals('scope1', $typeFields['field1']->getScope());
        $this->assertEquals('scope2', $typeFields['field2']->getScope());
    }

    // =========================================================================
    // VALIDATION: ERROR HANDLING (6 tests)
    // =========================================================================

    public function testInvalidScopeFormatDetection(): void
    {
        // RED: Invalid scopes should be detected
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'InvalidScopeFormat')]
        final class InvalidScopeFormat
        {
            #[GraphQLField(type: 'String', scope: 'invalid_scope_no_colon')]
            public string $field;
        }

        $this->registry->register(InvalidScopeFormat::class);
    }

    public function testEmptyScopeRejection(): void
    {
        // RED: Empty string scope should be invalid
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'EmptyScope')]
        final class EmptyScope
        {
            #[GraphQLField(type: 'String', scope: '')]
            public string $field;
        }

        $this->registry->register(EmptyScope::class);
    }

    public function testEmptyScopesArrayRejection(): void
    {
        // RED: Empty scopes array should be invalid
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'EmptyScopesArray')]
        final class EmptyScopesArray
        {
            #[GraphQLField(type: 'String', scopes: [])]
            public string $field;
        }

        $this->registry->register(EmptyScopesArray::class);
    }

    public function testInvalidActionWithHyphensValidation(): void
    {
        // RED: Hyphens in action prefix are invalid
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'InvalidActionWithHyphens')]
        final class InvalidActionWithHyphens
        {
            #[GraphQLField(type: 'String', scope: 'invalid-action:resource')]
            public string $field;
        }

        $this->registry->register(InvalidActionWithHyphens::class);
    }

    public function testInvalidResourceWithHyphensValidation(): void
    {
        // RED: Hyphens in resource name are invalid
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'InvalidResourceWithHyphens')]
        final class InvalidResourceWithHyphens
        {
            #[GraphQLField(type: 'String', scope: 'read:invalid-resource-name')]
            public string $field;
        }

        $this->registry->register(InvalidResourceWithHyphens::class);
    }

    public function testConflictingBothScopeAndScopes(): void
    {
        // RED: Can't have both scope and scopes on same field
        $this->expectException(\Exception::class);

        #[GraphQLType(name: 'ConflictingScopeAndScopes')]
        final class ConflictingScopeAndScopes
        {
            #[GraphQLField(
                type: 'String',
                scope: 'read:user.email',
                scopes: ['admin', 'auditor']
            )]
            public string $field;
        }

        $this->registry->register(ConflictingScopeAndScopes::class);
    }
}
