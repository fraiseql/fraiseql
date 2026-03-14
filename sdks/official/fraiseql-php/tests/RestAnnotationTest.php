<?php

declare(strict_types=1);

namespace FraiseQL\Tests;

use FraiseQL\QueryBuilder;
use FraiseQL\MutationBuilder;
use FraiseQL\Schema;
use PHPUnit\Framework\TestCase;

/**
 * Tests for REST annotation support on QueryBuilder and MutationBuilder.
 */
class RestAnnotationTest extends TestCase
{
    protected function setUp(): void
    {
        Schema::reset();
    }

    protected function tearDown(): void
    {
        Schema::reset();
    }

    // -------------------------------------------------------------------------
    // QueryBuilder — REST annotations
    // -------------------------------------------------------------------------

    public function testQueryRestPathAndMethodInToArray(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->returnsList(true)
            ->sqlSource('v_user')
            ->restPath('/api/users')
            ->restMethod('GET');

        $arr = $query->toArray();

        $this->assertArrayHasKey('rest', $arr);
        $this->assertSame('/api/users', $arr['rest']['path']);
        $this->assertSame('GET', $arr['rest']['method']);
    }

    public function testQueryRestPathAndMethodInToIntermediateArray(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->returnsList(true)
            ->sqlSource('v_user')
            ->restPath('/api/users')
            ->restMethod('GET');

        $arr = $query->toIntermediateArray();

        $this->assertArrayHasKey('rest', $arr);
        $this->assertSame('/api/users', $arr['rest']['path']);
        $this->assertSame('GET', $arr['rest']['method']);
    }

    public function testQueryRestDefaultMethodIsGet(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->returnsList(true)
            ->sqlSource('v_user')
            ->restPath('/api/users');

        $arr = $query->toArray();

        $this->assertSame('GET', $arr['rest']['method']);
    }

    public function testQueryWithoutRestOmitsBlock(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->returnsList(true)
            ->sqlSource('v_user');

        $arr = $query->toArray();

        $this->assertArrayNotHasKey('rest', $arr);
    }

    public function testQueryRestMethodValidation(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('Invalid REST method');

        QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restMethod('CONNECT');
    }

    public function testQueryRestMethodCaseInsensitive(): void
    {
        $query = QueryBuilder::query('users')
            ->returnType('User')
            ->sqlSource('v_user')
            ->restPath('/api/users')
            ->restMethod('post');

        $arr = $query->toArray();

        $this->assertSame('POST', $arr['rest']['method']);
    }

    // -------------------------------------------------------------------------
    // MutationBuilder — REST annotations
    // -------------------------------------------------------------------------

    public function testMutationRestPathAndMethodInToArray(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->operation('insert')
            ->restPath('/api/users')
            ->restMethod('POST');

        $arr = $mutation->toArray();

        $this->assertArrayHasKey('rest', $arr);
        $this->assertSame('/api/users', $arr['rest']['path']);
        $this->assertSame('POST', $arr['rest']['method']);
    }

    public function testMutationRestPathAndMethodInToIntermediateArray(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->operation('insert')
            ->restPath('/api/users')
            ->restMethod('POST');

        $arr = $mutation->toIntermediateArray();

        $this->assertArrayHasKey('rest', $arr);
        $this->assertSame('/api/users', $arr['rest']['path']);
        $this->assertSame('POST', $arr['rest']['method']);
    }

    public function testMutationRestDefaultMethodIsPost(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->restPath('/api/users');

        $arr = $mutation->toArray();

        $this->assertSame('POST', $arr['rest']['method']);
    }

    public function testMutationWithoutRestOmitsBlock(): void
    {
        $mutation = MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->operation('insert');

        $arr = $mutation->toArray();

        $this->assertArrayNotHasKey('rest', $arr);
    }

    public function testMutationRestMethodValidation(): void
    {
        $this->expectException(\InvalidArgumentException::class);

        MutationBuilder::mutation('createUser')
            ->returnType('User')
            ->sqlSource('fn_create_user')
            ->restMethod('OPTIONS');
    }

    public function testMutationRestMethodDelete(): void
    {
        $mutation = MutationBuilder::mutation('deleteUser')
            ->returnType('User')
            ->sqlSource('fn_delete_user')
            ->operation('delete')
            ->restPath('/api/users/{id}')
            ->restMethod('DELETE');

        $arr = $mutation->toArray();

        $this->assertSame('DELETE', $arr['rest']['method']);
    }

    public function testMutationRestMethodPut(): void
    {
        $mutation = MutationBuilder::mutation('updateUser')
            ->returnType('User')
            ->sqlSource('fn_update_user')
            ->operation('update')
            ->restPath('/api/users/{id}')
            ->restMethod('PUT');

        $arr = $mutation->toIntermediateArray();

        $this->assertSame('PUT', $arr['rest']['method']);
    }

    public function testMutationRestMethodPatch(): void
    {
        $mutation = MutationBuilder::mutation('patchUser')
            ->returnType('User')
            ->sqlSource('fn_patch_user')
            ->restPath('/api/users/{id}')
            ->restMethod('PATCH');

        $arr = $mutation->toArray();

        $this->assertSame('PATCH', $arr['rest']['method']);
    }
}
