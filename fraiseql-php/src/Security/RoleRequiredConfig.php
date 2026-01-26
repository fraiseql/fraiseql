<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Configuration for role-based access control.
 *
 * @package FraiseQL\Security
 */
final readonly class RoleRequiredConfig
{
    /**
     * @param array<string> $roles Required roles
     * @param RoleMatchStrategy $strategy Role matching strategy (ANY, ALL, EXACTLY)
     * @param bool $hierarchy Whether roles form a hierarchy
     * @param string $description Description of the role requirement
     * @param string $errorMessage Custom error message on denial
     * @param string $operations Operation-specific rules
     * @param bool $inherit Whether to inherit role requirements from parent types
     * @param bool $cacheable Whether to cache role validation results
     * @param int $cacheDurationSeconds Cache duration in seconds
     */
    public function __construct(
        public array $roles = [],
        public RoleMatchStrategy $strategy = RoleMatchStrategy::ANY,
        public bool $hierarchy = false,
        public string $description = '',
        public string $errorMessage = '',
        public string $operations = '',
        public bool $inherit = true,
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 600,
    ) {
    }

    /**
     * Convert to array representation for JSON serialization.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $data = [];

        if (!empty($this->roles)) {
            $data['roles'] = $this->roles;
        }
        if ($this->strategy !== RoleMatchStrategy::ANY) {
            $data['strategy'] = $this->strategy->value;
        }
        if ($this->hierarchy) {
            $data['hierarchy'] = $this->hierarchy;
        }
        if ($this->description !== '') {
            $data['description'] = $this->description;
        }
        if ($this->errorMessage !== '') {
            $data['error_message'] = $this->errorMessage;
        }
        if ($this->operations !== '') {
            $data['operations'] = $this->operations;
        }
        if (!$this->inherit) {
            $data['inherit'] = $this->inherit;
        }
        if ($this->cacheable) {
            $data['cacheable'] = $this->cacheable;
            $data['cache_duration_seconds'] = $this->cacheDurationSeconds;
        }

        return $data;
    }
}
