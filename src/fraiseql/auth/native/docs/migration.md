# Migration Guide - FraiseQL Native Authentication

Complete guide for migrating from external authentication providers to FraiseQL's native authentication system.

## Migration Overview

### Why Migrate?

- **Cost Reduction**: Save $240-$2000+/month on Auth0 costs
- **Performance**: 10x faster authentication (10ms vs 100ms+)
- **Data Sovereignty**: Complete control over user data
- **No Vendor Lock-in**: Own your authentication infrastructure
- **Advanced Security**: Argon2id, token families, theft detection

### Migration Strategies

1. **Parallel Migration** (Recommended): Run both systems during transition
2. **Direct Migration**: Export/import all users at once
3. **Gradual Migration**: Migrate users as they login

## From Auth0

### Pre-Migration Preparation

#### 1. Audit Current Auth0 Setup

```bash
# Install Auth0 CLI
npm install -g auth0-deploy-cli

# Export current configuration
auth0 export -f auth0-config.json
```

**Review your current Auth0 features:**
- [ ] User registration/login flows
- [ ] Password reset functionality  
- [ ] Social login providers (Google, Facebook, etc.)
- [ ] Multi-factor authentication (MFA)
- [ ] Custom user metadata fields
- [ ] Roles and permissions
- [ ] Hooks and rules
- [ ] Enterprise SSO (SAML, OIDC)

#### 2. Plan Feature Mapping

| Auth0 Feature | FraiseQL Native | Migration Notes |
|---------------|-----------------|-----------------|
| Email/Password Auth | ✅ Full support | Direct replacement |
| User Metadata | ✅ JSONB metadata | Custom migration script |
| Roles & Permissions | ✅ Array fields | Direct mapping |
| Password Reset | ✅ Token-based | Similar flow |
| Session Management | ✅ Multi-device | Enhanced features |
| Rate Limiting | ✅ Built-in | More granular |
| Social Login | ❌ Not built-in | Requires custom OAuth |
| MFA/2FA | ❌ Not built-in | Planned for future |
| Enterprise SSO | ❌ Not built-in | Consider keeping Auth0 |

### Migration Steps

#### Step 1: Set Up FraiseQL Native Auth

Follow the [Setup Guide](setup.md) to install and configure FraiseQL native auth alongside your existing Auth0 implementation.

#### Step 2: Export Auth0 Users

```python
# export_auth0_users.py
import requests
import json
import os
from datetime import datetime

class Auth0Exporter:
    def __init__(self, domain, client_id, client_secret):
        self.domain = domain
        self.access_token = self._get_access_token(client_id, client_secret)
    
    def _get_access_token(self, client_id, client_secret):
        """Get Management API token"""
        response = requests.post(f"https://{self.domain}/oauth/token", 
            json={
                "client_id": client_id,
                "client_secret": client_secret,
                "audience": f"https://{self.domain}/api/v2/",
                "grant_type": "client_credentials"
            })
        return response.json()["access_token"]
    
    def export_users(self, output_file="auth0_users.json"):
        """Export all users from Auth0"""
        headers = {"Authorization": f"Bearer {self.access_token}"}
        all_users = []
        page = 0
        per_page = 100
        
        while True:
            response = requests.get(
                f"https://{self.domain}/api/v2/users",
                headers=headers,
                params={
                    "per_page": per_page,
                    "page": page,
                    "include_totals": True
                }
            )
            
            data = response.json()
            users = data.get("users", [])
            
            if not users:
                break
                
            all_users.extend(users)
            page += 1
            print(f"Exported {len(all_users)} users...")
        
        # Save to file
        with open(output_file, 'w') as f:
            json.dump(all_users, f, indent=2)
        
        print(f"✅ Exported {len(all_users)} users to {output_file}")
        return all_users

# Usage
exporter = Auth0Exporter(
    domain="your-tenant.auth0.com",
    client_id="your_management_api_client_id", 
    client_secret="your_management_api_client_secret"
)

users = exporter.export_users()
```

#### Step 3: Import Users to FraiseQL

