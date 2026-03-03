<?php

/**
 * Generate parity schema for cross-SDK comparison.
 *
 * Usage:
 *   php tests/GenerateParitySchema.php
 */

declare(strict_types=1);

require_once __DIR__ . '/../vendor/autoload.php';

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

$raw = StaticAPI::exportSchema();

// Normalise to array-of-items format (same as Python / TypeScript / Go output)
$output = [
    'types'     => normaliseSection($raw['types']     ?? []),
    'queries'   => normaliseSection($raw['queries']   ?? []),
    'mutations' => normaliseSection($raw['mutations'] ?? []),
];

echo json_encode($output, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . PHP_EOL;

// ── Helper ────────────────────────────────────────────────────────────────────

/**
 * Convert a section that may be a dict (name => data) or an array to a list.
 *
 * @param array<string|int, mixed> $section
 * @return list<mixed>
 */
function normaliseSection(array $section): array
{
    if (array_is_list($section)) {
        return $section;
    }
    // Dict-keyed-by-name: inject name key if missing
    $result = [];
    foreach ($section as $name => $item) {
        if (is_array($item) && !isset($item['name'])) {
            $item = array_merge(['name' => $name], $item);
        }
        $result[] = $item;
    }
    return $result;
}
