# SaaS Platform Example

A FraiseQL v2 schema for a Software-as-a-Service platform with multi-tenant organization.

## Schema Structure

```
schema/
├── accounts/       # Account management and multi-tenancy
│   └── types.json
├── billing/        # Subscriptions and invoicing
│   └── types.json
├── teams/          # Team and organization management
│   └── types.json
└── integrations/   # Third-party integrations
    └── types.json
```

## Domains

### Accounts Domain

- **Account**: SaaS account with subscription tier
- **AccountUser**: User membership in an account
- **Queries**: getAccount, getAccountBySlug, listAccountUsers
- **Mutations**: createAccount, updateAccount

### Billing Domain

- **Subscription**: Account subscription and billing information
- **Invoice**: Billing invoices
- **Queries**: getSubscription, listInvoices
- **Mutations**: updateSubscription, cancelSubscription

### Teams Domain

- **Team**: Team within an account
- **TeamMember**: Team membership with roles
- **Queries**: listTeams, getTeam, listTeamMembers
- **Mutations**: createTeam, addTeamMember

### Integrations Domain

- **Integration**: Third-party service configuration
- **WebhookLog**: Webhook delivery tracking
- **Queries**: listIntegrations, getIntegration, listWebhookLogs
- **Mutations**: addIntegration, removeIntegration

## Compiling

```bash
fraiseql compile fraiseql.toml
```

Auto-discovers domains and generates `schema.compiled.json`.

## Key Concepts

### Multi-Tenancy

Each domain includes `accountId` field for tenant isolation:

```
Account
├── AccountUser (accountId)
├── Subscription (accountId)
├── Team (accountId)
└── Integration (accountId)
```

### Domain Boundaries

- **Accounts** owns account identity
- **Billing** handles subscriptions and payments
- **Teams** manages org structure
- **Integrations** handles external services

### Security

Role-based access control per domain:

- Account owner: account settings
- Billing admin: subscription management
- Team lead: team management
- Integration admin: integration setup

## Example Workflows

### Create a New Customer Account

```graphql
mutation {
  createAccount(name: "Acme Corp", tier: "pro") {
    id
    name
    createdAt
  }
}
```

### Set Up Billing

```graphql
mutation {
  updateSubscription(
    accountId: "123"
    plan: "pro"
    billingCycle: "monthly"
  ) {
    status
    nextBillingDate
  }
}
```

### Organize into Teams

```graphql
mutation {
  createTeam(accountId: "123", name: "Engineering") {
    id
    name
  }
}
```

## Extending the Schema

### Add Support Domain

```bash
mkdir -p schema/support
# Add schema/support/types.json with Ticket, SupportUser, etc.
fraiseql compile fraiseql.toml
```

### Add Analytics Domain

```bash
mkdir -p schema/analytics
# Add schema/analytics/types.json with Metrics, Events, etc.
fraiseql compile fraiseql.toml
```

The new domains are automatically discovered!

## Production Considerations

1. **Database**: Each query should have corresponding database views
2. **Security**: Implement proper authz rules in fraiseql.toml
3. **Performance**: Add indexes on accountId for tenant isolation
4. **Compliance**: Ensure GDPR/data deletion compliance per account

See `../../docs/DOMAIN_ORGANIZATION.md` for more information.
