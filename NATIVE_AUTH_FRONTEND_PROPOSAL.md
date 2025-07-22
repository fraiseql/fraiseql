# Native Authentication Proposal for PrintOptim Frontend

## Overview

This proposal outlines a transition from Auth0 to a native FraiseQL authentication system. The goal is to simplify our auth stack while maintaining security and adding better RBAC support.

## Current Situation with Auth0

Based on analysis of the frontend codebase:
- Using `@auth0/auth0-vue` v2.4.0
- Tokens stored in localStorage
- Complex integration with Apollo Client
- Custom middleware for protected routes
- SSR disabled due to Auth0 limitations
- Monthly costs and external dependency

## Proposed Native Authentication Solution

### Core Architecture

Replace Auth0 with a JWT-based system using **httpOnly cookies** for better security and simpler frontend integration.

```
Frontend (Nuxt 3) <---> FraiseQL API <---> PostgreSQL
   |                       |                    |
   └── httpOnly cookies ───┘                    |
                                                |
                          Native Auth System ───┘
```

### What Changes for Frontend

#### 1. Authentication Flow

**Current (Auth0):**
```typescript
// Complex Auth0 setup
const auth0 = useAuth0()
await auth0.loginWithRedirect({
  appState: { target: '/dashboard' }
})
const token = await auth0.getAccessTokenSilently()
```

**Proposed (Native):**
```typescript
// Simple native auth
const { login } = useAuth()
await login(email, password)
// No token handling needed - cookies are automatic
```

#### 2. API Integration

**Current (Auth0):**
```typescript
// Apollo Client with manual token injection
const authLink = setContext(async (_, { headers }) => {
  const token = await auth0.getAccessTokenSilently()
  return {
    headers: {
      ...headers,
      authorization: token ? `Bearer ${token}` : "",
    }
  }
})
```

**Proposed (Native):**
```typescript
// Simple Apollo setup - cookies sent automatically
const httpLink = createHttpLink({
  uri: '/graphql',
  credentials: 'include' // That's it!
})
```

#### 3. Protected Routes

**Current (Auth0):**
```typescript
// Complex auth check with Auth0
if (!auth0.isAuthenticated) {
  await auth0.loginWithRedirect({
    appState: { target: to.fullPath }
  })
}
```

**Proposed (Native):**
```typescript
// Simple auth middleware
export default defineNuxtRouteMiddleware(async (to) => {
  const { user, checkAuth } = useAuth()
  
  if (!user.value) {
    const isAuthenticated = await checkAuth()
    if (!isAuthenticated) {
      return navigateTo(`/login?redirect=${to.fullPath}`)
    }
  }
})
```

### New Auth Composable

Complete auth composable for the frontend:

```typescript
// composables/useAuth.ts
export const useAuth = () => {
  const user = useState<User | null>('auth.user', () => null)
  const loading = useState('auth.loading', () => false)
  
  const login = async (email: string, password: string) => {
    loading.value = true
    try {
      const { csrf_token } = await $fetch('/auth/csrf-token')
      const response = await $fetch('/auth/login', {
        method: 'POST',
        headers: { 'X-CSRF-Token': csrf_token },
        body: { email, password }
      })
      user.value = response.user
      return { success: true }
    } catch (error) {
      return { success: false, error }
    } finally {
      loading.value = false
    }
  }
  
  const logout = async () => {
    const { csrf_token } = await $fetch('/auth/csrf-token')
    await $fetch('/auth/logout', {
      method: 'POST',
      headers: { 'X-CSRF-Token': csrf_token }
    })
    user.value = null
    await navigateTo('/')
  }
  
  const register = async (email: string, password: string, fullName: string) => {
    const { csrf_token } = await $fetch('/auth/csrf-token')
    const response = await $fetch('/auth/register', {
      method: 'POST',
      headers: { 'X-CSRF-Token': csrf_token },
      body: { email, password, fullName }
    })
    user.value = response.user
    return { success: true }
  }
  
  const checkAuth = async () => {
    try {
      const response = await $fetch('/auth/me')
      user.value = response.user
      return true
    } catch {
      user.value = null
      return false
    }
  }
  
  // RBAC helpers
  const hasRole = (role: string) => 
    user.value?.roles?.some(r => r.identifier === role) ?? false
    
  const hasPermission = (permission: string) => 
    user.value?.permissions?.some(p => p.identifier === permission) ?? false
    
  const can = (resource: string, action: string) => 
    user.value?.permissions?.some(p => 
      p.resource === resource && p.action === action
    ) ?? false
  
  return {
    user: readonly(user),
    loading: readonly(loading),
    login,
    logout,
    register,
    checkAuth,
    hasRole,
    hasPermission,
    can
  }
}
```