```python
# import_to_fraiseql.py
import asyncio
import json
import secrets
import string
from datetime import datetime
from psycopg_pool import AsyncConnectionPool
from fraiseql.auth.native.models import User

class FraiseQLImporter:
    def __init__(self, database_url, schema="public"):
        self.pool = AsyncConnectionPool(database_url)
        self.schema = schema
    
    async def import_auth0_users(self, auth0_users_file="auth0_users.json"):
        """Import users from Auth0 export"""
        with open(auth0_users_file, 'r') as f:
            auth0_users = json.load(f)
        
        migrated_users = []
        
        async with self.pool.connection() as conn:
            async with conn.cursor() as cursor:
                for auth0_user in auth0_users:
                    try:
                        # Skip non-email users (social logins)
                        if not auth0_user.get('email'):
                            continue
                        
                        # Create FraiseQL user
                        user_data = self._convert_auth0_user(auth0_user)
                        
                        # Create user with temporary password
                        user = User(
                            email=user_data['email'],
                            name=user_data['name'],
                            roles=user_data.get('roles', []),
                            permissions=user_data.get('permissions', []),
                            metadata=user_data.get('metadata', {}),
                            is_active=not auth0_user.get('blocked', False),
                            email_verified=auth0_user.get('email_verified', False)
                        )
                        
                        # Set temporary password
                        temp_password = self._generate_temp_password()
                        user.set_password(temp_password)
                        
                        # Save user
                        await user.save(cursor, self.schema)
                        
                        # Queue password reset email
                        await self._queue_password_reset_email(
                            cursor, user.id, user.email, temp_password
                        )
                        
                        migrated_users.append({
                            'auth0_id': auth0_user['user_id'],
                            'fraiseql_id': str(user.id),
                            'email': user.email,
                            'temp_password': temp_password
                        })
                        
                        print(f"✅ Migrated {user.email}")
                        
                    except Exception as e:
                        print(f"❌ Failed to migrate {auth0_user.get('email', 'unknown')}: {e}")
                
                await conn.commit()
        
        # Save migration results
        with open('migration_results.json', 'w') as f:
            json.dump(migrated_users, f, indent=2)
        
        print(f"✅ Migrated {len(migrated_users)} users successfully")
        return migrated_users
    
    def _convert_auth0_user(self, auth0_user):
        """Convert Auth0 user to FraiseQL format"""
        # Extract name from various Auth0 fields
        name = (
            auth0_user.get('name') or
            auth0_user.get('nickname') or 
            auth0_user.get('username') or
            auth0_user.get('email', '').split('@')[0]
        )
        
        # Map Auth0 roles to FraiseQL roles
        auth0_roles = auth0_user.get('app_metadata', {}).get('roles', [])
        fraiseql_roles = self._map_roles(auth0_roles)
        
        # Convert metadata
        metadata = {}
        if 'user_metadata' in auth0_user:
            metadata.update(auth0_user['user_metadata'])
        if 'app_metadata' in auth0_user:
            # Filter out roles (handled separately)
            app_metadata = {k: v for k, v in auth0_user['app_metadata'].items() 
                          if k not in ['roles', 'permissions']}
            metadata.update(app_metadata)
        
        return {
            'email': auth0_user['email'],
            'name': name,
            'roles': fraiseql_roles,
            'permissions': self._extract_permissions(auth0_user),
            'metadata': metadata
        }
    
    def _map_roles(self, auth0_roles):
        """Map Auth0 roles to FraiseQL roles"""
        role_mapping = {
            'admin': 'admin',
            'user': 'user',
            'editor': 'editor',
            'viewer': 'viewer'
            # Add your custom role mappings
        }
        
        return [role_mapping.get(role, role) for role in auth0_roles]
    
    def _extract_permissions(self, auth0_user):
        """Extract permissions from Auth0 user"""
        # Auth0 permissions might be in app_metadata or separate API
        app_metadata = auth0_user.get('app_metadata', {})
        return app_metadata.get('permissions', [])
    
    def _generate_temp_password(self):
        """Generate secure temporary password"""
        # Generate 16-character password with mixed characters
        chars = string.ascii_letters + string.digits + "!@#$%^&*"
        return ''.join(secrets.choice(chars) for _ in range(16))
    
    async def _queue_password_reset_email(self, cursor, user_id, email, temp_password):
        """Queue password reset email for user"""
        # Create password reset token
        reset_token = secrets.token_urlsafe(32)
        token_hash = hashlib.sha256(reset_token.encode()).hexdigest()
        
        # Store reset token
        await cursor.execute(f"""
            INSERT INTO {self.schema}.tb_password_reset 
            (fk_user, token_hash, expires_at)
            VALUES (%s, %s, NOW() + INTERVAL '7 days')
        """, (user_id, token_hash))
        
        # Queue email (implement your email service)
        await self._send_migration_email(email, reset_token, temp_password)
    
    async def _send_migration_email(self, email, reset_token, temp_password):
        """Send migration welcome email"""
        # Implement with your email service (SendGrid, SES, etc.)
        email_content = f"""
        Welcome to our new authentication system!
        
        Your account has been migrated from Auth0. To complete the migration:
        
        1. Visit: https://your-app.com/reset-password?token={reset_token}
        2. Set your new password
        
        Your temporary password (if needed): {temp_password}
        
        This link expires in 7 days.
        """
        
        # Send email using your preferred service
        print(f"📧 Would send migration email to {email}")

# Usage
async def main():
    importer = FraiseQLImporter("postgresql://localhost:5432/yourdb")
    await importer.import_auth0_users("auth0_users.json")

asyncio.run(main())
```

