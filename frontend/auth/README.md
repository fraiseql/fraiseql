# FraiseQL Native Authentication Frontend Components

This directory contains TypeScript types, Vue.js components, and composables for integrating with FraiseQL's native authentication system.

## Features

- 🔐 **Complete Auth Flow**: Login, registration, password reset, session management
- 🎯 **Type Safety**: Full TypeScript support with backend-matching types
- ⚛️ **Reactive State**: Vue 3 / Nuxt 3 composables with reactive auth state
- 🔄 **Auto Token Refresh**: Automatic JWT token refresh with theft detection
- 📱 **Responsive UI**: Beautiful, accessible forms that work on all devices
- 🛡️ **Security**: Password strength validation, secure token storage
- 🎨 **Customizable**: Easy to style and extend components

## Quick Start

### 1. Install Dependencies

```bash
npm install vue@^3.0.0 @vue/composition-api
```

### 2. Use the Auth Composable

```vue
<script setup lang="ts">
import { useAuth } from './auth/composables/useAuth';

const {
  user,
  isAuthenticated,
  login,
  logout,
  register
} = useAuth({
  baseUrl: 'http://localhost:8000',
  redirectOnLogin: '/dashboard'
});
</script>

<template>
  <div v-if="isAuthenticated">
    Welcome {{ user?.name }}!
    <button @click="logout">Logout</button>
  </div>
</template>
```

### 3. Use Pre-built Components

```vue
<script setup>
import LoginForm from './auth/components/LoginForm.vue';
import RegisterForm from './auth/components/RegisterForm.vue';
</script>

<template>
  <!-- Login page -->
  <LoginForm
    redirect-to="/dashboard"
    @success="handleSuccess"
    @error="handleError"
  />

  <!-- Registration page -->
  <RegisterForm
    redirect-to="/welcome"
    @success="handleSuccess"
    @error="handleError"
  />
</template>
```

### 4. Make Authenticated GraphQL Requests

```vue
<script setup>
import { useAuth } from './auth/composables/useAuth';

const { graphqlQuery, isAuthenticated } = useAuth();

const fetchUserPosts = async () => {
  if (!isAuthenticated.value) return;

  const response = await graphqlQuery(`
    query MyPosts {
      myPosts(limit: 10) {
        id
        title
        content
        createdAt
      }
    }
  `);

  return response.data?.myPosts || [];
};
</script>
```

## File Structure

```
frontend/auth/
├── types.ts                     # TypeScript type definitions
├── client.ts                    # Core auth client (framework-agnostic)
├── composables/
│   └── useAuth.ts              # Vue 3 / Nuxt 3 composable
├── components/
│   ├── LoginForm.vue           # Complete login form
│   └── RegisterForm.vue        # Complete registration form
└── README.md                   # This file
```

## Configuration

### Environment Variables

Set these in your `.env` file:

```env
# Backend API URL
API_BASE_URL=http://localhost:8000

# JWT secret (must match backend)
JWT_SECRET_KEY=your-secret-key-here
```

### Auth Client Options

```typescript
const { ... } = useAuth({
  baseUrl: 'http://localhost:8000',     // Backend URL
  authPrefix: '/auth',                  // Auth endpoints prefix
  graphqlEndpoint: '/graphql',          // GraphQL endpoint
  redirectOnLogin: '/dashboard',        // Where to go after login
  redirectOnLogout: '/login',           // Where to go after logout
  autoRefresh: true                     // Auto refresh user data
});
```

## API Reference

### useAuth Composable

#### Reactive State
- `user` - Current user object (or null)
- `tokens` - Current JWT tokens (or null)
- `isAuthenticated` - Boolean auth status
- `isLoading` - Loading state for auth operations
- `error` - Current error message (or null)

#### Authentication Methods
- `login(data)` - Sign in with email/password
- `register(data)` - Create new account
- `logout()` - Sign out and clear session
- `forgotPassword(email)` - Request password reset
- `resetPassword(token, password)` - Reset password with token
- `refreshUser()` - Refresh current user data

#### Session Management
- `getSessions()` - Get all user sessions
- `revokeSession(id)` - Revoke a specific session

#### Utilities
- `hasRole(role)` - Check if user has role
- `hasPermission(permission)` - Check if user has permission
- `hasAnyRole(roles)` - Check if user has any of the roles
- `hasAnyPermission(permissions)` - Check if user has any of the permissions
- `validatePassword(password)` - Validate password strength
- `graphqlQuery(query, variables)` - Make authenticated GraphQL request

### Components

#### LoginForm
```vue
<LoginForm
  redirect-to="/dashboard"
  @success="user => { ... }"
  @error="error => { ... }"
/>
```

#### RegisterForm
```vue
<RegisterForm
  redirect-to="/welcome"
  @success="user => { ... }"
  @error="error => { ... }"
/>
```

## Customization

### Styling
All components use scoped CSS that can be easily overridden:

```vue
<style>
/* Override login form styles */
.login-form {
  max-width: 500px;
  background: #f8f9fa;
}

.form-submit {
  background-color: #your-brand-color;
}
</style>
```

### Custom Components
Use the auth client directly for custom components:

```vue
<script setup>
import { FraiseQLAuthClient } from './auth/client';

const authClient = new FraiseQLAuthClient({
  baseUrl: 'http://localhost:8000'
});

const handleCustomLogin = async () => {
  try {
    const response = await authClient.login({
      email: 'user@example.com',
      password: 'password123'
    });
    console.log('Login successful:', response);
  } catch (error) {
    console.error('Login failed:', error);
  }
};
</script>
```

## Integration Examples

### Nuxt 3 Plugin

Create `plugins/auth.client.ts`:

```typescript
import { useAuth } from '~/auth/composables/useAuth';

export default defineNuxtPlugin(async () => {
  const { initialize } = useAuth({
    baseUrl: useRuntimeConfig().public.apiBaseUrl
  });

  // Initialize auth state on app start
  await initialize();
});
```

### Route Guards

```typescript
// middleware/auth.ts
export default defineNuxtRouteMiddleware((to) => {
  const { isAuthenticated } = useAuth();

  if (!isAuthenticated.value) {
    return navigateTo('/login');
  }
});
```

### Global Error Handling

```typescript
// plugins/error-handler.ts
export default defineNuxtPlugin(() => {
  const { clearAuth } = useAuth();

  // Handle 401 errors globally
  $fetch.create({
    onResponseError({ response }) {
      if (response.status === 401) {
        clearAuth();
        navigateTo('/login');
      }
    }
  });
});
```

## Security Considerations

- **Token Storage**: Tokens are stored in localStorage by default. For high-security applications, consider using secure HTTP-only cookies.
- **HTTPS**: Always use HTTPS in production.
- **CSP**: Implement Content Security Policy headers.
- **Token Expiry**: Access tokens have short lifespans (15 minutes by default).
- **Refresh Rotation**: Refresh tokens are rotated on each use to prevent reuse attacks.

## Browser Support

- Chrome 88+
- Firefox 85+
- Safari 14+
- Edge 88+

## Contributing

When adding new features:

1. Update TypeScript types in `types.ts`
2. Add client methods in `client.ts`
3. Expose via the `useAuth` composable
4. Create or update Vue components as needed
5. Update this README