### RBAC in Templates

```vue
<template>
  <div>
    <!-- Role-based rendering -->
    <AdminPanel v-if="hasRole('admin')" />
    
    <!-- Permission-based rendering -->
    <button 
      v-if="can('posts', 'edit')" 
      @click="editPost"
    >
      Edit Post
    </button>
    
    <!-- Multiple roles -->
    <ManagerDashboard v-if="hasRole('manager') || hasRole('admin')" />
  </div>
</template>

<script setup>
const { hasRole, can } = useAuth()
</script>
```

### Benefits for Frontend

1. **Simpler Code**: No Auth0 SDK, just native fetch
2. **Better Performance**: No external API calls for tokens
3. **SSR Support**: Cookies work naturally with SSR
4. **No Token Management**: httpOnly cookies handle everything
5. **Better DX**: Simpler mental model, less configuration
6. **Type Safety**: Full TypeScript support with FraiseQL types

### Security Improvements

1. **httpOnly Cookies**: Tokens can't be stolen via XSS
2. **CSRF Protection**: Built-in double-submit cookie pattern
3. **No localStorage**: More secure token storage
4. **Automatic Renewal**: Seamless token refresh

### Migration Strategy

#### Phase 1: Parallel Systems (2 weeks)
- Implement native auth endpoints
- Both Auth0 and native auth work simultaneously
- No frontend changes required yet

#### Phase 2: Frontend Migration (1 week)
- Replace Auth0 provider with native composable
- Update Apollo Client configuration  
- Migrate protected routes
- Test all auth flows

#### Phase 3: Cleanup (1 week)
- Remove Auth0 dependencies
- Enable SSR
- Performance optimizations
- Documentation

### API Endpoints

```
POST   /auth/login          # Login
POST   /auth/logout         # Logout  
POST   /auth/refresh        # Refresh token (automatic)
GET    /auth/me            # Get current user
POST   /auth/register      # Register
GET    /auth/csrf-token    # Get CSRF token
```

### Response Formats

```typescript
// Login/Register Response
{
  user: {
    id: string
    email: string
    fullName: string
    avatarUrl?: string
    roles: Array<{
      id: string
      identifier: string
      name: string
    }>
    permissions: Array<{
      id: string
      identifier: string
      resource: string
      action: string
    }>
  }
}

// Error Response
{
  error: {
    code: 'INVALID_CREDENTIALS' | 'ACCOUNT_LOCKED' | etc
    message: string
    details?: any
  }
}
```

## Discussion Points

### 1. Login Page Design
- Should we keep redirect to separate login or use modal?
- Social login requirements for future?

### 2. Session Management  
- Logout from all devices feature needed?
- Remember me functionality?

### 3. User Experience
- How to handle token refresh failures?
- Loading states during auth checks?

### 4. Development Experience
- Need mock auth for development?
- Cypress test adjustments?

## Implementation Timeline

**Week 1**: Backend implementation (I'll handle this)
- Auth endpoints
- Database schema
- RBAC system

**Week 2**: Frontend integration (we collaborate)
- Auth composable
- Apollo configuration
- Route protection

**Week 3**: Testing & migration
- Migrate existing users
- Test all flows
- Fix edge cases

**Week 4**: Polish & deploy
- Performance optimization
- Documentation
- Production deployment

## Questions for Frontend Team

1. Any concerns about moving away from Auth0?
2. Preferences for login/register UX?
3. Additional RBAC requirements?
4. Timeline constraints?
5. Any Auth0 features we heavily depend on?

## Next Steps

1. Review and discuss this proposal
2. Agree on implementation approach
3. Create detailed tasks
4. Begin backend implementation
5. Coordinate on frontend changes

---

This native auth system will give us more control, better performance, and lower costs while simplifying the frontend codebase. Looking forward to your feedback!