# Security Policy

## Supported Versions

Currently supported versions for security updates:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security vulnerability within FraiseQL, please follow these steps:

### 1. Do NOT Create a Public Issue

Security vulnerabilities should not be reported through public GitHub issues.

### 2. Report Privately

Please report security vulnerabilities by emailing: [security email to be configured]

Include the following information:

- Type of vulnerability
- Full paths of source file(s) related to the vulnerability
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the vulnerability

### 3. Response Time

- We will acknowledge receipt of your vulnerability report within 48 hours
- We will provide a more detailed response within 7 days
- We will work on fixes and coordinate disclosure

## Security Best Practices for Users

When using FraiseQL:

1. **SQL Injection Protection**
   - Always use FraiseQL's built-in query builders
   - Never concatenate user input into queries
   - Use parameterized queries

2. **Authentication**
   - Implement proper authentication middleware
   - Use environment variables for sensitive configuration
   - Never commit credentials to version control

3. **Database Permissions**
   - Use least-privilege database users
   - Restrict database user permissions appropriately
   - Use read-only users where possible

4. **Dependencies**
   - Keep FraiseQL and dependencies updated
   - Monitor security advisories
   - Use tools like `pip-audit` to check for vulnerabilities

## Disclosure Policy

When we receive a security vulnerability report:

1. Confirm the vulnerability and determine affected versions
2. Develop fixes for all supported versions
3. Prepare security advisory
4. Release patches for all affected supported versions
5. Publish security advisory on GitHub

## Security Features

FraiseQL includes several security features:

- **SQL Injection Prevention**: All queries are parameterized
- **Type Safety**: Strong typing prevents many security issues
- **Input Validation**: Automatic validation based on GraphQL schema
- **Authentication Support**: Built-in authentication decorators

## Acknowledgments

We appreciate responsible disclosure of security vulnerabilities. Contributors who report valid security issues will be acknowledged in our security advisories (unless they prefer to remain anonymous).
