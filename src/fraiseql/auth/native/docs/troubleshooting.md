# Troubleshooting Guide - FraiseQL Native Authentication

Common issues, solutions, and debugging techniques for FraiseQL native authentication.

## Quick Diagnostic Commands

### Check System Status

```bash
# Verify FraiseQL installation
python -c "from fraiseql.auth.native import create_native_auth_provider; print('✅ FraiseQL native auth installed')"

# Check database connectivity
psql $DATABASE_URL -c "SELECT version();"

# Test JWT secret
python -c "import os; print('✅ JWT secret loaded' if os.environ.get('JWT_SECRET_KEY') else '❌ JWT secret missing')"

# Verify required tables exist
psql $DATABASE_URL -c "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename LIKE 'tb_%';"
```

### Health Check

```bash
# Test health endpoint
curl -f http://localhost:8000/health

# Test authentication flow
curl -X POST http://localhost:8000/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email": "test@example.com", "password": "TestPass123!", "name": "Test User"}'
```

## Common Issues

### 1. Installation and Import Issues

#### Issue: ImportError when importing native auth modules

```python
ImportError: No module named 'fraiseql.auth.native'
```

**Solutions:**

```bash
# Verify FraiseQL version
pip show fraiseql

# Upgrade to latest version
pip install --upgrade "fraiseql>=0.1.0b31"

# Install with auth dependencies
pip install "fraiseql[auth]>=0.1.0b31"

# Check Python path
python -c "import sys; print('\\n'.join(sys.path))"
```

#### Issue: Dependencies missing for auth features

```python
ModuleNotFoundError: No module named 'passlib'
ModuleNotFoundError: No module named 'pyjwt'
```

**Solutions:**

```bash
# Install auth dependencies manually
pip install "passlib[argon2]>=1.7.4"
pip install "pyjwt[crypto]>=2.8.0"
pip install "psycopg[pool]>=3.2.6"

# Or use the auth extra
pip install "fraiseql[auth]>=0.1.0b31"
```

### 2. Database Issues

#### Issue: Database connection failures

```
psycopg.OperationalError: connection to server failed
```

**Diagnostic Steps:**

```bash
# Check if PostgreSQL is running
systemctl status postgresql
# or
brew services list | grep postgresql

# Test direct connection
psql -h localhost -p 5432 -U postgres -d postgres

# Check if database exists
psql -h localhost -p 5432 -U postgres -l | grep fraiseql

# Verify DATABASE_URL format
echo $DATABASE_URL
# Should be: postgresql://username:password@host:port/database
```

**Solutions:**

```bash
# Start PostgreSQL
systemctl start postgresql
# or
brew services start postgresql

# Create database if missing
createdb fraiseql_dev
# or
psql -c "CREATE DATABASE fraiseql_dev;"

# Fix DATABASE_URL format
export DATABASE_URL="postgresql://username:password@localhost:5432/fraiseql_dev"
```

#### Issue: Tables not found

```
psycopg.errors.UndefinedTable: relation "tb_user" does not exist
```

**Solutions:**

```python
# Run database migration
from fraiseql.auth.native import apply_native_auth_schema
from psycopg_pool import AsyncConnectionPool
import asyncio

async def migrate():
    pool = AsyncConnectionPool("your-database-url")
    await apply_native_auth_schema(pool)
    await pool.close()

asyncio.run(migrate())
```

```bash
# Or use the migration script
python scripts/migrate.py
```

#### Issue: Permission denied on database operations

```
psycopg.errors.InsufficientPrivilege: permission denied for table tb_user
```

**Solutions:**

```sql
-- Grant permissions to database user
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO your_user;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO your_user;

-- Or create user with proper permissions
CREATE USER fraiseql_user WITH PASSWORD 'secure_password';
GRANT ALL PRIVILEGES ON DATABASE fraiseql_dev TO fraiseql_user;
```

### 3. Authentication Issues

#### Issue: JWT secret key not set

```
ValueError: JWT secret key required. Set JWT_SECRET_KEY environment variable
```

**Solutions:**

