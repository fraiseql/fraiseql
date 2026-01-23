<?php

declare(strict_types=1);

require_once __DIR__ . '/../vendor/autoload.php';

use FraiseQL\Attributes\{GraphQLType, GraphQLField};
use FraiseQL\{SchemaRegistry, SchemaFormatter, ObserverBuilder, Webhook, SlackAction, EmailAction};

// Define Order type
#[GraphQLType(name: 'Order', description: 'E-commerce order')]
class Order
{
    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'String')]
    public string $customerEmail;

    #[GraphQLField(type: 'String')]
    public string $status;

    #[GraphQLField(type: 'Float')]
    public float $total;

    #[GraphQLField(type: 'DateTime')]
    public string $createdAt;
}

// Define Payment type
#[GraphQLType(name: 'Payment', description: 'Payment record')]
class Payment
{
    #[GraphQLField(type: 'ID')]
    public string $id;

    #[GraphQLField(type: 'ID')]
    public string $orderId;

    #[GraphQLField(type: 'Float')]
    public float $amount;

    #[GraphQLField(type: 'String')]
    public string $status;

    #[GraphQLField(type: 'DateTime', nullable: true)]
    public ?string $processedAt;
}

// Register types
$registry = SchemaRegistry::getInstance();
$registry->register(Order::class);
$registry->register(Payment::class);

// Observer 1: High-value orders
ObserverBuilder::create('onHighValueOrder')
    ->entity('Order')
    ->event('INSERT')
    ->condition('total > 1000')
    ->addAction(Webhook::create('https://api.example.com/high-value-orders'))
    ->addAction(SlackAction::create('#sales', '🎉 High-value order {id}: ${total}'))
    ->addAction(EmailAction::create(
        'sales@example.com',
        'High-value order {id}',
        'Order {id} for ${total} was created by {customer_email}'
    ))
    ->register();

// Observer 2: Order shipped
ObserverBuilder::create('onOrderShipped')
    ->entity('Order')
    ->event('UPDATE')
    ->condition("status.changed() and status == 'shipped'")
    ->addAction(Webhook::withEnv('SHIPPING_WEBHOOK_URL'))
    ->addAction(EmailAction::withFrom(
        '{customer_email}',
        'Your order {id} has shipped!',
        'Your order is on its way. Track it here: https://example.com/track/{id}',
        'noreply@example.com'
    ))
    ->register();

// Observer 3: Payment failures
ObserverBuilder::create('onPaymentFailure')
    ->entity('Payment')
    ->event('UPDATE')
    ->condition("status == 'failed'")
    ->addAction(SlackAction::create('#payments', '⚠️ Payment failed for order {order_id}: {amount}'))
    ->addAction(Webhook::create('https://api.example.com/payment-failures', [
        'headers' => ['Authorization' => 'Bearer {PAYMENT_API_TOKEN}']
    ]))
    ->retry(['max_attempts' => 5, 'backoff_strategy' => 'exponential'])
    ->register();

// Observer 4: Archive deleted orders
ObserverBuilder::create('onOrderDeleted')
    ->entity('Order')
    ->event('DELETE')
    ->addAction(Webhook::create('https://api.example.com/archive', [
        'body_template' => '{"type": "order", "id": "{{id}}", "data": {{_json}}}'
    ]))
    ->register();

// Observer 5: All new orders
ObserverBuilder::create('onOrderCreated')
    ->entity('Order')
    ->event('INSERT')
    ->addAction(SlackAction::create('#orders', 'New order {id} by {customer_email}'))
    ->register();

// Export schema
$formatter = new SchemaFormatter();
$schema = $formatter->formatRegistry($registry);
$schema->saveToFile('ecommerce_observers_schema.json');

echo "\n✅ Schema exported to ecommerce_observers_schema.json\n";
echo "   Types: " . $schema->getTypeCount() . "\n";
echo "   Observers: " . count($schema->observers) . "\n";

echo "\n🎯 Observer Summary:\n";
echo "   1. onHighValueOrder → Webhooks, Slack, Email for total > 1000\n";
echo "   2. onOrderShipped → Webhook + customer email when status='shipped'\n";
echo "   3. onPaymentFailure → Slack + webhook with retry on payment failures\n";
echo "   4. onOrderDeleted → Archive deleted orders via webhook\n";
echo "   5. onOrderCreated → Slack notification for all new orders\n";

echo "\n✨ Next steps:\n";
echo "   1. fraiseql-cli compile ecommerce_observers_schema.json\n";
echo "   2. fraiseql-server --schema ecommerce_observers_schema.compiled.json\n";
echo "   3. Observers will execute automatically on database changes!\n";
