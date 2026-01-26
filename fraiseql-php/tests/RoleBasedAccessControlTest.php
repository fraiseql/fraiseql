<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\Security\RoleRequiredBuilder;
use FraiseQL\Security\RoleMatchStrategy;
use FraiseQL\Attributes\RoleRequired;

/**
 * Tests for role-based access control.
 */
final class RoleBasedAccessControlTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testRoleRequiredSingleRole(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin')
            ->description('Admin role required')
            ->build();

        $this->assertCount(1, $config->roles);
        $this->assertContains('admin', $config->roles);
    }

    public function testRoleRequiredMultipleRoles(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('manager', 'director')
            ->description('Manager or director required')
            ->build();

        $this->assertCount(2, $config->roles);
        $this->assertContains('manager', $config->roles);
        $this->assertContains('director', $config->roles);
    }

    public function testRoleRequiredRolesArray(): void
    {
        $config = RoleRequiredBuilder::create()
            ->rolesArray(['viewer', 'editor', 'admin'])
            ->description('Multiple roles via array')
            ->build();

        $this->assertCount(3, $config->roles);
    }

    public function testRoleMatchStrategyAny(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('manager', 'director')
            ->strategy(RoleMatchStrategy::ANY)
            ->description('User needs at least one role')
            ->build();

        $this->assertSame(RoleMatchStrategy::ANY, $config->strategy);
    }

    public function testRoleMatchStrategyAll(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin', 'auditor')
            ->strategy(RoleMatchStrategy::ALL)
            ->description('User needs all roles')
            ->build();

        $this->assertSame(RoleMatchStrategy::ALL, $config->strategy);
    }

    public function testRoleMatchStrategyExactly(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin')
            ->strategy(RoleMatchStrategy::EXACTLY)
            ->description('User must have exactly these roles')
            ->build();

        $this->assertSame(RoleMatchStrategy::EXACTLY, $config->strategy);
    }

    public function testRoleHierarchy(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('user')
            ->hierarchy(true)
            ->description('Role hierarchy enabled')
            ->build();

        $this->assertTrue($config->hierarchy);
    }

    public function testRoleInheritance(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('editor')
            ->inherit(true)
            ->description('Inherit role requirements')
            ->build();

        $this->assertTrue($config->inherit);
    }

    public function testRoleOperationSpecific(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin')
            ->operations('delete,create')
            ->description('Admin for destructive operations')
            ->build();

        $this->assertSame('delete,create', $config->operations);
    }

    public function testRoleCaching(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('viewer')
            ->cacheable(true)
            ->cacheDurationSeconds(1800)
            ->build();

        $this->assertTrue($config->cacheable);
        $this->assertSame(1800, $config->cacheDurationSeconds);
    }

    public function testRoleErrorMessage(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin')
            ->errorMessage('You must be an administrator to access this resource')
            ->build();

        $this->assertSame('You must be an administrator to access this resource', $config->errorMessage);
    }

    public function testRoleFluentChaining(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('manager', 'director')
            ->strategy(RoleMatchStrategy::ANY)
            ->hierarchy(true)
            ->description('Manager or director with hierarchy')
            ->errorMessage('Insufficient role')
            ->operations('read,update')
            ->inherit(false)
            ->cacheable(true)
            ->cacheDurationSeconds(900)
            ->build();

        $this->assertCount(2, $config->roles);
        $this->assertSame(RoleMatchStrategy::ANY, $config->strategy);
        $this->assertTrue($config->hierarchy);
        $this->assertFalse($config->inherit);
        $this->assertSame(900, $config->cacheDurationSeconds);
    }

    public function testRoleAdminPattern(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('admin')
            ->strategy(RoleMatchStrategy::EXACTLY)
            ->hierarchy(true)
            ->description('Full admin access with hierarchy')
            ->build();

        $this->assertCount(1, $config->roles);
        $this->assertTrue($config->hierarchy);
    }

    public function testRoleManagerPattern(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('manager', 'director', 'executive')
            ->strategy(RoleMatchStrategy::ANY)
            ->description('Management tier access')
            ->operations('read,create,update')
            ->build();

        $this->assertCount(3, $config->roles);
        $this->assertSame('read,create,update', $config->operations);
    }

    public function testRoleDataScientistPattern(): void
    {
        $config = RoleRequiredBuilder::create()
            ->roles('data_scientist', 'analyst')
            ->strategy(RoleMatchStrategy::ANY)
            ->description('Data access for scientists and analysts')
            ->operations('read')
            ->build();

        $this->assertCount(2, $config->roles);
    }

    public function testRoleAttributeBasic(): void
    {
        #[RoleRequired(roles: ['admin'])]
        class AdminPanel
        {
        }

        $this->assertTrue(true);
    }

    public function testRoleAttributeWithStrategy(): void
    {
        #[RoleRequired(
            roles: ['manager', 'director'],
            strategy: RoleMatchStrategy::ANY,
            description: 'Management access',
        )]
        class SalaryData
        {
        }

        $this->assertTrue(true);
    }

    public function testRoleAttributeAllParameters(): void
    {
        #[RoleRequired(
            roles: ['admin', 'auditor'],
            strategy: RoleMatchStrategy::ALL,
            hierarchy: true,
            description: 'Full admin with auditor',
            errorMessage: 'Insufficient privileges',
            operations: 'delete,create',
            inherit: false,
            cacheable: true,
            cacheDurationSeconds: 1200,
        )]
        class ComplexRoleRequirement
        {
        }

        $this->assertTrue(true);
    }

    public function testMultipleRolesWithDifferentStrategies(): void
    {
        $any = RoleRequiredBuilder::create()
            ->roles('editor', 'contributor')
            ->strategy(RoleMatchStrategy::ANY)
            ->build();

        $all = RoleRequiredBuilder::create()
            ->roles('editor', 'reviewer')
            ->strategy(RoleMatchStrategy::ALL)
            ->build();

        $exactly = RoleRequiredBuilder::create()
            ->roles('admin')
            ->strategy(RoleMatchStrategy::EXACTLY)
            ->build();

        $this->assertSame(RoleMatchStrategy::ANY, $any->strategy);
        $this->assertSame(RoleMatchStrategy::ALL, $all->strategy);
        $this->assertSame(RoleMatchStrategy::EXACTLY, $exactly->strategy);
    }

    public function testRoleHierarchyPatterns(): void
    {
        // Admin > Manager > Employee hierarchy
        $admin = RoleRequiredBuilder::create()
            ->roles('admin')
            ->hierarchy(true)
            ->build();

        $manager = RoleRequiredBuilder::create()
            ->roles('manager')
            ->hierarchy(true)
            ->build();

        $employee = RoleRequiredBuilder::create()
            ->roles('employee')
            ->hierarchy(false)
            ->build();

        $this->assertTrue($admin->hierarchy);
        $this->assertTrue($manager->hierarchy);
        $this->assertFalse($employee->hierarchy);
    }
}