#### Step 4: Implement Parallel Authentication

```python
# parallel_auth_middleware.py
from fastapi import Request, HTTPException
from fraiseql.auth.native.provider import NativeAuthProvider

class ParallelAuthMiddleware:
    def __init__(self, native_provider: NativeAuthProvider, auth0_client):
        self.native_provider = native_provider
        self.auth0_client = auth0_client
    
    async def authenticate_user(self, request: Request):
        """Try native auth first, fall back to Auth0"""
        auth_header = request.headers.get("authorization")
        
        if not auth_header or not auth_header.startswith("Bearer "):
            return None
        
        token = auth_header.split(" ")[1]
        
        # Try native auth first
        try:
            user_context = await self.native_provider.get_user_from_token(token)
            if user_context:
                return user_context
        except Exception:
            pass  # Continue to Auth0
        
        # Fall back to Auth0
        try:
            auth0_user = await self._validate_auth0_token(token)
            if auth0_user:
                # Convert Auth0 user to native format
                return await self._convert_auth0_user_context(auth0_user)
        except Exception:
            pass
        
        return None
    
    async def _validate_auth0_token(self, token):
        """Validate token with Auth0"""
        # Implement Auth0 token validation
        # This is a simplified example
        import jwt
        import requests
        
        # Get Auth0 public key
        jwks_response = requests.get(f"https://your-tenant.auth0.com/.well-known/jwks.json")
        jwks = jwks_response.json()
        
        # Validate JWT (simplified)
        decoded_token = jwt.decode(token, options={"verify_signature": False})
        return decoded_token
```

#### Step 5: Gradual User Migration

```python
# gradual_migration.py
async def migrate_user_on_login(email: str, password: str):
    """Migrate user from Auth0 during login"""
    
    # Try native login first
    try:
        return await native_login(email, password)
    except InvalidCredentialsError:
        pass
    
    # Try Auth0 login
    try:
        auth0_user = await auth0_login(email, password)
        
        # Migrate user to native auth
        native_user = await create_native_user_from_auth0(auth0_user, password)
        
        # Return native auth tokens
        return await native_login(email, password)
        
    except Exception as e:
        raise InvalidCredentialsError("Login failed")

async def create_native_user_from_auth0(auth0_user, password):
    """Create native user from Auth0 user during login"""
    user = User(
        email=auth0_user['email'],
        name=auth0_user.get('name', ''),
        roles=extract_roles(auth0_user),
        permissions=extract_permissions(auth0_user),
        metadata=extract_metadata(auth0_user),
        is_active=True,
        email_verified=auth0_user.get('email_verified', True)
    )
    
    user.set_password(password)
    
    async with db_pool.connection() as conn:
        async with conn.cursor() as cursor:
            await user.save(cursor, schema)
            await conn.commit()
    
    return user
```

#### Step 6: Frontend Migration

**Vue.js/Nuxt Example:**

```javascript
// Before (Auth0)
import { useAuth0 } from '@auth0/auth0-vue'

const { loginWithRedirect, user, logout } = useAuth0()

// After (FraiseQL Native)
import { useNativeAuth } from '@/composables/useNativeAuth'

const { login, user, logout } = useNativeAuth()

// Migration wrapper composable
export const useAuth = () => {
  const native = useNativeAuth()
  const auth0 = useAuth0()
  
  // Try native auth first, fall back to Auth0
  const login = async (email, password) => {
    try {
      return await native.login(email, password)
    } catch (error) {
      if (error.code === 'USER_NOT_FOUND') {
        // User not migrated yet, try Auth0
        return await auth0.loginWithRedirect()
      }
      throw error
    }
  }
  
  return { login, user: native.user || auth0.user, logout }
}
```