```bash
# Generate secure secret key
export JWT_SECRET_KEY="$(python -c 'import secrets; print(secrets.token_urlsafe(64))')"

# Or set manually (not recommended for production)
export JWT_SECRET_KEY="your-super-secure-secret-key-here"

# Verify it's set
echo $JWT_SECRET_KEY
```

#### Issue: Invalid token errors

```json
{
  "detail": {
    "code": "INVALID_TOKEN",
    "message": "Token is invalid or expired"
  }
}
```

**Diagnostic Steps:**

```python
# Debug token validation
from fraiseql.auth.native.tokens import TokenManager
import os

token_manager = TokenManager(secret_key=os.environ["JWT_SECRET_KEY"])

# Test token generation
tokens = token_manager.generate_tokens("test-user-id")
print(f"Generated token: {tokens['access_token']}")

# Test token validation
try:
    payload = token_manager.verify_access_token(tokens['access_token'])
    print(f"Token valid: {payload}")
except Exception as e:
    print(f"Token invalid: {e}")
```

**Solutions:**

1. **Token expired**: Use refresh token to get new access token
2. **Wrong secret key**: Verify JWT_SECRET_KEY is consistent
3. **Malformed token**: Check token format and transmission
4. **Clock skew**: Synchronize server clocks

#### Issue: Password validation failures

```json
{
  "detail": {
    "code": "VALIDATION_ERROR",
    "message": "Password does not meet security requirements"
  }
}
```

**Default Requirements:**
- Minimum 8 characters
- At least 1 uppercase letter
- At least 1 lowercase letter
- At least 1 digit
- At least 1 special character

**Test Password Validation:**

```python
from fraiseql.auth.native.models import User

user = User()

# Test different passwords
test_passwords = [
    "weak",                    # Too short
    "WeakPassword",           # No digit or special char
    "weakpass123",            # No uppercase
    "WEAKPASS123!",           # No lowercase
    "WeakPass123!",           # Should pass
]

for password in test_passwords:
    is_valid = user.validate_password(password)
    print(f"{password:<20} {'✅' if is_valid else '❌'}")
```

### 4. Performance Issues

#### Issue: Slow authentication responses

**Diagnostic Steps:**

```python
# Time password hashing
import time
from fraiseql.auth.native.models import User

user = User()
start = time.time()
user.set_password("TestPassword123!")
duration = time.time() - start
print(f"Password hashing took: {duration:.3f}s")
# Should be ~0.1s (100ms)
```

**Solutions:**

1. **Argon2id too aggressive**: Reduce memory_cost or time_cost
2. **Database connection pool**: Increase pool size
3. **Network latency**: Check database network connection

```python
# Tune Argon2id parameters (less secure but faster)
from passlib.hash import argon2

# Current settings (secure)
argon2_secure = argon2.using(
    memory_cost=102400,  # 100MB
    time_cost=2,         # 2 iterations
    parallelism=8        # 8 threads
)

# Faster settings (less secure)
argon2_fast = argon2.using(
    memory_cost=65536,   # 64MB
    time_cost=1,         # 1 iteration
    parallelism=4        # 4 threads
)
```

#### Issue: High memory usage

**Diagnostic Steps:**

```bash
# Monitor memory usage
ps aux | grep python
top -p $(pgrep -f "your-app")

# Check connection pool size
# Look for DATABASE_POOL_MAX_SIZE in your configuration
```

**Solutions:**

```python
# Reduce connection pool size
pool = AsyncConnectionPool(
    database_url,
    min_size=5,    # Reduced from 10
    max_size=20    # Reduced from 50
)

# Optimize Argon2id memory usage
argon2_optimized = argon2.using(
    memory_cost=65536,  # 64MB instead of 100MB
    time_cost=2,
    parallelism=4       # 4 threads instead of 8
)
```

### 5. Rate Limiting Issues

#### Issue: Legitimate users getting rate limited

```json
{
  "detail": {
    "code": "RATE_LIMITED",
    "message": "Too many requests. Try again later."
  }
}
```

**Diagnostic Steps:**

```python
# Check current rate limiting configuration
from fraiseql.auth.native.middleware import RateLimitMiddleware

# View rate limit settings
print(f"General limit: {middleware.requests_per_minute}/min")
print(f"Auth limit: {middleware.auth_requests_per_minute}/min")
```

