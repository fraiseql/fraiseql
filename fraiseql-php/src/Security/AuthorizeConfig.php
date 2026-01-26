<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Configuration for custom authorization rules.
 *
 * @package FraiseQL\Security
 */
final readonly class AuthorizeConfig
{
    /**
     * @param string $rule Authorization rule expression
     * @param string $policy Reference to a named policy
     * @param string $description Description of what this rule protects
     * @param string $errorMessage Custom error message on denial
     * @param bool $recursive Whether to apply hierarchically to child fields
     * @param string $operations Operation-specific rules (read, create, update, delete)
     * @param bool $cacheable Whether to cache authorization decisions
     * @param int $cacheDurationSeconds Cache duration in seconds
     */
    public function __construct(
        public string $rule = '',
        public string $policy = '',
        public string $description = '',
        public string $errorMessage = '',
        public bool $recursive = false,
        public string $operations = '',
        public bool $cacheable = true,
        public int $cacheDurationSeconds = 300,
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

        if ($this->rule !== '') {
            $data['rule'] = $this->rule;
        }
        if ($this->policy !== '') {
            $data['policy'] = $this->policy;
        }
        if ($this->description !== '') {
            $data['description'] = $this->description;
        }
        if ($this->errorMessage !== '') {
            $data['error_message'] = $this->errorMessage;
        }
        if ($this->recursive) {
            $data['recursive'] = $this->recursive;
        }
        if ($this->operations !== '') {
            $data['operations'] = $this->operations;
        }
        if ($this->cacheable) {
            $data['cacheable'] = $this->cacheable;
            $data['cache_duration_seconds'] = $this->cacheDurationSeconds;
        }

        return $data;
    }
}
