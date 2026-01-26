<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\Security\AuthzPolicyBuilder;
use FraiseQL\Security\AuthzPolicyType;
use FraiseQL\Attributes\AuthzPolicy;

/**
 * Tests for attribute-based authorization policies.
 */
final class AttributeBasedAccessControlTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testAbacPolicyDefinition(): void
    {
        $config = AuthzPolicyBuilder::create('secretClearance')
            ->type(AuthzPolicyType::ABAC)
            ->description('Requires top secret clearance')
            ->attributesArray(['clearance_level >= 3', 'background_check == true'])
            ->build();

        $this->assertSame('secretClearance', $config->name);
        $this->assertSame(AuthzPolicyType::ABAC, $config->type);
        $this->assertCount(2, $config->attributes);
    }

    public function testAbacAttributesVariadic(): void
    {
        $config = AuthzPolicyBuilder::create('financialData')
            ->attributes(
                'clearance_level >= 2',
                'department == "finance"',
                'mfa_enabled == true'
            )
            ->build();

        $this->assertCount(3, $config->attributes);
        $this->assertContains('clearance_level >= 2', $config->attributes);
    }

    public function testAbacAttributesArray(): void
    {
        $config = AuthzPolicyBuilder::create('regionalData')
            ->attributesArray(['region == "US"', 'gdpr_compliant == true'])
            ->build();

        $this->assertCount(2, $config->attributes);
    }

    public function testAbacClearanceLevelPattern(): void
    {
        $config = AuthzPolicyBuilder::create('classifiedDocument')
            ->type(AuthzPolicyType::ABAC)
            ->description('Access based on clearance level')
            ->attributes('clearance_level >= 2')
            ->build();

        $this->assertSame(AuthzPolicyType::ABAC, $config->type);
        $this->assertCount(1, $config->attributes);
    }

    public function testAbacDepartmentPattern(): void
    {
        $config = AuthzPolicyBuilder::create('departmentData')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('department == "HR"')
            ->description('HR department access only')
            ->build();

        $this->assertSame('departmentData', $config->name);
    }

    public function testAbacTimeBasedPattern(): void
    {
        $config = AuthzPolicyBuilder::create('timeRestrictedData')
            ->type(AuthzPolicyType::ABAC)
            ->attributes(
                'current_time > "09:00"',
                'current_time < "17:00"',
                'day_of_week != "Sunday"'
            )
            ->description('Business hours access')
            ->build();

        $this->assertCount(3, $config->attributes);
    }

    public function testAbacGeographicPattern(): void
    {
        $config = AuthzPolicyBuilder::create('geographicRestriction')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('region in ["US", "CA", "MX"]')
            ->description('North American access only')
            ->build();

        $this->assertCount(1, $config->attributes);
    }

    public function testAbacGdprCompliance(): void
    {
        $config = AuthzPolicyBuilder::create('personalData')
            ->type(AuthzPolicyType::ABAC)
            ->attributes(
                'gdpr_compliant == true',
                'data_residency == "EU"',
                'consent_given == true'
            )
            ->description('GDPR-compliant access')
            ->build();

        $this->assertCount(3, $config->attributes);
    }

    public function testAbacProjectBasedPattern(): void
    {
        $config = AuthzPolicyBuilder::create('projectData')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('user_project == resource_project')
            ->description('Users can only access their own projects')
            ->build();

        $this->assertCount(1, $config->attributes);
    }

    public function testAbacClassificationPattern(): void
    {
        $config = AuthzPolicyBuilder::create('dataClassification')
            ->type(AuthzPolicyType::ABAC)
            ->attributes(
                'user_classification >= resource_classification',
                'has_need_to_know == true'
            )
            ->description('Classification-based access control')
            ->build();

        $this->assertCount(2, $config->attributes);
    }

    public function testAbacCaching(): void
    {
        $config = AuthzPolicyBuilder::create('cachedAbac')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('attribute1 == "value"')
            ->cacheable(true)
            ->cacheDurationSeconds(3600)
            ->build();

        $this->assertTrue($config->cacheable);
        $this->assertSame(3600, $config->cacheDurationSeconds);
    }

    public function testAbacNoCache(): void
    {
        $config = AuthzPolicyBuilder::create('sensitivAbac')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('sensitive_attribute == true')
            ->cacheable(false)
            ->build();

        $this->assertFalse($config->cacheable);
    }

    public function testAbacAuditLogging(): void
    {
        $config = AuthzPolicyBuilder::create('auditedAbac')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('access_control == true')
            ->auditLogging(true)
            ->build();

        $this->assertTrue($config->auditLogging);
    }

    public function testAbacErrorMessage(): void
    {
        $config = AuthzPolicyBuilder::create('restrictedAbac')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('clearance_level >= 3')
            ->errorMessage('Your clearance level is insufficient for this resource')
            ->build();

        $this->assertSame('Your clearance level is insufficient for this resource', $config->errorMessage);
    }

    public function testAbacOperationSpecific(): void
    {
        $config = AuthzPolicyBuilder::create('deleteRestricted')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('role == "admin"')
            ->operations('delete,create')
            ->build();

        $this->assertSame('delete,create', $config->operations);
    }

    public function testAbacRecursive(): void
    {
        $config = AuthzPolicyBuilder::create('recursiveAbac')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('hierarchy_level >= 2')
            ->recursive(true)
            ->build();

        $this->assertTrue($config->recursive);
    }

    public function testAbacFluentChaining(): void
    {
        $config = AuthzPolicyBuilder::create('complexAbac')
            ->type(AuthzPolicyType::ABAC)
            ->description('Complex ABAC policy')
            ->attributes('clearance >= 2', 'department == "IT"', 'mfa == true')
            ->cacheable(true)
            ->cacheDurationSeconds(1800)
            ->recursive(false)
            ->operations('read,update')
            ->auditLogging(true)
            ->errorMessage('Access denied')
            ->build();

        $this->assertSame('complexAbac', $config->name);
        $this->assertSame(AuthzPolicyType::ABAC, $config->type);
        $this->assertCount(3, $config->attributes);
        $this->assertTrue($config->cacheable);
        $this->assertTrue($config->auditLogging);
    }

    public function testAbacAttributeWithRule(): void
    {
        $config = AuthzPolicyBuilder::create('hybridAbac')
            ->type(AuthzPolicyType::ABAC)
            ->rule("hasAttribute(\$context, 'clearance_level', 3)")
            ->attributes('clearance_level >= 3')
            ->build();

        $this->assertSame("hasAttribute(\$context, 'clearance_level', 3)", $config->rule);
    }

    public function testAbacAttributePattern(): void
    {
        #[AuthzPolicy(
            name: 'abacExample',
            type: AuthzPolicyType::ABAC,
            attributes: ['clearance >= 2', 'department == "Finance"']
        )]
        class AbacExample
        {
        }

        $this->assertTrue(true);
    }
}
