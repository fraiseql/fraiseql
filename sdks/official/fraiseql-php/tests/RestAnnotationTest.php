<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use PHPUnit\Framework\TestCase;
use FraiseQL\QueryBuilder;
use FraiseQL\MutationBuilder;

/**
 * Tests for REST transport annotation support on QueryBuilder and MutationBuilder.
 */
final class RestAnnotationTest extends TestCase
{
    public function testQueryRestPathEmitsRestBlock(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restPath('/users/{id}')
            ->restMethod('GET');

        $intermediate = $query->toIntermediateArray();

        $this->assertArrayHasKey('rest', $intermediate);
        $this->assertSame('/users/{id}', $intermediate['rest']['path']);
        $this->assertSame('GET', $intermediate['rest']['method']);
    }

    public function testQueryRestPathDefaultMethodIsGet(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restPath('/users');

        $intermediate = $query->toIntermediateArray();

        $this->assertArrayHasKey('rest', $intermediate);
        $this->assertSame('GET', $intermediate['rest']['method']);
    }

    public function testQueryWithoutRestPathOmitsRestBlock(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user');

        $intermediate = $query->toIntermediateArray();

        $this->assertArrayNotHasKey('rest', $intermediate);
    }

    public function testQueryRestPathAppearsInToArray(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restPath('/users')
            ->restMethod('POST');

        $arr = $query->toArray();

        $this->assertArrayHasKey('rest', $arr);
        $this->assertSame('/users', $arr['rest']['path']);
        $this->assertSame('POST', $arr['rest']['method']);
    }

    public function testQueryRestMethodIsCaseInsensitive(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restPath('/users')
            ->restMethod('post');

        $intermediate = $query->toIntermediateArray();

        $this->assertSame('POST', $intermediate['rest']['method']);
    }

    public function testQueryRestMethodInvalidThrows(): void
    {
        $this->expectException(\InvalidArgumentException::class);

        QueryBuilder::query('users')
            ->returnType('User')
            ->restMethod('INVALID');
    }

    public function testMutationRestPathEmitsRestBlock(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->operation('insert')
            ->restPath('/users')
            ->restMethod('POST');

        $intermediate = $mutation->toIntermediateArray();

        $this->assertArrayHasKey('rest', $intermediate);
        $this->assertSame('/users', $intermediate['rest']['path']);
        $this->assertSame('POST', $intermediate['rest']['method']);
    }

    public function testMutationRestPathDefaultMethodIsPost(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->restPath('/users');

        $intermediate = $mutation->toIntermediateArray();

        $this->assertArrayHasKey('rest', $intermediate);
        $this->assertSame('POST', $intermediate['rest']['method']);
    }

    public function testMutationWithoutRestPathOmitsRestBlock(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user');

        $intermediate = $mutation->toIntermediateArray();

        $this->assertArrayNotHasKey('rest', $intermediate);
    }

    public function testMutationRestMethodInvalidThrows(): void
    {
        $this->expectException(\InvalidArgumentException::class);

        MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->restMethod('INVALID');
    }
}