**Solutions:**

```python
# Increase rate limits
from fraiseql.auth.native import add_security_middleware

add_security_middleware(
    app,
    secret_key="your-secret",
    enable_rate_limiting=True,
    rate_limit_requests_per_minute=120,     # Increased from 60
    rate_limit_auth_requests_per_minute=10  # Increased from 5
)
```

```python
# Implement IP whitelisting
WHITELISTED_IPS = ["192.168.1.0/24", "10.0.0.0/8"]

async def is_whitelisted_ip(ip: str) -> bool:
    import ipaddress
    for subnet in WHITELISTED_IPS:
        if ipaddress.ip_address(ip) in ipaddress.ip_network(subnet):
            return True
    return False
```

### 6. Session Management Issues

#### Issue: Users logged out unexpectedly

**Diagnostic Steps:**

```python
# Check session status
async def debug_session(user_id: str):
    async with db_pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(
                "SELECT pk_session, token_family, created_at, last_active, revoked_at "
                "FROM tb_session WHERE fk_user = %s",
                (user_id,)
            )
            sessions = await cursor.fetchall()
            
            for session in sessions:
                print(f"Session: {session[0]}")
                print(f"Family: {session[1]}")
                print(f"Created: {session[2]}")
                print(f"Last active: {session[3]}")
                print(f"Revoked: {session[4] or 'Active'}")
                print("---")
```

**Common Causes:**

1. **Token theft detection triggered**: Check for token reuse
2. **Session timeout**: Check session TTL settings
3. **Manual session revocation**: Check admin actions
4. **Database cleanup**: Check for automated session cleanup

**Solutions:**

```python
# Increase token TTL
token_manager = TokenManager(
    secret_key=jwt_secret,
    access_token_ttl=timedelta(minutes=30),   # Increased from 15
    refresh_token_ttl=timedelta(days=60)      # Increased from 30
)

# Disable automatic session cleanup
# Comment out or modify session cleanup jobs
```

### 7. Multi-Tenant Issues

#### Issue: Cross-tenant data access

**Diagnostic Steps:**

```python
# Test tenant isolation
async def test_tenant_isolation():
    # Create providers for different tenants
    tenant1_provider = await create_native_auth_provider(
        db_pool, schema="tenant_1"
    )
    tenant2_provider = await create_native_auth_provider(
        db_pool, schema="tenant_2"
    )
    
    # Test that tenant1 cannot access tenant2 data
    try:
        tenant1_users = await get_all_users(tenant1_provider)
        tenant2_users = await get_all_users(tenant2_provider)
        
        # Should be different
        assert tenant1_users != tenant2_users
        print("✅ Tenant isolation working correctly")
    except Exception as e:
        print(f"❌ Tenant isolation issue: {e}")
```

**Solutions:**

```python
# Ensure schema is always specified
async def get_user_safely(db_pool, schema: str, user_id: str):
    async with db_pool.connection() as conn:
        # Always use schema prefix
        await conn.execute(f"SET search_path TO {schema}, public")
        async with conn.cursor() as cursor:
            await cursor.execute(
                "SELECT * FROM tb_user WHERE pk_user = %s",
                (user_id,)
            )
            return await cursor.fetchone()
```

### 8. Frontend Integration Issues

#### Issue: CORS errors in browser

```
Access to fetch at 'http://localhost:8000/auth/login' from origin 'http://localhost:3000' has been blocked by CORS policy
```

**Solutions:**

```python
# Configure CORS properly
from fastapi.middleware.cors import CORSMiddleware

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000", "https://yourdomain.com"],
    allow_credentials=True,
    allow_methods=["GET", "POST", "PUT", "DELETE", "OPTIONS"],
    allow_headers=["Authorization", "Content-Type", "X-CSRF-Token"],
)
```

#### Issue: Tokens not persisting in browser

**Solutions:**