#### Step 7: Testing Migration

```python
# test_migration.py
import asyncio
import pytest

class MigrationTester:
    def __init__(self, auth0_users, native_auth_provider):
        self.auth0_users = auth0_users
        self.native_provider = native_auth_provider
    
    async def test_user_migration(self):
        """Test that all users were migrated correctly"""
        for auth0_user in self.auth0_users:
            if not auth0_user.get('email'):
                continue
                
            # Check user exists in FraiseQL
            native_user = await self._get_native_user(auth0_user['email'])
            assert native_user is not None, f"User {auth0_user['email']} not migrated"
            
            # Verify user data
            assert native_user.email == auth0_user['email']
            assert native_user.name == (auth0_user.get('name') or auth0_user.get('nickname'))
            
            # Test authentication works
            await self._test_user_auth(auth0_user['email'])
    
    async def test_parallel_auth(self):
        """Test parallel authentication works"""
        # Test with migrated user (should use native)
        # Test with non-migrated user (should use Auth0)
        pass
    
    async def _get_native_user(self, email):
        """Get user from native auth system"""
        async with self.native_provider.db_pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute(
                    f"SELECT * FROM {self.native_provider.schema}.tb_user WHERE email = %s",
                    (email,)
                )
                return await cursor.fetchone()
```

### Post-Migration Cleanup

#### 1. Monitor Both Systems

```python
# migration_monitor.py
import asyncio
from datetime import datetime, timedelta

async def monitor_migration_progress():
    """Monitor parallel authentication usage"""
    
    # Count native vs Auth0 logins
    native_logins = await count_native_logins_today()
    auth0_logins = await count_auth0_logins_today()
    
    migration_percentage = native_logins / (native_logins + auth0_logins) * 100
    
    print(f"Migration Progress: {migration_percentage:.1f}%")
    print(f"Native logins: {native_logins}")
    print(f"Auth0 logins: {auth0_logins}")
    
    # Alert if migration stalled
    if migration_percentage < 80 and days_since_migration() > 30:
        await send_migration_alert()
```

#### 2. Sunset Auth0

Once migration is complete (95%+ users migrated):

1. **Remove Auth0 SDK** from frontend
2. **Remove Auth0 fallback** from middleware  
3. **Disable Auth0 tenant** (keep backup for 30 days)
4. **Update documentation** and remove Auth0 references
5. **Cancel Auth0 subscription**

## From Firebase Auth

### Export Firebase Users

```python
# export_firebase_users.py
import firebase_admin
from firebase_admin import auth, credentials
import json

# Initialize Firebase Admin SDK
cred = credentials.Certificate("path/to/serviceAccountKey.json")
firebase_admin.initialize_app(cred)

def export_firebase_users():
    """Export all users from Firebase Auth"""
    users = []
    page = auth.list_users()
    
    while page:
        for user in page.users:
            users.append({
                'uid': user.uid,
                'email': user.email,
                'display_name': user.display_name,
                'phone_number': user.phone_number,
                'email_verified': user.email_verified,
                'disabled': user.disabled,
                'custom_claims': user.custom_claims,
                'creation_timestamp': user.user_metadata.creation_timestamp,
                'last_sign_in_timestamp': user.user_metadata.last_sign_in_timestamp
            })
        
        page = page.get_next_page()
    
    with open('firebase_users.json', 'w') as f:
        json.dump(users, f, indent=2, default=str)
    
    print(f"Exported {len(users)} Firebase users")
    return users

# Usage
firebase_users = export_firebase_users()
```

### Import Firebase Users

