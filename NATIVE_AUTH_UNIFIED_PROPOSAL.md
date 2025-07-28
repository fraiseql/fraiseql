# 🔐 FraiseQL Native Authentication Proposal

## 🎯 TL;DR - Our Recommendation

**Go with REST-based authentication endpoints.** It's simpler, faster to implement, and easier to debug. You can always migrate to GraphQL later if needed.

### Why REST Wins

- **6 days faster** to implement (13 vs 19 days)
- **Dead simple** to debug with curl/Postman
- **Lower risk** - proven patterns everyone knows
- **Incremental path** - start REST, migrate to GraphQL later if desired

### Quick Implementation Plan

1. **Week 1**: Build REST auth endpoints
2. **Week 2**: Frontend integration
3. **Week 3**: Parallel run with Auth0
4. **Week 4**: Full migration complete 🎉

---

## 🤔 The Big Picture

We're replacing Auth0 with native JWT authentication using httpOnly cookies. Two paths emerged:

### Option A: REST Auth Endpoints (Recommended ✅)

- Traditional `/auth/login`, `/auth/logout` endpoints
- GraphQL for app data, REST for auth
- CSRF protection via double-submit cookies

### Option B: Pure GraphQL (Alternative)

- Everything through GraphQL mutations
- Unified API surface
- Auto-refresh magic in middleware

---

## 🚀 REST Implementation (Recommended)

### What It Looks Like

**Frontend - Clean & Simple:**

```typescript
// Just works™
const { user, login, logout } = useAuth()

await login('user@example.com', 'password')
// Done! Cookies set, user logged in
```

**Backend - Straightforward:**

```python
@router.post("/auth/login")
async def login(credentials: LoginCredentials, response: Response):
    # Validate user
    # Set httpOnly cookies
    # Return user data
    return {"user": user_data}
```

### Security Features

- ✅ httpOnly cookies (15min access, 30d refresh)
- ✅ CSRF double-submit protection
- ✅ Rate limiting (5 login attempts/minute)
- ✅ Token rotation with theft detection
- ✅ Bcrypt password hashing

### Frontend Integration

```typescript
// composables/useAuth.ts
export const useAuth = () => {
  const user = useState<User | null>('auth.user')

  const login = async (email: string, password: string) => {
    const { csrf_token } = await $fetch('/auth/csrf-token')
    const response = await $fetch('/auth/login', {
      method: 'POST',
      headers: { 'X-CSRF-Token': csrf_token },
      body: { email, password }
    })
    user.value = response.user
  }

  return { user, login, logout, checkAuth }
}
```

---

## 🎭 GraphQL Alternative (For Completeness)

<details>
<summary><strong>Click to explore the GraphQL approach</strong></summary>

### What It Looks Like

**Frontend - Apollo-Powered:**

```typescript
const { mutate: loginMutation } = useMutation(LOGIN_MUTATION)

const login = async (email: string, password: string) => {
  await loginMutation({ input: { email, password } })
  await refetchMe() // Update cache
}
```

**Backend - Pure GraphQL:**

```python
@fraiseql.mutation
async def login(info, input: LoginInput) -> AuthPayload:
    # Everything's a GraphQL resolver
    # Auto-refresh happens in middleware
    return AuthPayload(user=user, expiresAt=expires)
```

### The Good

- ✅ One API to rule them all
- ✅ Type-safe end-to-end
- ✅ Auto token refresh (no frontend timers!)
- ✅ Better for complex permissions

### The Not-So-Good

- ❌ 6 extra days to implement
- ❌ Hidden complexity in middleware
- ❌ Harder to debug ("why won't this cookie set?!")
- ❌ Team needs GraphQL expertise

### Implementation Effort

- **Backend**: 13 days (types, mutations, auto-refresh magic)
- **Frontend**: 6 days (Apollo setup, error handling)
- **Total**: 19 days vs 13 for REST

</details>

---

## 📊 Quick Comparison

| Feature | REST (Recommended) | GraphQL |
|---------|-------------------|----------|
| **Dev Time** | 13 days 🚀 | 19 days |
| **Complexity** | Low | High |
| **Debugging** | Easy (curl works!) | Requires GraphQL tools |
| **Token Refresh** | Explicit endpoint | Auto-magic middleware |
| **CSRF Protection** | Double-submit | Custom headers |
| **Team Learning** | Minimal | Significant |

---

## 🛠️ Implementation Timeline

### REST Approach (Recommended)

**Week 1**: Backend endpoints

- `/auth/login`, `/auth/logout`, `/auth/refresh`
- CSRF protection
- Rate limiting

**Week 2**: Frontend integration

- Auth composable
- Login/register pages
- Protected routes

**Week 3**: Migration prep

- User data export from Auth0
- Parallel authentication
- Testing all flows

**Week 4**: Go live! 🎉

---

## 🔮 Future Considerations

Starting with REST doesn't lock you in:

1. **Phase 1**: REST auth (you are here)
2. **Phase 2**: Production experience
3. **Phase 3**: Gradual GraphQL migration (if needed)
4. **Phase 4**: Full GraphQL nirvana (optional)

---

## 🎬 Final Verdict

**REST-based auth is the pragmatic choice.** It gets you to production faster with less risk. Your team can understand it immediately, and you can debug issues without specialized tools.

The GraphQL approach is architecturally pure but adds complexity without proportional benefits for authentication. Save the GraphQL goodness for your application's data layer where it truly shines.

**Remember**: The best authentication system is the one that works reliably in production. REST gets you there faster.

---

*Ready to start? Check out the implementation guide in `/docs/auth-implementation.md`* 🚀
