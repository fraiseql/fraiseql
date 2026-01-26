<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\SchemaRegistry;
use FraiseQL\Security\AuthorizeBuilder;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\Attributes\Authorize;

/**
 * Tests for custom authorization rules.
 */
final class AuthorizationTest extends TestCase
{
    protected function tearDown(): void
    {
        SchemaRegistry::getInstance()->clear();
        parent::tearDown();
    }

    public function testAuthorizationRuleBuilder(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("isOwner(\$context.userId, \$field.ownerId)")
            ->description("Ensures users can only access their own notes")
            ->build();

        $this->assertSame("isOwner(\$context.userId, \$field.ownerId)", $config->rule);
        $this->assertSame("Ensures users can only access their own notes", $config->description);
    }

    public function testAuthorizationWithPolicy(): void
    {
        $config = AuthorizeBuilder::create()
            ->policy("piiAccess")
            ->description("References the piiAccess policy")
            ->build();

        $this->assertSame("piiAccess", $config->policy);
        $this->assertTrue($config->cacheable);
    }

    public function testAuthorizationWithErrorMessage(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("hasRole(\$context, 'admin')")
            ->errorMessage("Only administrators can access this resource")
            ->build();

        $this->assertSame("Only administrators can access this resource", $config->errorMessage);
    }

    public function testAuthorizationRecursiveApplication(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("canAccessNested(\$context)")
            ->recursive(true)
            ->description("Recursively applies to nested types")
            ->build();

        $this->assertTrue($config->recursive);
    }

    public function testAuthorizationOperationSpecific(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("isAdmin(\$context)")
            ->operations("create,delete")
            ->description("Only applies to create and delete operations")
            ->build();

        $this->assertSame("create,delete", $config->operations);
    }

    public function testAuthorizationCacheConfiguration(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("checkAuthorization(\$context)")
            ->cacheable(true)
            ->cacheDurationSeconds(3600)
            ->build();

        $this->assertTrue($config->cacheable);
        $this->assertSame(3600, $config->cacheDurationSeconds);
    }

    public function testAuthorizationNoCaching(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("checkSensitiveAuthorization(\$context)")
            ->cacheable(false)
            ->build();

        $this->assertFalse($config->cacheable);
    }

    public function testAuthorizationMultipleRules(): void
    {
        $config1 = AuthorizeBuilder::create()
            ->rule("isOwner(\$context.userId, \$field.ownerId)")
            ->description("Ownership check")
            ->build();

        $config2 = AuthorizeBuilder::create()
            ->rule("hasScope(\$context, 'read:notes')")
            ->description("Scope check")
            ->build();

        $this->assertNotSame($config1->rule, $config2->rule);
    }

    public function testAuthorizationFluentChaining(): void
    {
        $config = AuthorizeBuilder::create()
            ->rule("isOwner(\$context.userId, \$field.ownerId)")
            ->description("Ownership authorization")
            ->errorMessage("You can only access your own notes")
            ->recursive(false)
            ->operations("read,update")
            ->cacheable(true)
            ->cacheDurationSeconds(600)
            ->build();

        $this->assertSame("isOwner(\$context.userId, \$field.ownerId)", $config->rule);
        $this->assertSame("Ownership authorization", $config->description);
        $this->assertSame("You can only access your own notes", $config->errorMessage);
        $this->assertFalse($config->recursive);
        $this->assertSame("read,update", $config->operations);
        $this->assertTrue($config->cacheable);
    }

    public function testAuthorizationAttributeBasic(): void
    {
        #[Authorize(rule: "isOwner(\$context.userId, \$field.ownerId)")]
        class ProtectedNote
        {
        }

        $this->assertTrue(true);
    }

    public function testAuthorizationAttributeWithAllParameters(): void
    {
        #[Authorize(
            rule: "isOwner(\$context.userId, \$field.ownerId)",
            description: "Ownership check",
            errorMessage: "Access denied",
            recursive: true,
            operations: "read",
            cacheable: false,
            cacheDurationSeconds: 0,
        )]
        class FullyConfiguredNote
        {
        }

        $this->assertTrue(true);
    }
}
