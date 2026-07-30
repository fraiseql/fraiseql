<?php

/**
 * Author the cross-SDK conformance fixture with the PHP SDK's public API.
 *
 * Driven by sdks/official/conformance/run.py; see sdks/official/conformance/README.md.
 *
 * The one rule for every SDK's copy of this file: author through the SDK, never
 * hand-assemble the JSON. This goes through the attribute-based idiom the
 * SchemaExporter docblock documents — StaticAPI::register() plus the query/mutation
 * builders — and exports with SchemaExporter, which is what `bin/fraiseql export` runs.
 */

declare(strict_types=1);

require_once __DIR__ . '/../vendor/autoload.php';

use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;

#[GraphQLType(name: 'User', sqlSource: 'v_user', relay: true)]
final class ConformanceUser
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $email;

    #[GraphQLField(type: 'String', nullable: true, description: 'The user\'s "display" name')]
    public ?string $name;

    #[GraphQLField(type: 'Float', nullable: true, scope: 'read:User.salary')]
    public ?float $salary;
}

#[GraphQLType(name: 'Order', sqlSource: 'v_order')]
final class ConformanceOrder
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'Float', nullable: false)]
    public float $total;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $status;
}

#[GraphQLType(name: 'UserNotFound', sqlSource: 'v_user_not_found', isError: true)]
final class ConformanceUserNotFound
{
    #[GraphQLField(type: 'String', nullable: false)]
    public string $message;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $code;
}

#[GraphQLType(name: 'CreateUserInput', isInput: true)]
final class ConformanceCreateUserInput
{
    #[GraphQLField(type: 'String', nullable: false)]
    public string $email;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $name;
}

#[GraphQLType(name: 'User', sqlSource: 'v_user')]
final class ConformanceMinimalUser
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $email;
}

function authorMinimal(): void
{
    StaticAPI::register(ConformanceMinimalUser::class);

    StaticAPI::query('users')
        ->returnType('User')
        ->returnsList()
        ->nullable(false)
        ->sqlSource('v_user')
        ->register();
}

function authorFull(): void
{
    StaticAPI::register(ConformanceUser::class);
    StaticAPI::register(ConformanceOrder::class);
    StaticAPI::register(ConformanceUserNotFound::class);
    StaticAPI::register(ConformanceCreateUserInput::class);

    StaticAPI::enum('OrderStatus', ['PENDING', 'SHIPPED', 'CANCELLED']);

    StaticAPI::query('users')
        ->returnType('User')
        ->returnsList()
        ->nullable(false)
        ->sqlSource('v_user')
        ->register();

    StaticAPI::query('user')
        ->returnType('User')
        ->returnsList(false)
        ->nullable(true)
        ->sqlSource('v_user')
        ->argument('id', 'ID', nullable: false)
        ->register();

    StaticAPI::query('tenantOrders')
        ->returnType('Order')
        ->returnsList()
        ->nullable(false)
        ->sqlSource('v_order')
        ->inject(['tenant_id' => 'jwt:tenant_id'])
        ->cacheTtlSeconds(300)
        ->requiresRole('admin')
        ->register();

    StaticAPI::mutation('createUser')
        ->returnType('User')
        ->sqlSource('fn_create_user')
        ->operation('insert')
        ->argument('email', 'String', nullable: false)
        ->argument('name', 'String', nullable: true)
        ->invalidatesViews(['v_user', 'v_user_summary'])
        ->invalidatesFactTables(['tf_signup'])
        ->register();

    StaticAPI::mutation('placeOrder')
        ->returnType('Order')
        ->sqlSource('fn_place_order')
        ->operation('insert')
        ->inject(['user_id' => 'jwt:sub'])
        ->invalidatesViews(['v_order_summary'])
        ->invalidatesFactTables(['tf_sale'])
        ->register();
}

$fixture = getenv('FRAISEQL_CONFORMANCE_FIXTURE');
$out     = getenv('FRAISEQL_CONFORMANCE_OUT');
if ($fixture === false || $out === false) {
    fwrite(STDERR, "FRAISEQL_CONFORMANCE_FIXTURE and FRAISEQL_CONFORMANCE_OUT must be set\n");
    exit(2);
}

StaticAPI::clear();
match ($fixture) {
    'minimal' => authorMinimal(),
    'full'    => authorFull(),
    default   => throw new RuntimeException("unknown fixture $fixture"),
};

SchemaExporter::exportToFile($out);
