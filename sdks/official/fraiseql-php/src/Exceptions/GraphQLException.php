<?php

declare(strict_types=1);

namespace FraiseQL\Exceptions;

class GraphQLException extends FraiseQLException
{
    /** @param list<array{message: string}> $errors */
    public function __construct(
        public readonly array $errors,
        string $message = ''
    ) {
        parent::__construct($message ?: ($errors[0]['message'] ?? 'GraphQL error'));
    }
}
