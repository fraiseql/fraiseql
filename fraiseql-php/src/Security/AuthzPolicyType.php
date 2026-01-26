<?php

declare(strict_types=1);

namespace FraiseQL\Security;

/**
 * Defines the type of authorization policy.
 *
 * @package FraiseQL\Security
 */
enum AuthzPolicyType: string
{
    /** Role-based access control (RBAC) */
    case RBAC = 'rbac';

    /** Attribute-based access control (ABAC) */
    case ABAC = 'abac';

    /** Custom rule expressions */
    case CUSTOM = 'custom';

    /** Hybrid approach combining multiple methods */
    case HYBRID = 'hybrid';
}