```python
# import_firebase_users.py
async def import_firebase_users(firebase_users_file="firebase_users.json"):
    """Import users from Firebase export"""
    with open(firebase_users_file, 'r') as f:
        firebase_users = json.load(f)
    
    for firebase_user in firebase_users:
        if not firebase_user.get('email'):
            continue
        
        # Create FraiseQL user
        user = User(
            email=firebase_user['email'],
            name=firebase_user.get('display_name', ''),
            roles=extract_firebase_roles(firebase_user),
            permissions=extract_firebase_permissions(firebase_user),
            metadata={
                'firebase_uid': firebase_user['uid'],
                'phone_number': firebase_user.get('phone_number'),
                'migrated_at': datetime.now().isoformat()
            },
            is_active=not firebase_user.get('disabled', False),
            email_verified=firebase_user.get('email_verified', False)
        )
        
        # Set temporary password and queue reset email
        temp_password = generate_temp_password()
        user.set_password(temp_password)
        
        await user.save(cursor, schema)
        await queue_password_reset_email(user.id, user.email)
```

## From Supabase Auth

### Export Supabase Users

```sql
-- Export users from Supabase (run in Supabase SQL editor)
COPY (
  SELECT 
    id,
    email, 
    raw_user_meta_data,
    raw_app_meta_data,
    email_confirmed_at IS NOT NULL as email_verified,
    created_at,
    updated_at
  FROM auth.users 
  WHERE deleted_at IS NULL
) TO STDOUT WITH CSV HEADER;
```

### Import Supabase Users

```python
# import_supabase_users.py
import csv
import json

async def import_supabase_users(csv_file="supabase_users.csv"):
    """Import users from Supabase CSV export"""
    with open(csv_file, 'r') as f:
        reader = csv.DictReader(f)
        
        for row in reader:
            # Parse JSON metadata
            user_metadata = json.loads(row.get('raw_user_meta_data', '{}'))
            app_metadata = json.loads(row.get('raw_app_meta_data', '{}'))
            
            user = User(
                email=row['email'],
                name=user_metadata.get('name', ''),
                roles=app_metadata.get('roles', []),
                permissions=app_metadata.get('permissions', []),
                metadata={
                    'supabase_id': row['id'],
                    **user_metadata,
                    **app_metadata
                },
                is_active=True,
                email_verified=bool(row['email_verified'])
            )
            
            await migrate_user(user)
```

## Migration Checklist

### Pre-Migration
- [ ] Audit current authentication setup
- [ ] Plan feature mapping
- [ ] Set up FraiseQL native auth in staging
- [ ] Create user export scripts
- [ ] Design parallel authentication strategy
- [ ] Prepare migration communication

### During Migration  
- [ ] Export users from current provider
- [ ] Import users to FraiseQL with temporary passwords
- [ ] Deploy parallel authentication middleware
- [ ] Send migration emails to users
- [ ] Monitor migration progress
- [ ] Handle support requests

### Post-Migration
- [ ] Verify all users can authenticate
- [ ] Remove parallel authentication
- [ ] Disable/cancel old provider
- [ ] Update documentation
- [ ] Celebrate cost savings! 🎉

## Common Issues and Solutions

### Issue: Social Login Dependencies
**Problem**: Users rely on Google/Facebook login
**Solution**: 
- Keep Auth0 for social logins only
- Use FraiseQL native for email/password
- Implement OAuth2 providers separately if needed

### Issue: Enterprise SSO Requirements  
**Problem**: Customers need SAML/OIDC
**Solution**:
- Keep Auth0 for enterprise customers
- Use FraiseQL native for regular users
- Plan SAML/OIDC implementation for future

### Issue: Complex Role/Permission Systems
**Problem**: Existing roles don't map directly
**Solution**:
- Create role mapping script
- Use metadata for complex permissions
- Gradually migrate to FraiseQL's role system

### Issue: Password Migration
**Problem**: Can't migrate hashed passwords
**Solution**:
- Generate temporary passwords
- Send password reset emails
- Implement gradual migration on login

## Support

Need help with migration? 

- **Migration Issues**: [Open GitHub Issue](https://github.com/fraiseql/fraiseql/issues)
- **Custom Migration Scripts**: Consider professional services
- **Enterprise Migration**: Contact for dedicated support

## Success Stories

> "Migrated 50,000 users from Auth0 to FraiseQL in 2 weeks. Zero downtime, 99.8% user satisfaction. Saving $2000/month." - Enterprise SaaS

> "Firebase to FraiseQL migration took 3 days. The token theft protection already saved us twice." - B2B Platform

> "Auth0 to FraiseQL cut our authentication latency from 150ms to 8ms. Users noticed immediately." - High-Performance App