```javascript
// Proper token storage
class AuthStorage {
    setTokens(accessToken, refreshToken) {
        // Use localStorage for SPAs
        localStorage.setItem('access_token', accessToken);
        localStorage.setItem('refresh_token', refreshToken);
        
        // Or use secure cookies for SSR
        document.cookie = `access_token=${accessToken}; HttpOnly; Secure; SameSite=Strict`;
    }
    
    getAccessToken() {
        return localStorage.getItem('access_token');
    }
    
    clearTokens() {
        localStorage.removeItem('access_token');
        localStorage.removeItem('refresh_token');
    }
}
```

## Debugging Tools

### Enable Debug Logging

```python
# Enable detailed logging
import logging

# Set log level
logging.basicConfig(level=logging.DEBUG)

# Enable specific module logging
logging.getLogger("fraiseql.auth.native").setLevel(logging.DEBUG)
logging.getLogger("psycopg").setLevel(logging.DEBUG)
```

### Database Query Logging

```python
# Log all SQL queries
import asyncpg
import logging

# Enable asyncpg logging
logging.getLogger("asyncpg").setLevel(logging.DEBUG)

# Or log queries manually
class DebugCursor:
    def __init__(self, cursor):
        self._cursor = cursor
        self.logger = logging.getLogger("sql_debug")
    
    async def execute(self, query, parameters=None):
        self.logger.debug(f"SQL: {query}")
        if parameters:
            self.logger.debug(f"Parameters: {parameters}")
        return await self._cursor.execute(query, parameters)
```

### Token Debug Tool

```python
# debug_token.py
import sys
from fraiseql.auth.native.tokens import TokenManager
import json

def debug_token(token: str, secret_key: str):
    """Debug JWT token details"""
    token_manager = TokenManager(secret_key=secret_key)
    
    try:
        # Decode without verification first
        import jwt
        unverified = jwt.decode(token, options={"verify_signature": False})
        print("Unverified payload:")
        print(json.dumps(unverified, indent=2))
        print()
        
        # Try to verify
        verified = token_manager.verify_access_token(token)
        print("✅ Token is valid")
        print("Verified payload:")
        print(json.dumps(verified, indent=2))
        
    except jwt.ExpiredSignatureError:
        print("❌ Token has expired")
    except jwt.InvalidTokenError as e:
        print(f"❌ Token is invalid: {e}")
    except Exception as e:
        print(f"❌ Unexpected error: {e}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python debug_token.py <token> <secret_key>")
        sys.exit(1)
    
    debug_token(sys.argv[1], sys.argv[2])
```

Usage:
```bash
python debug_token.py "eyJhbGciOiJIUzI1NiIs..." "your-secret-key"
```

### Performance Profiler

```python
# profile_auth.py
import asyncio
import time
import cProfile
from fraiseql.auth.native.models import User

async def profile_auth_operations():
    """Profile authentication operations"""
    
    user = User()
    
    # Profile password hashing
    def hash_password():
        user.set_password("TestPassword123!")
    
    print("Profiling password hashing...")
    cProfile.run('hash_password()', sort='cumtime')
    
    # Profile token operations
    from fraiseql.auth.native.tokens import TokenManager
    token_manager = TokenManager(secret_key="test-secret")
    
    def generate_tokens():
        for _ in range(100):
            token_manager.generate_tokens("user-id")
    
    print("\nProfiling token generation...")
    cProfile.run('generate_tokens()', sort='cumtime')

# Run profiler
asyncio.run(profile_auth_operations())
```

## Testing Tools

### Auth Flow Tester

