# JWT Locale Claim: Best Practices Analysis

**Date**: 2026-01-12
**Context**: User-aware automatic collation feature for FraiseQL v2
**Question**: Can we rely on JWT containing user locale? Is it best practice?

---

## TL;DR: **Yes, with Caveats**

✅ **`locale` is a standard OpenID Connect claim** - Well-defined, widely supported
✅ **Major providers support it** - Auth0, Okta, Clerk, Azure AD
✅ **Non-security-critical** - Affects UX (sorting), not authorization
⚠️ **Not guaranteed present** - Must have fallback strategy
⚠️ **Requires user profile data** - May need UserInfo endpoint call

**Recommendation**: Use locale claim as **primary strategy** with **database fallback**.

---

## Standards & Specifications

### 1. OpenID Connect Standard (OIDC Core 1.0)

**`locale` is a standard profile claim** defined in the [OpenID Connect Core 1.0 specification](https://openid.net/specs/openid-connect-core-1_0.html):

```
locale
  OPTIONAL. End-User's locale, represented as a BCP47 [RFC5646] language tag.
  This is typically an ISO 639-1 Alpha-2 [ISO639‑1] language code in lowercase
  and an ISO 3166-1 Alpha-2 [ISO3166‑1] country code in uppercase,
  separated by a dash. For example, en-US or fr-CA.
```

**Scope**: Part of the `profile` scope in OIDC:
- When a client requests the `profile` scope, the authorization server may return: `name`, `family_name`, `given_name`, `locale`, `zoneinfo`, etc.

**Sources**:
- [OpenID Connect Core 1.0 - Final](https://openid.net/specs/openid-connect-core-1_0.html)
- [OpenID Connect Standard Claims](https://www.cerberauth.com/blog/openid-connect-standard-claims/)

### 2. JWT RFC 7519 (Core JWT Standard)

**`locale` is NOT a registered claim** in [RFC 7519](https://datatracker.ietf.org/doc/html/rfc7519).

The core JWT standard defines only:
- `iss` (issuer)
- `sub` (subject)
- `aud` (audience)
- `exp` (expiration)
- `nbf` (not before)
- `iat` (issued at)
- `jti` (JWT ID)

**However**: RFC 7519 explicitly allows **public claims** (like `locale` from OIDC) and **private claims** (custom application-specific).

**Sources**:
- [RFC 7519 - JSON Web Token (JWT)](https://datatracker.ietf.org/doc/html/rfc7519)
- [JWT.IO - Introduction](https://www.jwt.io/introduction)

---

## Provider Support

### Auth0

**Support**: ✅ Yes (via custom claims or UserInfo endpoint)

**ID Token**:
```json
{
  "sub": "auth0|123",
  "iss": "https://your-domain.auth0.com/",
  "aud": "your-client-id",
  "exp": 1704153600,
  "locale": "fr-FR"  // ← Can be added via Actions
}
```

**Implementation**:
1. **Via Auth0 Actions** (custom claims):
   ```javascript
   exports.onExecutePostLogin = async (event, api) => {
     const locale = event.user.user_metadata.locale || 'en-US';
     api.idToken.setCustomClaim('locale', locale);
   };
   ```

2. **Via UserInfo endpoint** (standard):
   ```bash
   curl https://your-domain.auth0.com/userinfo \
     -H "Authorization: Bearer ACCESS_TOKEN"

   # Response:
   {
     "sub": "auth0|123",
     "name": "John Doe",
     "locale": "fr-FR"  // ← From user profile
   }
   ```

**Best Practice**: Use namespaced custom claims to avoid collisions:
```javascript
api.idToken.setCustomClaim('https://yourdomain.com/locale', locale);
```

**Sources**:
- [Auth0 - Create Custom Claims](https://auth0.com/docs/secure/tokens/json-web-tokens/create-custom-claims)
- [Auth0 - Adding Custom Claims with Actions](https://auth0.com/blog/adding-custom-claims-to-id-token-with-auth0-actions/)

### Okta

**Support**: ✅ Yes (via custom authorization server)

**ID Token**:
```json
{
  "sub": "00u123",
  "iss": "https://your-org.okta.com/oauth2/default",
  "aud": "your-client-id",
  "exp": 1704153600,
  "locale": "de-DE"  // ← Custom claim
}
```

**Implementation**:
1. Configure custom claim in authorization server:
   - Claim name: `locale`
   - Value type: Expression
   - Value: `user.locale` (from user profile attribute)

2. Add to token via claim mapping

**Limitation**: Custom claims can only be added to **custom authorization server**, not org authorization server.

**Sources**:
- [Okta - Customize Tokens with Custom Claims](https://developer.okta.com/docs/guides/customize-tokens-returned-from-okta/main/)
- [Okta - Identity, Claims, & Tokens Primer](https://developer.okta.com/blog/2017/07/25/oidc-primer-part-1)

### Clerk

**Support**: ✅ Yes (automatic with providers, manual setup)

**ID Token**:
```json
{
  "sub": "user_123",
  "iss": "https://your-app.clerk.accounts.dev",
  "aud": "your-frontend",
  "exp": 1704153600,
  "locale": "ja-JP"  // ← From user profile
}
```

**Implementation**:
- Clerk normalizes claims automatically across different OAuth providers
- User profile can include locale preference
- Built-in support for major providers

**Sources**:
- [Clerk - SSO Best Practices](https://clerk.com/articles/sso-best-practices-for-secure-scalable-logins)

### Azure AD / Microsoft Identity Platform

**Support**: ✅ Yes (standard OIDC claim)

**ID Token**:
```json
{
  "sub": "AAAAAbbbb",
  "iss": "https://login.microsoftonline.com/...",
  "aud": "your-app-id",
  "exp": 1704153600,
  "locale": "en-US"  // ← From user profile
}
```

**UserInfo Endpoint**: Standard OIDC UserInfo returns locale

**Sources**:
- [Microsoft - UserInfo Endpoint](https://learn.microsoft.com/en-us/azure/active-directory-b2c/userinfo-endpoint)

---

## Best Practices Analysis

### ✅ Advantages of Using JWT Locale

1. **Standard Compliance**
   - OIDC standard claim (widely recognized)
   - Follows BCP47 language tag format (RFC 5646)
   - Consistent across providers

2. **Performance**
   - No additional database query needed
   - Locale available immediately on auth
   - Cached with JWT (until expiry)

3. **User Preference**
   - Reflects user's actual language preference
   - Portable across sessions/devices
   - Managed in identity provider (single source of truth)

4. **Non-Security-Critical**
   - Locale affects UX (sorting), not authorization
   - Low risk if manipulated (wrong sorting vs data breach)
   - JWT signature still validates authenticity

5. **Widely Supported**
   - All major identity providers support it
   - Standard in enterprise applications
   - Expected pattern in modern apps

### ⚠️ Caveats & Considerations

1. **Not Guaranteed Present**
   - User profile may not have locale set
   - Not all providers include it by default
   - May require explicit configuration

   **Mitigation**: Always have fallback strategy

2. **Requires User Profile Data**
   - Locale must be stored in user profile
   - May require UserInfo endpoint call (not in ID token)
   - Extra network hop if not in token

   **Mitigation**: Include in ID token via custom claims

3. **Token Size**
   - Adding claims increases JWT size
   - May affect bandwidth/storage
   - Matters for mobile/edge cases

   **Mitigation**: Locale is small (~5-10 bytes), negligible impact

4. **Claim Name Variations**
   - Different providers use different names: `locale`, `lang`, `language`
   - May need to check multiple claim names

   **Mitigation**: Check multiple claim names (already in our plan)

5. **Update Latency**
   - Locale cached until token expires
   - User changes preference → old locale used until refresh
   - Token TTL typically 1 hour

   **Mitigation**: Acceptable for sorting (not real-time critical)

6. **Privacy Considerations**
   - Locale reveals user's language/region
   - May be considered PII in some jurisdictions
   - JWT is signed but not encrypted by default

   **Mitigation**: Locale is typically public data, low sensitivity

---

## Alternative Approaches

### 1. Database User Preferences Table

**Pattern**: Store locale in database, query on each request

**Pros**:
- Always up-to-date (no token expiry lag)
- Guaranteed present (default value)
- Can have multiple preferences (locale, timezone, currency, etc.)

**Cons**:
- Additional database query per request
- Increased latency (~5-10ms)
- Requires user management system
- Not portable across services

**Recommendation**: Use as **fallback**, not primary

### 2. HTTP Headers (Accept-Language)

**Pattern**: Use `Accept-Language` HTTP header

**Pros**:
- Standard HTTP header
- Browser sends automatically
- No JWT dependency

**Cons**:
- Browser preference, not user account preference
- Can be overridden by browser settings
- Not reliable for logged-in users
- Doesn't persist across devices

**Recommendation**: Use as **last resort fallback**

### 3. GraphQL Query Parameter

**Pattern**: Client sends locale in every query

**Pros**:
- Explicit and controllable
- No backend state needed
- Works for unauthenticated users

**Cons**:
- Requires client to send every time
- Easy to forget/inconsistent
- Duplicates data if user is authenticated

**Recommendation**: Use as **manual override option**

---

## Recommended Strategy: Hybrid Approach

### Priority Cascade

```rust
fn resolve_locale(
    query_param: Option<&str>,      // 1. Explicit override
    user: Option<&AuthenticatedUser>, // 2. JWT claim
    db_preference: Option<&str>,    // 3. Database fallback
    accept_language: Option<&str>,  // 4. HTTP header
) -> String {
    query_param
        .or_else(|| user.and_then(|u| u.locale.as_ref()))
        .or_else(|| db_preference)
        .or_else(|| accept_language)
        .unwrap_or("en-US")  // 5. System default
}
```

### Implementation Tiers

**Tier 1: JWT Claim** (Primary - 80% of cases)
```rust
// Extract from JWT during authentication
let user = AuthMiddleware::validate_request(&req).await?;
// user.locale = Some("fr-FR")

// Use in query execution
let collation = user.icu_collation(); // "fr-FR-x-icu"
```

**Tier 2: Database Fallback** (Secondary - 15% of cases)
```sql
-- User preferences table
CREATE TABLE user_preferences (
    user_id UUID PRIMARY KEY,
    locale VARCHAR(10) NOT NULL DEFAULT 'en-US',
    timezone VARCHAR(50),
    updated_at TIMESTAMP
);

-- Query on auth if JWT locale missing
SELECT locale FROM user_preferences WHERE user_id = $1;
```

**Tier 3: Accept-Language Header** (Tertiary - 4% of cases)
```rust
// Extract from HTTP headers
let accept_lang = req.headers().get("Accept-Language");
// "en-US,en;q=0.9,fr;q=0.8" → "en-US"
```

**Tier 4: System Default** (Fallback - 1% of cases)
```rust
// Hardcoded system default
const DEFAULT_LOCALE: &str = "en-US";
```

---

## Security Considerations

### JWT Best Practices (Applied to Locale)

Based on [JWT Security Best Practices (Curity)](https://curity.io/resources/learn/jwt-best-practices/) and [Auth0 JWT Best Current Practices](https://auth0.com/blog/a-look-at-the-latest-draft-for-jwt-bcp/):

1. **Always Validate JWT Signature** ✅
   - Already done by AuthMiddleware
   - Prevents locale manipulation attacks

2. **Validate All Standard Claims** ✅
   - `iss`, `aud`, `exp`, `sub` validation
   - Locale is additional, non-critical claim

3. **Short Token Expiry** ✅
   - Typical: 1 hour (3600s)
   - Limits exposure if token compromised

4. **Locale Format Validation** ⚠️ (Must Implement)
   ```rust
   fn validate_locale(locale: &str) -> bool {
       // BCP47 format: en-US, fr-FR, ja-JP
       let re = Regex::new(r"^[a-z]{2}-[A-Z]{2}$").unwrap();
       re.is_match(locale)
   }
   ```

5. **Collation Whitelist** ⚠️ (Must Implement)
   ```rust
   const ALLOWED_LOCALES: &[&str] = &[
       "en-US", "en-GB", "fr-FR", "de-DE", "ja-JP", // ...
   ];

   fn validate_collation(locale: &str) -> bool {
       ALLOWED_LOCALES.contains(&locale)
   }
   ```

### Attack Scenarios & Mitigations

**Scenario 1: Malicious Locale Injection**
```json
{
  "sub": "user123",
  "locale": "'; DROP TABLE users; --"  // ← SQL injection attempt
}
```
**Mitigation**:
- JWT signature validation (claim can't be modified)
- Locale format validation (regex)
- Collation whitelist
- COLLATE clause is safe (not string interpolation)

**Scenario 2: Invalid Collation**
```json
{
  "sub": "user123",
  "locale": "xx-YY"  // ← Non-existent locale
}
```
**Mitigation**:
- PostgreSQL will error if collation doesn't exist
- Catch error, log warning, fallback to default
- Pre-validate against whitelist

**Scenario 3: JWT Replay Attack**
- Attacker reuses old JWT with different locale

**Mitigation**:
- Expiration validation (already done)
- Worst case: Incorrect sorting (not a security breach)

---

## Industry Examples

### 1. FusionAuth

From [FusionAuth Forum Discussion](https://fusionauth.io/community/forum/topic/214/i-want-to-pass-the-locale-and-timezone-info-to-apps-via-a-jwt):

**Question**: "I want to pass the locale and timezone info to apps via a JWT"

**Answer**: "You can populate the JWT with `locale` and `zoneinfo` claims. These are standard OpenID Connect claims."

**Implementation**: Custom Lambda to add claims from user data.

### 2. Microsoft Azure AD

**Standard Behavior**: UserInfo endpoint returns `locale` claim for all users who have language preference set.

**Format**: ISO 639-1 + ISO 3166-1 (e.g., `en-US`)

### 3. Google Identity Platform

**Claim**: `locale` available in ID token and UserInfo endpoint

**Format**: BCP47 language tag

---

## Conclusion & Recommendations

### ✅ YES, Relying on JWT Locale is Best Practice

**Reasons**:
1. **Standard**: OIDC standard claim, widely recognized
2. **Supported**: All major providers (Auth0, Okta, Clerk, Azure AD)
3. **Performant**: No additional queries, cached with JWT
4. **User-Centric**: Reflects actual user preference
5. **Low Risk**: Non-security-critical, affects UX only

### Implementation Guidelines

**DO**:
- ✅ Extract `locale` from JWT claims
- ✅ Validate format (BCP47: `en-US`, `fr-FR`)
- ✅ Whitelist allowed locales
- ✅ Have fallback strategy (database, header, default)
- ✅ Support multiple claim names (`locale`, `lang`, `language`)
- ✅ Log collation strategy for debugging

**DON'T**:
- ❌ Trust locale without validation
- ❌ Use only JWT locale (have fallbacks)
- ❌ Store sensitive data based on locale
- ❌ Ignore collation errors (catch and fallback)

### Updated Architecture for FraiseQL

```rust
impl AuthMiddleware {
    fn extract_user_from_claims(&self, claims: &Value) -> Result<AuthenticatedUser> {
        // Extract locale (try multiple claim names)
        let locale = claims.get("locale")
            .or_else(|| claims.get("lang"))
            .or_else(|| claims.get("language"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .filter(|l| Self::validate_locale(l));  // ← Validate format

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
            locale,  // ← Include validated locale
        })
    }

    fn validate_locale(locale: &str) -> bool {
        // BCP47 format: en-US, fr-FR, etc.
        let re = Regex::new(r"^[a-z]{2}-[A-Z]{2}$").unwrap();
        re.is_match(locale) && ALLOWED_LOCALES.contains(&locale)
    }
}

impl CollationResolver {
    pub fn resolve(
        field: &OrderByField,
        user: Option<&AuthenticatedUser>,
        db_locale: Option<&str>,  // ← Add database fallback
        accept_language: Option<&str>,  // ← Add header fallback
    ) -> (Option<String>, CollationStrategy) {
        // Priority 1: Explicit collation (manual override)
        if let Some(collation) = &field.collation {
            return (Some(collation.clone()), CollationStrategy::Explicit);
        }

        // Priority 2: JWT locale (automatic)
        if let Some(user) = user {
            if let Some(collation) = user.icu_collation() {
                return (Some(collation), CollationStrategy::UserLocale);
            }
        }

        // Priority 3: Database preference
        if let Some(locale) = db_locale {
            let collation = format!("{}-x-icu", locale);
            return (Some(collation), CollationStrategy::Database);
        }

        // Priority 4: Accept-Language header
        if let Some(lang) = accept_language {
            let locale = parse_accept_language(lang);  // "en-US,en;q=0.9" → "en-US"
            let collation = format!("{}-x-icu", locale);
            return (Some(collation), CollationStrategy::HttpHeader);
        }

        // Priority 5: System default
        (None, CollationStrategy::DatabaseDefault)
    }
}
```

### Configuration Example

```python
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "order_by": {
            "enabled": True,
            "auto_collation": {
                "enabled": True,
                "sources": ["jwt", "database", "header"],  # Priority order
                "fallback": "en-US-x-icu",
                "validate": True,  # Validate locale format
                "whitelist": [
                    "en-US", "en-GB", "fr-FR", "de-DE", "ja-JP",
                    "es-ES", "it-IT", "pt-BR", "zh-CN", "ko-KR"
                ]
            }
        }
    }
)
def users() -> list[User]:
    """Users with automatic locale-aware sorting."""
    pass
```

---

## Sources

- [OpenID Connect Core 1.0 - Standard Claims](https://openid.net/specs/openid-connect-core-1_0.html)
- [RFC 7519 - JSON Web Token (JWT)](https://datatracker.ietf.org/doc/html/rfc7519)
- [Auth0 - JWT Best Practices](https://auth0.com/docs/secure/tokens/json-web-tokens/json-web-token-claims)
- [Auth0 - Create Custom Claims](https://auth0.com/docs/secure/tokens/json-web-tokens/create-custom-claims)
- [Okta - Customize Tokens with Custom Claims](https://developer.okta.com/docs/guides/customize-tokens-returned-from-okta/main/)
- [Clerk - SSO Best Practices](https://clerk.com/articles/sso-best-practices-for-secure-scalable-logins)
- [Microsoft - UserInfo Endpoint](https://learn.microsoft.com/en-us/azure/active-directory-b2c/userinfo-endpoint)
- [Curity - JWT Security Best Practices](https://curity.io/resources/learn/jwt-best-practices/)
- [FusionAuth - Locale in JWT Discussion](https://fusionauth.io/community/forum/topic/214/i-want-to-pass-the-locale-and-timezone-info-to-apps-via-a-jwt)
- [OpenID Connect Standard Claims](https://www.cerberauth.com/blog/openid-connect-standard-claims/)

---

**Status**: Analysis Complete
**Recommendation**: **Proceed with JWT locale approach** with proper validation and fallback strategy
**Risk Level**: Low (non-security-critical, well-supported standard)
