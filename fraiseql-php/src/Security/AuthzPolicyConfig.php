<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Configuration for reusable authorization policies.
 *
 * @package FraiseQL\Security
 */
final readonly class AuthzPolicyConfig
{
    /**
     * @param string $name Policy name
     * @param string $description Policy description
     * @param string $rule Authorization rule expression
     * @param array<string> $attributes Attribute conditions for ABAC policies
     * @param AuthzPolicyType $type Policy type (RBAC, ABAC, CUSTOM, HYBRID)
     * @param bool $cacheable Whether to cache authorization decisions
     * @param int $cacheDurationSeconds Cache duration in seconds
     * @param bool $recursive Whether to apply recursively to nested types
     * @param string $operations Operation-specific rules
     * @param bool $auditLogging Whether to log access decisions
     * @param string $errorMessage Custom error message
     */
    public function __construct(
        public string $name,
        public string $description = '',
        public string $rule = '',
        public array $attributes = [],
        public AuthzPolicyType $type = AuthzPolicyType::CUSTOM,
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 300,
        public bool $recursive = false,
        public string $operations = '',
        public bool $auditLogging = false,
        public string $errorMessage = '',
    ) {
    }

    /**
     * Convert to array representation for JSON serialization.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $data = ['name' => $this->name];

        if ($this->description !== '') {
            $data['description'] = $this->description;
        }
        if ($this->rule !== '') {
            $data['rule'] = $this->rule;
        }
        if (!empty($this->attributes)) {
            $data['attributes'] = $this->attributes;
        }
        if ($this->type !== AuthzPolicyType::CUSTOM) {
            $data['type'] = $this->type->value;
        }
        if ($this->cacheable) {
            $data['cacheable'] = $this->cacheable;
            $data['cache_duration_seconds'] = $this->cacheDurationSeconds;
        }
        if ($this->recursive) {
            $data['recursive'] = $this->recursive;
        }
        if ($this->operations !== '') {
            $data['operations'] = $this->operations;
        }
        if ($this->auditLogging) {
            $data['audit_logging'] = $this->auditLogging;
        }
        if ($this->errorMessage !== '') {
            $data['error_message'] = $this->errorMessage;
        }

        return $data;
    }
}