```python
# test_auth_flow.py
import asyncio
import aiohttp
import json

class AuthFlowTester:
    def __init__(self, base_url):
        self.base_url = base_url
        self.session = None
        self.tokens = {}
    
    async def __aenter__(self):
        self.session = aiohttp.ClientSession()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.session.close()
    
    async def test_registration(self):
        """Test user registration"""
        data = {
            "email": f"test+{int(time.time())}@example.com",
            "password": "TestPassword123!",
            "name": "Test User"
        }
        
        async with self.session.post(
            f"{self.base_url}/auth/register",
            json=data
        ) as resp:
            if resp.status == 201:
                result = await resp.json()
                self.tokens = {
                    'access_token': result['access_token'],
                    'refresh_token': result['refresh_token']
                }
                print("✅ Registration successful")
                return True
            else:
                error = await resp.text()
                print(f"❌ Registration failed: {error}")
                return False
    
    async def test_login(self, email, password):
        """Test user login"""
        data = {"email": email, "password": password}
        
        async with self.session.post(
            f"{self.base_url}/auth/login",
            json=data
        ) as resp:
            if resp.status == 200:
                result = await resp.json()
                self.tokens = {
                    'access_token': result['access_token'],
                    'refresh_token': result['refresh_token']
                }
                print("✅ Login successful")
                return True
            else:
                error = await resp.text()
                print(f"❌ Login failed: {error}")
                return False
    
    async def test_protected_endpoint(self):
        """Test accessing protected endpoint"""
        headers = {
            'Authorization': f"Bearer {self.tokens['access_token']}"
        }
        
        async with self.session.get(
            f"{self.base_url}/auth/me",
            headers=headers
        ) as resp:
            if resp.status == 200:
                user = await resp.json()
                print(f"✅ Protected endpoint access successful: {user['email']}")
                return True
            else:
                error = await resp.text()
                print(f"❌ Protected endpoint access failed: {error}")
                return False
    
    async def test_token_refresh(self):
        """Test token refresh"""
        data = {"refresh_token": self.tokens['refresh_token']}
        
        async with self.session.post(
            f"{self.base_url}/auth/refresh",
            json=data
        ) as resp:
            if resp.status == 200:
                result = await resp.json()
                self.tokens.update({
                    'access_token': result['access_token'],
                    'refresh_token': result['refresh_token']
                })
                print("✅ Token refresh successful")
                return True
            else:
                error = await resp.text()
                print(f"❌ Token refresh failed: {error}")
                return False
    
    async def run_full_test(self):
        """Run complete authentication flow test"""
        print("🔄 Starting authentication flow test...\n")
        
        # Test registration
        if not await self.test_registration():
            return False
        
        # Test protected endpoint access
        if not await self.test_protected_endpoint():
            return False
        
        # Test token refresh
        if not await self.test_token_refresh():
            return False
        
        # Test protected endpoint with new token
        if not await self.test_protected_endpoint():
            return False
        
        print("\n🎉 All authentication tests passed!")
        return True

# Usage
async def main():
    async with AuthFlowTester("http://localhost:8000") as tester:
        await tester.run_full_test()

if __name__ == "__main__":
    import time
    asyncio.run(main())
```

## Getting Help

### Self-Service Resources

1. **Run the comprehensive test script**:
   ```bash
   python scripts/test-native-auth.py
   ```

2. **Check the logs** for detailed error information:
   ```bash
   tail -f /var/log/your-app.log
   grep -i "error\|warning" /var/log/your-app.log
   ```

3. **Use the debug tools** provided in this guide

4. **Check the [API Reference](api-reference.md)** for correct endpoint usage

### Community Support

- **GitHub Issues**: [Report bugs](https://github.com/fraiseql/fraiseql/issues)
- **GitHub Discussions**: [Ask questions](https://github.com/fraiseql/fraiseql/discussions)
- **Discord**: Join our [community server](https://discord.gg/fraiseql)

### Professional Support

For enterprise deployments needing dedicated support:
- **Enterprise Support**: Contact for SLA-backed support
- **Custom Development**: Paid development for specific requirements
- **Migration Services**: Professional migration assistance

### Issue Reporting Template

When reporting issues, please include:

```markdown
## Environment
- FraiseQL version: 
- Python version: 
- PostgreSQL version: 
- Operating System: 
- Deployment type: (local/docker/kubernetes/etc.)

## Issue Description
Brief description of the problem

## Steps to Reproduce
1. 
2. 
3. 

## Expected Behavior
What should have happened

## Actual Behavior
What actually happened

## Logs/Error Messages
```
paste logs here
```

## Configuration
- JWT_SECRET_KEY: [SET/NOT SET]
- DATABASE_URL: [REDACTED CONNECTION STRING]
- Other relevant environment variables

## Additional Context
Any other relevant information
```

---

**Remember**: Most authentication issues are configuration-related. Double-check your environment variables, database connectivity, and secret keys before seeking help.