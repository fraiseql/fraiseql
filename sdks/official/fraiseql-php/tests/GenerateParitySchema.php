<?php

/**
 * Generate parity schema for cross-SDK comparison.
 *
 * Usage:
 *   php tests/GenerateParitySchema.php
 */

declare(strict_types=1);

require_once __DIR__ . '/../vendor/autoload.php';

use FraiseQL\SchemaExporter;
use FraiseQL\StaticAPI;
use FraiseQL\TypeBuilder;

// Reset registry
StaticAPI::clear();

// ── Types ──────────────────────────────────────────────────────────────────

TypeBuilder::type('User')
    ->sqlSource('v_user')
    ->field('id',    'ID',     nullable: false)
    ->field('email', 'String', nullable: false)
    ->field('name',  'String', nullable: false)
    ->register();

TypeBuilder::type('Order')
    ->sqlSource('v_order')
    ->field('id',    'ID',    nullable: false)
    ->field('total', 'Float', nullable: false)
    ->register();

TypeBuilder::type('UserNotFound')
    ->sqlSource('v_user_not_found')
    ->isError(true)
    ->field('message', 'String', nullable: false)
    ->field('code',    'String', nullable: false)
    ->register();

// ── Queries ─────────────────────────────────────────────────────────────────

StaticAPI::query('users')
    ->returnType('User')
    ->returnsList(true)
    ->sqlSource('v_user')
    ->register();

StaticAPI::query('tenantOrders')
    ->returnType('Order')
    ->returnsList(true)
    ->sqlSource('v_order')
    ->inject(['tenant_id' => 'jwt:tenant_id'])
    ->cacheTtlSeconds(300)
    ->requiresRole('admin')
    ->register();

// ── Mutations ────────────────────────────────────────────────────────────────

StaticAPI::mutation('createUser')
    ->returnType('User')
    ->sqlSource('fn_create_user')
    ->operation('insert')
    ->argument('email', 'String', nullable: false)
    ->argument('name',  'String', nullable: false)
    ->register();

StaticAPI::mutation('placeOrder')
    ->returnType('Order')
    ->sqlSource('fn_place_order')
    ->operation('insert')
    ->inject(['user_id' => 'jwt:sub'])
    ->invalidatesViews(['v_order_summary'])
    ->invalidatesFactTables(['tf_sales'])
    ->register();

// ── Export ───────────────────────────────────────────────────────────────────

// `SchemaExporter` is the exporter `vendor/bin/fraiseql export` runs, and it already
// emits every section as a list. This script used to call a second serializer and then
// hand-normalise its output, which is how the divergence went unnoticed: the normaliser
// flattened the top-level sections and left `fields` and `arguments` name-keyed, so
// `compare_schemas.py` died on `"id"["name"]` before comparing PHP, Java, or the golden
// fixture at all (#952). A generator that post-processes is a generator that can hide a
// producer defect — this one now emits exactly what the SDK ships.
$schema = SchemaExporter::toArray();

echo json_encode(
    [
        'types'     => $schema['types'],
        'queries'   => $schema['queries'],
        'mutations' => $schema['mutations'],
    ],
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES
) . PHP_EOL;
