# OAuth Provider Selection Guide

**Status:** ✅ Production Ready
**Audience:** Architects, DevOps, Security Engineers
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

## Quick Decision

```
Internal Team Only?
├─ YES → Keycloak (self-hosted) or SCRAM
│
Public Users?
├─ YES → Google OAuth (easiest) or Auth0 (more features)
│
Need Specific IDP?
├─ AWS Cognito → Configure as OIDC provider
├─ Azure AD → Configure as OIDC provider
├─ Okta → Configure as OIDC provider
├─ GitHub (developers) → GitHub OAuth
└─ SAML requirement? → Keycloak + SAML bridge
```

---

## Comparison Matrix

### Setup & Operations

| Provider | Effort | Time | Complexity | Cost |
|----------|--------|------|-----------|------|
| **Google OAuth** | 🟢 Easy | 15 min | 🟢 Low | Free |
| **Auth0** | 🟡 Medium | 30 min | 🟡 Medium | $0-$2,500/mo |
| **Keycloak** | 🔴 Hard | 2-4 hrs | 🔴 High | Free (self-hosted) |
| **SCRAM** | 🟢 Easy | 10 min | 🟢 Very Low | Free |
| **AWS Cognito** | 🟡 Medium | 45 min | 🟡 Medium | Pay-per-auth |
| **Azure AD** | 🟡 Medium | 45 min | 🟡 Medium | Included w/Azure |
| **Okta** | 🔴 Hard | 1-2 hrs | 🔴 High | $2,000+/mo |

### Features

| Feature | Google | Auth0 | Keycloak | SCRAM | Cognito | Azure AD |
|---------|--------|--------|----------|-------|---------|----------|
| **OAuth 2.0** | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| **OIDC** | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| **MFA/2FA** | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| **Social Login** | N/A | ✅ Multiple | ⚠️ Setup | ❌ | ❌ | ❌ |
| **SAML** | ❌ | ✅ | ✅ | ❌ | ❌ | ✅ |
| **LDAP/AD** | ❌ | ✅ | ✅ | ⚠️ Custom | ❌ | ✅ |
| **Self-Hosted** | ❌ | ❌ | ✅ | ✅ | ❌ | ❌ |
| **Managed Service** | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ |
| **Role-Based Access** | ⚠️ Custom | ✅ | ✅ | ❌ | ✅ | ✅ |
| **Custom Attributes** | ⚠️ Limited | ✅ | ✅ | ✅ | ✅ | ✅ |

### Users & Teams

| Provider | Public API? | Team Users? | Enterprise? | Compliance |
|----------|-----------|-----------|-----------|-----------|
| **Google** | ✅ Yes | ⚠️ Workspace | ⚠️ Basic | SOC 2 |
| **Auth0** | ✅ Yes | ✅ Yes | ✅ Yes | SOC 2, GDPR |
| **Keycloak** | N/A | ✅ Yes | ✅ Yes | Depends |
| **SCRAM** | N/A | ✅ Yes | ❌ No | None |
| **Cognito** | ✅ AWS only | ✅ Yes | ✅ Yes | SOC 2, HIPAA |
| **Azure AD** | ✅ Enterprise | ✅ Yes | ✅ Yes | SOC 2, GDPR |

---

## Decision Flowchart

### Question 1: Scale & Control

```
Need complete control over authentication?
├─ YES → Keycloak (self-hosted) ✅
│        (Full ownership, highest complexity)
│
├─ NO: Need managed service?
│  ├─ YES → Auth0 or Google ✅
│  │        (Managed, less operational burden)
│  │
│  └─ NO: Simple password auth?
│     └─ YES → SCRAM ✅
│              (Simplest, lowest overhead)
│
└─ Enterprise infrastructure?
   ├─ AWS → AWS Cognito ✅
   ├─ Azure → Azure AD ✅
   └─ Other → Use OIDC-compliant provider
```

### Question 2: User Base

```
Public internet users?
├─ YES → Google OAuth ✅
│        (Easiest, familiar to users)
│
├─ NO: Internal team only?
│  ├─ YES → Keycloak or SCRAM ✅
│  │        (Self-contained)
│  │
│  └─ NO: Enterprise customers?
│     └─ YES → Auth0 or Keycloak ✅
│              (SAML, provisioning, compliance)
│
└─ Social login needed?
   ├─ YES → Auth0 ✅
   │        (Multiple providers, easy setup)
   │
   └─ NO → Any provider fine
```

### Question 3: Features

