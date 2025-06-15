# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take security seriously at FraiseQL. If you discover a security vulnerability, please follow these steps:

1. **DO NOT** open a public issue
2. Email security@fraiseql.org with:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 7 days
- **Fix Timeline**: Depends on severity
  - Critical: 7-14 days
  - High: 14-30 days
  - Medium: 30-60 days
  - Low: Next release

## Security Best Practices

When using FraiseQL:

### SQL Injection Prevention
- Always use parameterized queries
- Never construct SQL strings manually
- Validate all user inputs

### Authentication
- Use strong JWT secrets
- Implement proper token expiration
- Enable rate limiting in production

### Database Security
- Use least-privilege database users
- Enable SSL for database connections
- Regularly update PostgreSQL

### Deployment
- Keep dependencies updated
- Use security headers
- Enable CORS appropriately
- Implement rate limiting

## Security Features

FraiseQL includes:
- SQL injection prevention
- Query complexity limiting
- Rate limiting support
- JWT authentication
- Input validation
- Parameterized queries

## Acknowledgments

We appreciate responsible disclosure and will acknowledge security researchers who follow this policy.