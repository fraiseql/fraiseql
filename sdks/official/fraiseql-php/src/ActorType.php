<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Exceptions\FraiseQLException;

/**
 * The `ActorType` roster #966's actor gate is an allow-list of.
 *
 * snake_case, as the compiler spells it
 * (`crates/fraiseql-core/src/security/actor_type.rs`) and as the change-log
 * `actor_type TEXT` column stores it.
 */
final class ActorType
{
    public const HUMAN_USER = 'human_user';
    public const SERVICE_ACCOUNT = 'service_account';
    public const AI_AGENT = 'ai_agent';
    public const SYSTEM_JOB = 'system_job';

    public const ALL = [
        self::HUMAN_USER,
        self::SERVICE_ACCOUNT,
        self::AI_AGENT,
        self::SYSTEM_JOB,
    ];

    /**
     * Validate an allow-list where the author wrote it.
     *
     * The compiler refuses an unknown token by name, but only at compile time, and this
     * is a security gate enforced in the same executor arm as `requires_role` on every
     * transport — one that fails late fails after the author has stopped looking (#1123).
     *
     * An empty list is refused rather than passed on: the compiled schema omits the key
     * when empty, so an empty allow-list reads as a declared gate and compiles to none.
     *
     * @param list<string> $actors
     * @throws FraiseQLException
     */
    public static function validate(array $actors, string $context): void
    {
        if ($actors === []) {
            throw new FraiseQLException(
                "{$context}: requiresActor() was given an empty list. An empty allow-list "
                . 'admits nobody and is dropped from the compiled schema, which admits '
                . 'everybody — name the actor types instead. Valid: ' . implode(', ', self::ALL) . '.',
            );
        }
        $unknown = array_values(array_diff($actors, self::ALL));
        if ($unknown !== []) {
            throw new FraiseQLException(
                "{$context}: requiresActor() names unknown actor type(s) "
                . implode(', ', $unknown) . '. Valid: ' . implode(', ', self::ALL) . '.',
            );
        }
    }
}
