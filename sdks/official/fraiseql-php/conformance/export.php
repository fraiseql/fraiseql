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

use FraiseQL\ActorType;
use FraiseQL\Attributes\GraphQLField;
use FraiseQL\Attributes\GraphQLType;
use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use FraiseQL\VectorConfig;

#[GraphQLType(name: 'User', sqlSource: 'v_user', relay: true)]
final class ConformanceUser
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $email;

    #[GraphQLField(
        type: 'String',
        nullable: true,
        description: 'The user\'s "display" name',
        deprecated: 'use displayName',
    )]
    public ?string $name;

    #[GraphQLField(type: 'Float', nullable: true, scope: 'read:User.salary')]
    public ?float $salary;

    // Two words and a digit segment (#1249). A PHP property is idiomatically
    // camelCase and is emitted verbatim, so these match the reference as written;
    // the SDKs that translate are the ones with snake_case or PascalCase
    // identifiers (Python, Ruby, Elixir, C#, F#).
    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $lastLoginAt;

    #[GraphQLField(type: 'String', nullable: true)]
    public ?string $phone1;
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

// `crud` is an authoring-time expansion the compiler has no concept of, so the only
// evidence this SDK implements it is that the operations and input objects appear in the
// compiled schema. `computed` is the same: emitting the flag makes the document
// uncompilable, so the sole evidence it was honoured is `slug` on the type and
// absent from both input objects.
#[GraphQLType(name: 'SupportTicket', sqlSource: 'v_support_ticket', crud: true)]
final class ConformanceSupportTicket
{
    #[GraphQLField(type: 'Int', nullable: false)]
    public int $id;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $title;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $dueDate;

    #[GraphQLField(type: 'String', nullable: false, computed: true)]
    public string $slug;
}

#[GraphQLType(name: 'UserNotFound', sqlSource: 'v_user_not_found', isError: true)]
final class ConformanceUserNotFound
{
    #[GraphQLField(type: 'String', nullable: false)]
    public string $message;

    #[GraphQLField(type: 'String', nullable: false)]
    public string $code;
}

#[GraphQLType(name: 'Document', sqlSource: 'v_document')]
final class ConformanceDocument
{
    #[GraphQLField(type: 'ID', nullable: false)]
    public string $id;

    #[GraphQLField(type: 'Vector', nullable: false, vectorConfig: new VectorConfig(
        dimensions: 1536,
        indexType: VectorConfig::INDEX_IVF_FLAT,
        distanceMetric: VectorConfig::METRIC_L2,
    ))]
    public array $embedding;

    #[GraphQLField(type: 'BitVector', nullable: false, vectorConfig: new VectorConfig(
        dimensions: 768,
        distanceMetric: VectorConfig::METRIC_HAMMING,
    ))]
    public string $fingerprint;

    #[GraphQLField(type: 'HalfVector', nullable: true, vectorConfig: new VectorConfig(
        dimensions: 1536,
        distanceMetric: VectorConfig::METRIC_INNER_PRODUCT,
    ))]
    public ?array $compact;

    #[GraphQLField(type: 'SparseVector', nullable: true, vectorConfig: new VectorConfig(
        dimensions: 30000,
        indexType: VectorConfig::INDEX_NONE,
    ))]
    public ?string $terms;

    #[GraphQLField(type: 'Float', nullable: false, vectorDistance: 'embedding')]
    public float $similarity;
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
    StaticAPI::register(ConformanceSupportTicket::class);
    StaticAPI::register(ConformanceUserNotFound::class);
    StaticAPI::register(ConformanceDocument::class);
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
        // #966's actor allow-list, enforced in the same executor gate as requiresRole on
        // every transport, and authorable in no SDK until #1123.
        ->requiresActor([ActorType::HUMAN_USER, ActorType::SERVICE_ACCOUNT])
        ->register();

    StaticAPI::mutation('createUser')
        ->returnType('User')
        ->sqlSource('fn_create_user')
        ->operation('insert')
        ->argument('email', 'String', nullable: false)
        ->argument('name', 'String', nullable: true)
        ->invalidatesViews(['v_user', 'v_user_summary'])
        ->invalidatesFactTables(['tf_signup'])
        ->requiresActor([ActorType::SERVICE_ACCOUNT])
        ->register();

    StaticAPI::mutation('placeOrder')
        ->returnType('Order')
        ->sqlSource('fn_place_order')
        ->operation('insert')
        ->inject(['user_id' => 'jwt:sub'])
        ->invalidatesViews(['v_order_summary'])
        ->invalidatesFactTables(['tf_sale'])
        ->register();

    StaticAPI::subscription('orderUpdated')
        ->entityType('Order')
        ->argument('orderId', 'ID', argNullable: true)
        ->description('Stream of order update events')
        ->topic('order_events')
        ->filterCondition('orderId', '$.id')
        ->fields(['id', 'total'])
        ->build();
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
