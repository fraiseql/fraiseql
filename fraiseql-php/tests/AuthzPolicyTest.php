<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\Security\AuthzPolicyBuilder;
use FraiseQL\Security\AuthzPolicyType;
use FraiseQL\Attributes\AuthzPolicy;

/**
 * Tests for authorization policy definitions and reuse.
 */
final class AuthzPolicyTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testRbacPolicyDefinition(): void
    {
        $config = AuthzPolicyBuilder::create('adminOnly')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'admin')")
            ->description('Access restricted to administrators')
            ->auditLogging(true)
            ->build();

        $this->assertSame('adminOnly', $config->name);
        $this->assertSame(AuthzPolicyType::RBAC, $config->type);
        $this->assertSame("hasRole(\$context, 'admin')", $config->rule);
        $this->assertTrue($config->auditLogging);
    }

    public function testAbacPolicyDefinition(): void
    {
        $config = AuthzPolicyBuilder::create('secretClearance')
            ->type(AuthzPolicyType::ABAC)
            ->description('Requires top secret clearance')
            ->attributes('clearance_level >= 3', 'background_check == true')
            ->build();

        $this->assertSame('secretClearance', $config->name);
        $this->assertSame(AuthzPolicyType::ABAC, $config->type);
        $this->assertCount(2, $config->attributes);
    }

    public function testCustomPolicyDefinition(): void
    {
        $config = AuthzPolicyBuilder::create('customRule')
            ->type(AuthzPolicyType::CUSTOM)
            ->rule("isOwner(\$context.userId, \$resource.ownerId)")
            ->description('Custom ownership rule')
            ->build();

        $this->assertSame(AuthzPolicyType::CUSTOM, $config->type);
    }

    public function testHybridPolicyDefinition(): void
    {
        $config = AuthzPolicyBuilder::create('auditAccess')
            ->type(AuthzPolicyType::HYBRID)
            ->description('Role and attribute-based access')
            ->rule("hasRole(\$context, 'auditor')")
            ->attributes('audit_enabled == true')
            ->build();

        $this->assertSame(AuthzPolicyType::HYBRID, $config->type);
        $this->assertSame("hasRole(\$context, 'auditor')", $config->rule);
    }

    public function testPolicyRegistration(): void
    {
        $registry = SchemaRegistry::getInstance();

        $config = AuthzPolicyBuilder::create('testPolicy')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'test')")
            ->build();

        $registry->registerAuthzPolicy($config);

        $this->assertTrue($registry->hasAuthzPolicy('testPolicy'));
        $retrieved = $registry->getAuthzPolicy('testPolicy');
        $this->assertNotNull($retrieved);
        $this->assertSame('testPolicy', $retrieved->name);
    }

    public function testMultiplePoliciesRegistration(): void
    {
        $registry = SchemaRegistry::getInstance();

        $policy1 = AuthzPolicyBuilder::create('policy1')
            ->type(AuthzPolicyType::RBAC)
            ->build();

        $policy2 = AuthzPolicyBuilder::create('policy2')
            ->type(AuthzPolicyType::ABAC)
            ->build();

        $policy3 = AuthzPolicyBuilder::create('policy3')
            ->type(AuthzPolicyType::CUSTOM)
            ->build();

        $registry->registerAuthzPolicy($policy1);
        $registry->registerAuthzPolicy($policy2);
        $registry->registerAuthzPolicy($policy3);

        $all = $registry->getAllAuthzPolicies();
        $this->assertCount(3, $all);
    }

    public function testPiiAccessPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('piiAccess')
            ->type(AuthzPolicyType::RBAC)
            ->description('Access to Personally Identifiable Information')
            ->rule("hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')")
            ->build();

        $this->assertSame('piiAccess', $config->name);
    }

    public function testAdminOnlyPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('adminOnly')
            ->type(AuthzPolicyType::RBAC)
            ->description('Admin-only access')
            ->rule("hasRole(\$context, 'admin')")
            ->auditLogging(true)
            ->build();

        $this->assertTrue($config->auditLogging);
    }

    public function testRecursivePolicyApplication(): void
    {
        $config = AuthzPolicyBuilder::create('recursiveProtection')
            ->type(AuthzPolicyType::CUSTOM)
            ->rule("canAccessNested(\$context)")
            ->recursive(true)
            ->description('Recursively applies to nested types')
            ->build();

        $this->assertTrue($config->recursive);
    }

    public function testOperationSpecificPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('readOnly')
            ->type(AuthzPolicyType::CUSTOM)
            ->rule("hasRole(\$context, 'viewer')")
            ->operations('read')
            ->description('Policy applies only to read operations')
            ->build();

        $this->assertSame('read', $config->operations);
    }

    public function testCachedPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('cachedAccess')
            ->type(AuthzPolicyType::CUSTOM)
            ->rule("hasRole(\$context, 'viewer')")
            ->cacheable(true)
            ->cacheDurationSeconds(3600)
            ->description('Access control with result caching')
            ->build();

        $this->assertTrue($config->cacheable);
        $this->assertSame(3600, $config->cacheDurationSeconds);
    }

    public function testAuditedPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('auditedAccess')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'auditor')")
            ->auditLogging(true)
            ->description('Access with comprehensive audit logging')
            ->build();

        $this->assertTrue($config->auditLogging);
    }

    public function testPolicyWithErrorMessage(): void
    {
        $config = AuthzPolicyBuilder::create('restrictedAccess')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'executive')")
            ->errorMessage('Only executive level users can access this resource')
            ->build();

        $this->assertSame('Only executive level users can access this resource', $config->errorMessage);
    }

    public function testPolicyFluentChaining(): void
    {
        $config = AuthzPolicyBuilder::create('complexPolicy')
            ->type(AuthzPolicyType::HYBRID)
            ->description('Complex hybrid policy')
            ->rule("hasRole(\$context, 'admin')")
            ->attributes('security_clearance >= 3')
            ->cacheable(true)
            ->cacheDurationSeconds(1800)
            ->recursive(false)
            ->operations('create,update,delete')
            ->auditLogging(true)
            ->errorMessage('Insufficient privileges')
            ->build();

        $this->assertSame('complexPolicy', $config->name);
        $this->assertSame(AuthzPolicyType::HYBRID, $config->type);
        $this->assertTrue($config->cacheable);
        $this->assertTrue($config->auditLogging);
    }

    public function testPolicyComposition(): void
    {
        $publicPolicy = AuthzPolicyBuilder::create('publicAccess')
            ->type(AuthzPolicyType::RBAC)
            ->rule('true')  // Everyone has access
            ->build();

        $piiPolicy = AuthzPolicyBuilder::create('piiAccess')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'data_manager')")
            ->build();

        $adminPolicy = AuthzPolicyBuilder::create('adminAccess')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'admin')")
            ->build();

        $this->assertSame('publicAccess', $publicPolicy->name);
        $this->assertSame('piiAccess', $piiPolicy->name);
        $this->assertSame('adminAccess', $adminPolicy->name);
    }

    public function testFinancialDataPolicy(): void
    {
        $config = AuthzPolicyBuilder::create('financialData')
            ->type(AuthzPolicyType::ABAC)
            ->description('Access to financial records')
            ->attributes('clearance_level >= 2', 'department == "finance"')
            ->build();

        $this->assertSame('financialData', $config->name);
        $this->assertCount(2, $config->attributes);
    }

    public function testSecurityClearancePolicy(): void
    {
        $config = AuthzPolicyBuilder::create('secretClearance')
            ->type(AuthzPolicyType::ABAC)
            ->attributes('clearance_level >= 3', 'background_check == true')
            ->description('Requires top secret clearance')
            ->build();

        $this->assertCount(2, $config->attributes);
    }

    public function testPolicyAttributeBasic(): void
    {
        #[AuthzPolicy(
            name: 'adminOnly',
            rule: "hasRole(\$context, 'admin')",
        )]
        class AdminPolicy
        {
        }

        $this->assertTrue(true);
    }

    public function testPolicyAttributeWithAllParameters(): void
    {
        #[AuthzPolicy(
            name: 'complexPolicy',
            type: AuthzPolicyType::HYBRID,
            description: 'Complex policy',
            rule: "hasRole(\$context, 'admin')",
            attributes: ['clearance >= 3'],
            errorMessage: 'Access denied',
            recursive: true,
            operations: 'delete,create',
            auditLogging: true,
            cacheable: true,
            cacheDurationSeconds: 1800,
        )]
        class ComplexPolicy
        {
        }

        $this->assertTrue(true);
    }

    public function testToArraySerialization(): void
    {
        $config = AuthzPolicyBuilder::create('testPolicy')
            ->type(AuthzPolicyType::RBAC)
            ->rule("hasRole(\$context, 'admin')")
            ->description('Test policy')
            ->build();

        $array = $config->toArray();
        $this->assertArrayHasKey('name', $array);
        $this->assertArrayHasKey('type', $array);
        $this->assertArrayHasKey('rule', $array);
        $this->assertSame('testPolicy', $array['name']);
    }
}
