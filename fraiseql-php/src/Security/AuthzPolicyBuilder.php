<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Fluent API for building reusable authorization policies.
 *
 * Example:
 * ```php
 * AuthzPolicyBuilder::create('piiAccess')
 *     ->type(AuthzPolicyType::RBAC)
 *     ->rule("hasRole(\$context, 'data_manager') OR hasScope(\$context, 'read:pii')")
 *     ->description("Access to Personally Identifiable Information")
 *     ->build();
 * ```
 *
 * @package FraiseQL\Security
 */
final class AuthzPolicyBuilder
{
    private string $name;
    private string $description = '';
    private string $rule = '';
    /** @var array<string> */
    private array $attributes = [];
    private AuthzPolicyType $type = AuthzPolicyType::CUSTOM;
    private bool $cacheable = true;
    private int $cacheDurationSeconds = 300;
    private bool $recursive = false;
    private string $operations = '';
    private bool $auditLogging = false;
    private string $errorMessage = '';

    /**
     * Create a new AuthzPolicyBuilder instance.
     *
     * @param string $name Policy name
     * @return self
     */
    public static function create(string $name): self
    {
        $builder = new self();
        $builder->name = $name;
        return $builder;
    }

    /**
     * Set the policy description.
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
     * Set attribute conditions for ABAC policies (variadic).
     *
     * @param string ...$attributes
     * @return $this
     */
    public function attributes(string ...$attributes): self
    {
        $this->attributes = $attributes;
        return $this;
    }

    /**
     * Set attribute conditions from an array.
     *
     * @param array<string> $attributes
     * @return $this
     */
    public function attributesArray(array $attributes): self
    {
        $this->attributes = $attributes;
        return $this;
    }

    /**
     * Set the policy type.
     *
     * @param AuthzPolicyType $type
     * @return $this
     */
    public function type(AuthzPolicyType $type): self
    {
        $this->type = $type;
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
     * Set whether to apply recursively to nested types.
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
     * Set which operations this policy applies to.
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
     * Set whether to log access decisions.
     *
     * @param bool $auditLogging
     * @return $this
     */
    public function auditLogging(bool $auditLogging): self
    {
        $this->auditLogging = $auditLogging;
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
     * Build the authorization policy configuration.
     *
     * @return AuthzPolicyConfig
     */
    public function build(): AuthzPolicyConfig
    {
        return new AuthzPolicyConfig(
            name: $this->name,
            description: $this->description,
            rule: $this->rule,
            attributes: $this->attributes,
            type: $this->type,
            cacheable: $this->cacheable,
            cacheDurationSeconds: $this->cacheDurationSeconds,
            recursive: $this->recursive,
            operations: $this->operations,
            auditLogging: $this->auditLogging,
            errorMessage: $this->errorMessage,
        );
    }
}
