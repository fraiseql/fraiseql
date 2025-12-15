"""Meta-tests for FraiseQL integration coverage.

Meta-tests validate that ALL components of a category work through the
complete pipeline. They auto-enumerate components and test each one
end-to-end to prevent entire classes of integration bugs.

These tests are designed to:
- Auto-discover all components in a category (scalars, operators, decorators, etc.)
- Test each component through the complete GraphQL pipeline
- Fail immediately if ANY component doesn't integrate properly
- Prevent regressions by catching integration issues before they reach production

Meta-tests follow the pattern established by test_operator_registration.py
but extend it to full E2E validation rather than just registration checking.
"""
