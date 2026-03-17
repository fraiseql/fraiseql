<?php

declare(strict_types=1);

namespace FraiseQL;

use FraiseQL\Exceptions\AuthenticationException;
use FraiseQL\Exceptions\FraiseQLException;
use FraiseQL\Exceptions\GraphQLException;
use FraiseQL\Exceptions\NetworkException;
use FraiseQL\Exceptions\RateLimitException;
use FraiseQL\Exceptions\TimeoutException;

final class FraiseQLClient
{
    public function __construct(
        private readonly string $url,
        private readonly ?string $authorization = null,
        private readonly ?RetryConfig $retry = null,
        private readonly float $timeout = 30.0,
    ) {}

    /**
     * Execute a GraphQL query.
     *
     * @param array<string, mixed> $variables
     * @return array<string, mixed>
     * @throws GraphQLException if the response contains errors
     * @throws NetworkException on transport failure
     * @throws TimeoutException if the request times out
     * @throws AuthenticationException on 401 or 403 responses
     * @throws RateLimitException on 429 responses
     */
    public function query(string $query, array $variables = [], ?string $operationName = null): array
    {
        return $this->execute($query, $variables, $operationName);
    }

    /**
     * Execute a GraphQL mutation.
     *
     * @param array<string, mixed> $variables
     * @return array<string, mixed>
     * @throws GraphQLException if the response contains errors
     * @throws NetworkException on transport failure
     * @throws TimeoutException if the request times out
     * @throws AuthenticationException on 401 or 403 responses
     * @throws RateLimitException on 429 responses
     */
    public function mutate(string $mutation, array $variables = [], ?string $operationName = null): array
    {
        return $this->execute($mutation, $variables, $operationName);
    }

    /**
     * @param array<string, mixed> $variables
     * @return array<string, mixed>
     * @throws GraphQLException
     * @throws NetworkException
     * @throws TimeoutException
     * @throws AuthenticationException
     * @throws RateLimitException
     */
    private function execute(string $gqlQuery, array $variables, ?string $operationName = null): array
    {
        $payload = ['query' => $gqlQuery];
        if (!empty($variables)) {
            $payload['variables'] = $variables;
        }
        if ($operationName !== null) {
            $payload['operationName'] = $operationName;
        }

        $body = json_encode($payload, JSON_THROW_ON_ERROR);

        $headers = [
            'Content-Type: application/json',
            'Accept: application/json',
        ];
        if ($this->authorization !== null) {
            $headers[] = "Authorization: {$this->authorization}";
        }

        $ch = curl_init();
        curl_setopt_array($ch, [
            CURLOPT_URL => $this->url,
            CURLOPT_POST => true,
            CURLOPT_POSTFIELDS => $body,
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_TIMEOUT => (int) $this->timeout,
            CURLOPT_CONNECTTIMEOUT => (int) min($this->timeout, 10),
            CURLOPT_HTTPHEADER => $headers,
        ]);

        $response = curl_exec($ch);
        $errno = curl_errno($ch);
        $info = curl_getinfo($ch);
        curl_close($ch);

        if ($errno === CURLE_OPERATION_TIMEDOUT) {
            throw new TimeoutException('Request timed out');
        }
        if ($response === false) {
            throw new NetworkException("cURL error {$errno}: " . curl_strerror($errno));
        }

        $httpCode = (int) $info['http_code'];
        if ($httpCode === 401 || $httpCode === 403) {
            throw new AuthenticationException($httpCode);
        }
        if ($httpCode === 429) {
            throw new RateLimitException();
        }

        /** @var array{data?: mixed, errors?: list<array{message: string}>|null} $parsed */
        $parsed = json_decode((string) $response, true, 512, JSON_THROW_ON_ERROR);

        // null errors = success (cross-SDK invariant)
        if (!empty($parsed['errors'])) {
            throw new GraphQLException($parsed['errors']);
        }

        return (array) ($parsed['data'] ?? []);
    }
}
