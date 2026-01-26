<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Fluent API for building role-based access control rules.
 *
 * Example:
 * ```php
 * RoleRequiredBuilder::create()
 *     ->roles('manager', 'director')
 *     ->strategy(RoleMatchStrategy::ANY)
 *     ->description('Managers and directors can view salaries')
 *     ->build();
 * ```
 *
 * @package FraiseQL\Security
 */
final class RoleRequiredBuilder
{
    /** @var array<string> */
    private array $roles = [];
    private RoleMatchStrategy $strategy = RoleMatchStrategy::ANY;
    private bool $hierarchy = false;
    private string $description = '';
    private string $errorMessage = '';
    private string $operations = '';
    private bool $inherit = true;
    private bool $cacheable = true;
    private int $cacheDurationSeconds = 600;

    /**
     * Create a new RoleRequiredBuilder instance.
     *
     * @return self
     */
    public static function create(): self
    {
        return new self();
    }

    /**
     * Set required roles (variadic for convenience).
     *
     * @param string ...$roles
     * @return $this
     */
    public function roles(string ...$roles): self
    {
        $this->roles = $roles;
        return $this;
    }

    /**
     * Set required roles from an array.
     *
     * @param array<string> $roles
     * @return $this
     */
    public function rolesArray(array $roles): self
    {
        $this->roles = $roles;
        return $this;
    }

    /**
     * Set the role matching strategy.
     *
     * @param RoleMatchStrategy $strategy
     * @return $this
     */
    public function strategy(RoleMatchStrategy $strategy): self
    {
        $this->strategy = $strategy;
        return $this;
    }

    /**
     * Set whether roles form a hierarchy.
     *
     * @param bool $hierarchy
     * @return $this
     */
    public function hierarchy(bool $hierarchy): self
    {
        $this->hierarchy = $hierarchy;
        return $this;
    }

    /**
     * Set the description of the role requirement.
     *
     * @param string $description
     * @return $this
     */
    public function description(string $description): self
    {
        $this->description = $description;
        return $this;
    }

    /**
     * Set the custom error message.
     *
     * @param string $errorMessage
     * @return $this
     */
    public function errorMessage(string $errorMessage): self
    {
        $this->errorMessage = $errorMessage;
        return $this;
    }

    /**
     * Set which operations this rule applies to.
     *
     * @param string $operations
     * @return $this
     */
    public function operations(string $operations): self
    {
        $this->operations = $operations;
        return $this;
    }

    /**
     * Set whether to inherit role requirements from parent types.
     *
     * @param bool $inherit
     * @return $this
     */
    public function inherit(bool $inherit): self
    {
        $this->inherit = $inherit;
        return $this;
    }

    /**
     * Set whether to cache role validation results.
     *
     * @param bool $cacheable
     * @return $this
     */
    public function cacheable(bool $cacheable): self
    {
        $this->cacheable = $cacheable;
        return $this;
    }

    /**
     * Set the cache duration in seconds.
     *
     * @param int $duration
     * @return $this
     */
    public function cacheDurationSeconds(int $duration): self
    {
        $this->cacheDurationSeconds = $duration;
        return $this;
    }

    /**
     * Build the role configuration.
     *
     * @return RoleRequiredConfig
     */
    public function build(): RoleRequiredConfig
    {
        return new RoleRequiredConfig(
            roles: $this->roles,
            strategy: $this->strategy,
            hierarchy: $this->hierarchy,
            description: $this->description,
            errorMessage: $this->errorMessage,
            operations: $this->operations,
            inherit: $this->inherit,
            cacheable: $this->cacheable,
            cacheDurationSeconds: $this->cacheDurationSeconds,
        );
    }
}
