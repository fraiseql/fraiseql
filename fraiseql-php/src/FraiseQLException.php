<?php

declare(strict_types=1);

namespace FraiseQL;

use Exception;

/**
 * Base exception class for all FraiseQL errors.
 *
 * Provides a consistent error hierarchy for schema compilation, validation,
 * and execution errors.
 */
class FraiseQLException extends Exception
{
}