```
Need MFA/2FA?
├─ YES → Auth0, Keycloak, Cognito ✅
│
Need SAML?
├─ YES → Keycloak or Auth0 ✅
│
Need directory sync (LDAP/AD)?
├─ YES → Keycloak or Azure AD ✅
│
Need fine-grained RBAC?
├─ YES → Auth0, Keycloak ✅
│
Need custom attributes?
├─ YES → Keycloak, Auth0, SCRAM ✅
│
Simple auth only?
└─ YES → Google, SCRAM ✅
```

---

## Detailed Recommendations

### Google OAuth (Best for Startups)

**Best for:**
- Public users (simplest)
- Startups wanting quick launch
- Teams with Google Workspace
- No complex enterprise needs

**Why it wins:**
- 🟢 Super easy setup (15 minutes)
- 🟢 Familiar to users
- 🟢 Free
- 🟢 Minimal operational overhead
- 🟡 Limited customization

**Setup Example:**
```bash
# 1. Create Google Cloud Project
# 2. Enable Google+ API
# 3. Create OAuth credentials
# 4. Copy to fraiseql.toml
GOOGLE_CLIENT_ID=xxxxx.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=xxxxx
```

**Time to production:** 30 minutes
**Cost:** Free
**Maintenance:** Minimal

**Limitations:**
- Can't customize login flow
- Relies on Google's infrastructure
- GDPR: Data in US
- No SAML support

---

### Auth0 (Best for Growing Companies)

**Best for:**
- Feature-rich authentication needs
- Enterprise customers
- Multiple social login providers
- Complex authorization requirements

**Why it wins:**
- ✅ Managed service (no ops)
- ✅ SAML + OAuth 2.0
- ✅ Social login (20+ providers)
- ✅ MFA/2FA, device flow
- ✅ Excellent documentation
- 🟡 More expensive ($0-$2,500/mo)

**Setup Example:**
```bash
# 1. Create Auth0 account
# 2. Create application
# 3. Copy settings
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_CLIENT_ID=xxxxx
AUTH0_CLIENT_SECRET=xxxxx
```

**Time to production:** 1-2 hours
**Cost:** Free tier available, $13-$2,500/mo for paid
**Maintenance:** Minimal

**Use when:**
- Building SaaS (most customers want SSO/SAML)
- Enterprise features important
- Want managed service reliability
- Multiple authentication methods needed

---

### Keycloak (Best for Enterprise)

**Best for:**
- Complete control needed
- Self-hosted infrastructure required
- SAML + OIDC both needed
- LDAP/Active Directory integration

**Why it wins:**
- ✅ Open source (full control)
- ✅ Self-hosted (data stays on-prem)
- ✅ SAML + OAuth 2.0 + OIDC
- ✅ LDAP/AD integration
- ✅ User federation
- 🔴 Complex setup (2-4 hours)

**Setup Example:**
```bash
# 1. Deploy Keycloak (Docker)
docker run -d \
  -e KEYCLOAK_ADMIN=admin \
  -e KEYCLOAK_ADMIN_PASSWORD=password \
  -p 8080:8080 \
  quay.io/keycloak/keycloak:latest

# 2. Create realm and client
# 3. Configure FraiseQL
KEYCLOAK_URL=http://keycloak:8080
KEYCLOAK_REALM=production
KEYCLOAK_CLIENT_ID=fraiseql
```

**Time to production:** 4-8 hours (includes setup)
**Cost:** Free (self-hosted) + infrastructure costs
**Maintenance:** Medium (monitoring, updates, backups)

**Use when:**
- Enterprise security requirements
- Need to integrate LDAP/AD
- Regulatory compliance (GDPR, HIPAA) critical
- Want to own authentication system

---

### SCRAM (Simplest Option)

**Best for:**
- Internal teams only
- Minimal authentication needs
- Just username/password (no social login)
- Maximum simplicity

**Why it wins:**
- 🟢 Simplest possible setup
- 🟢 No external dependencies
- 🟢 Direct database storage
- 🟢 No 3rd party involved
- 🟡 No advanced features

**Setup Example:**
```toml
# fraiseql.toml
[auth]
enabled = true
scheme = "scram"
database_url = "postgresql://..."
```

**Time to production:** 10 minutes
**Cost:** Free
**Maintenance:** None (uses your database)

**Use when:**
- Internal team (5-50 people)
- No social login needed
- Want complete simplicity
- No regulatory requirements

---

### AWS Cognito (Best for AWS Ecosystem)

**Best for:**
- Existing AWS infrastructure
- Pay-per-authentication billing
- Already using AWS services
- HIPAA compliance needed

