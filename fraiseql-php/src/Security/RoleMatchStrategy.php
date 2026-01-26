<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Defines how to match multiple roles in RBAC.
 *
 * @package FraiseQL\Security
 */
enum RoleMatchStrategy: string
{
    /** User must have at least one of the specified roles */
    case ANY = 'any';

    /** User must have all of the specified roles */
    case ALL = 'all';

    /** User must have exactly these roles, no more, no less */
    case EXACTLY = 'exactly';
}
