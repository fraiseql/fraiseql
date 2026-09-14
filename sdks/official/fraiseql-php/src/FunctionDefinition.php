<?php

declare(strict_types=1);

namespace FraiseQL;

/**
 * A serverless function definition (#1325) — an out-of-band handler the server
 * dispatches on a trigger and runs in a WASM or Deno sandbox.
 *
 * The name is also the module **file stem**: the server loads
 * `<module_dir>/<name>.<ext>`, so it is carried verbatim and never recased.
 * `module_dir` and `dlq_store` are deliberately absent — they are deployment
 * settings owned by `[functions]` in `fraiseql.toml`.
 */
final class FunctionDefinition
{
    /**
     * @param string $name Function name, and the module file stem.
     * @param string $trigger e.g. `after:mutation:Order:update`. `after:mutation`
     *     matches the mutation's RETURN TYPE, not its name.
     * @param string $runtime `Deno` or `Wasm`.
     * @param int|null $timeoutMs Timeout override in milliseconds.
     * @param array<string, mixed>|null $runAs Authority ceiling (#594); absent ⇒ fail-closed.
     * @param array<int, array<string, mixed>> $when `when` predicates (#597).
     * @param bool $reRunnable Opt out of durable dispatch.
     * @param array<string, mixed>|null $retry Per-function retry policy.
     */
    public function __construct(
        public readonly string $name,
        public readonly string $trigger,
        public readonly string $runtime = 'Deno',
        public readonly ?int $timeoutMs = null,
        public readonly ?array $runAs = null,
        public readonly array $when = [],
        public readonly bool $reRunnable = false,
        public readonly ?array $retry = null,
    ) {
    }

    /**
     * The compiled-schema shape. Keys are snake_case because that is what the
     * compiler reads; only keys the author set are emitted.
     *
     * @return array<string, mixed>
     */
    public function toArray(): array
    {
        $definition = [
            'name'    => $this->name,
            'trigger' => $this->trigger,
            'runtime' => $this->runtime,
        ];
        if ($this->timeoutMs !== null) {
            $definition['timeout_ms'] = $this->timeoutMs;
        }
        if ($this->runAs !== null) {
            $definition['run_as'] = $this->runAs;
        }
        if ($this->when !== []) {
            $definition['when'] = $this->when;
        }
        if ($this->reRunnable) {
            $definition['re_runnable'] = true;
        }
        if ($this->retry !== null) {
            $definition['retry'] = $this->retry;
        }

        return $definition;
    }
}
