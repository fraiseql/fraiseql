<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Fluent API for building custom authorization rules.
 *
 * Example:
 * ```php
 * AuthorizeBuilder::create()
 *     ->rule("isOwner(\$context.userId, \$field.ownerId)")
 *     ->description("Ensures users can only access their own notes")
 *     ->build();
 * ```
 *
 * @package FraiseQL\Security
 */
final class AuthorizeBuilder
{
    private string $rule = '';
    private string $policy = '';
    private string $description = '';
    private string $errorMessage = '';
    private bool $recursive = false;
    private string $operations = '';
    private bool $cacheable = true;
    private int $cacheDurationSeconds = 300;

    /**
     * Create a new AuthorizeBuilder instance.
     *
     * @return self
     */
    public static function create(): self
    {
        return new self();
    }

    /**
     * Set the authorization rule expression.
     *
     * @param string $rule
     * @return $this
     */
    public function rule(string $rule): self
    {
        $this->rule = $rule;
        return $this;
    }

    /**
     * Set the reference to a named authorization policy.
     *
     * @param string $policy
     * @return $this
     */
    public function policy(string $policy): self
    {
        $this->policy = $policy;
        return $this;
    }

    /**
     * Set the description of what this rule protects.
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
     * Set whether to apply rule hierarchically to child fields.
     *
     * @param bool $recursive
     * @return $this
     */
    public function recursive(bool $recursive): self
    {
        $this->recursive = $recursive;
        return $this;
    }

    /**
     * Set which operations this rule applies to (read, create, update, delete).
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
     * Set whether to cache authorization decisions.
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
     * Build the authorization configuration.
     *
     * @return AuthorizeConfig
     */
    public function build(): AuthorizeConfig
    {
        return new AuthorizeConfig(
            rule: $this->rule,
            policy: $this->policy,
            description: $this->description,
            errorMessage: $this->errorMessage,
            recursive: $this->recursive,
            operations: $this->operations,
            cacheable: $this->cacheable,
            cacheDurationSeconds: $this->cacheDurationSeconds,
        );
    }
}
