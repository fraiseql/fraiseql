<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * Singleton sentinel for "field not provided" in update mutation inputs.
 * Distinguishes from null (which means "set to null").
 *
 * Named UnsetValue because "unset" is a reserved keyword in PHP.
 *
 * Usage:
 *   $field = FraiseQL\UnsetValue::instance();
 *   if ($field === FraiseQL\UnsetValue::instance()) { // not provided }
 */
final class UnsetValue
{
    private static ?self $instance = null;

    private function __construct()
    {
    }

    public static function instance(): self
    {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        return self::$instance;
    }

    public function __toString(): string
    {
        return 'UNSET';
    }

    // Prevent cloning and unserialization
    private function __clone()
    {
    }
    public function __wakeup(): never
    {
        throw new \RuntimeException('Cannot unserialize UNSET singleton');
    }
}