**Pros:**
- ✅ Integrated with AWS ecosystem
- ✅ Integrated MFA
- ✅ Simple setup
- ✅ Pay-per-auth (low cost at startup)

**Cons:**
- ❌ Limited to AWS
- ❌ No SAML
- ❌ Vendor lock-in
- 🟡 Limited customization

**Use when:**
- Already committed to AWS
- Prefer pay-as-you-go
- No need for on-prem

---

### Azure AD (Best for Microsoft Ecosystem)

**Best for:**
- Existing Azure infrastructure
- Microsoft 365 integration
- Enterprise customers
- On-prem Active Directory

**Pros:**
- ✅ SAML + OAuth 2.0 + OIDC
- ✅ Direct AD/LDAP integration
- ✅ Enterprise support
- ✅ Compliance certifications

**Cons:**
- ❌ Limited to Azure
- ❌ Higher complexity
- ❌ Vendor lock-in

**Use when:**
- Enterprise Microsoft customer
- Have Active Directory
- SAML required

---

## Decision Table

| Use Case | Recommendation | Reason |
|----------|---|---|
| Startup, public users | Google OAuth | Fastest to market |
| SaaS platform | Auth0 | Enterprise features, managed |
| Internal team (10 people) | SCRAM | Simplest |
| Internal team (100+ people) | Keycloak | Scale + control |
| Enterprise customer base | Auth0 or Keycloak | SAML + features |
| AWS infrastructure | Cognito | Integration |
| Azure infrastructure | Azure AD | Integration |
| Regulated industry (HIPAA) | Keycloak or Cognito | Compliance + control |
| GDPR compliance critical | Keycloak | Data stays on-prem |
| Need LDAP/AD sync | Keycloak or Azure AD | Integration |

---

## Migration Scenarios

### Scenario: Migrate from Google OAuth to Auth0

**Effort:** Low (1-2 hours)
**Downtime:** 10-15 minutes

```bash
# 1. Create Auth0 account and application
# 2. Update fraiseql.toml with Auth0 credentials
# 3. Deploy update
# 4. Test: Existing sessions from Google remain valid

# Users who were authenticated by Google:
# - First login after migration must re-authenticate with Auth0
# - New Google account linking handled by Auth0

# Timeline:
# - During update: Brief outage (users log out)
# - After update: Users re-authenticate once
# - No data loss
```

### Scenario: Self-Host Keycloak for Enterprise

**Effort:** High (4-8 hours first time)
**Downtime:** 30 minutes (scheduled)

```bash
# 1. Deploy Keycloak in production
# 2. Set up database (PostgreSQL recommended)
# 3. Configure realm and clients
# 4. Test with staging environment
# 5. Sync existing users (if needed)
# 6. Update FraiseQL to use Keycloak
# 7. Gradual rollout (10% → 50% → 100%)

# Timeline:
# - Setup: 2-4 hours
# - Testing: 1-2 hours
# - Cutover: 1 hour
# - Monitoring: Ongoing
```

---

## Troubleshooting Provider Selection

### "We chose Google OAuth but need SAML"

**Options:**
1. Migrate to Auth0 or Keycloak (2-4 hours)
2. Accept OAuth-only limitation
3. Use Auth0 bridge (external service)

**Recommendation:** Migrate if SAML critical for customers

### "Keycloak is too complex to operate"

**Solutions:**
1. Use managed Keycloak service (Kloudless, etc.)
2. Simplify: Use only OAuth 2.0 subset
3. Migrate to Auth0 (managed alternative)

### "SCRAM doesn't have features we need"

**Options:**
1. Migrate to OAuth/OIDC provider (1-2 hours)
2. Implement features in application layer
3. Wait for future OAuth support

### "We're stuck with legacy provider (old system)"

**Bridge Pattern:**
```
Client → New OAuth Provider (Auth0, Keycloak)
           ↓
         Bridge Service
           ↓
         Legacy LDAP/AD/Custom
```

Use an OAuth provider as a bridge to your legacy system.

---

## See Also

- **[Google OAuth Setup](./SETUP-GOOGLE-OAUTH.md)** - Quick start
- **[Auth0 Setup](./SETUP-AUTH0.md)** - Quick start
- **[Keycloak Setup](./SETUP-KEYCLOAK.md)** - Quick start
- **[Security Checklist](./SECURITY-CHECKLIST.md)** - Pre-production verification
- **[API Reference](./API-REFERENCE.md)** - Complete API

---

**Remember:** OAuth provider choice is not permanent. Most migrations take just a few hours. Choose based on current needs and scale up as you grow.
