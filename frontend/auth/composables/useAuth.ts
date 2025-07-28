/**
 * Vue 3 / Nuxt 3 Composable for FraiseQL Native Authentication
 *
 * Provides reactive authentication state and methods for Vue/Nuxt applications.
 * Integrates seamlessly with FraiseQL's native auth backend.
 */

import { computed, ref, reactive, onMounted, watch } from 'vue';
import type { Ref } from 'vue';
import { FraiseQLAuthClient } from '../client';
import type {
  AuthResponse,
  AuthState,
  LoginRequest,
  RegisterRequest,
  ForgotPasswordRequest,
  ResetPasswordRequest,
  User,
  Session,
} from '../types';

// Global auth state (shared across components)
const globalAuthState = reactive<AuthState>({
  user: null,
  tokens: null,
  isAuthenticated: false,
  isLoading: false,
  error: null,
});

// Global client instance
let authClient: FraiseQLAuthClient | null = null;

export interface UseAuthOptions {
  baseUrl?: string;
  authPrefix?: string;
  graphqlEndpoint?: string;
  redirectOnLogin?: string;
  redirectOnLogout?: string;
  autoRefresh?: boolean;
}

export function useAuth(options: UseAuthOptions = {}) {
  // Initialize client if not already done
  if (!authClient) {
    authClient = new FraiseQLAuthClient({
      baseUrl: options.baseUrl || process.env.API_BASE_URL || 'http://localhost:8000',
      authPrefix: options.authPrefix,
      graphqlEndpoint: options.graphqlEndpoint,
      onTokenExpired: () => {
        globalAuthState.user = null;
        globalAuthState.tokens = null;
        globalAuthState.isAuthenticated = false;
        globalAuthState.error = 'Your session has expired. Please log in again.';

        // Redirect to login if specified
        if (options.redirectOnLogout && typeof window !== 'undefined') {
          window.location.href = options.redirectOnLogout;
        }
      },
      onError: (error) => {
        globalAuthState.error = error.message;
      },
    });
  }

  // Reactive state refs
  const user = computed(() => globalAuthState.user);
  const tokens = computed(() => globalAuthState.tokens);
  const isAuthenticated = computed(() => globalAuthState.isAuthenticated);
  const isLoading = computed(() => globalAuthState.isLoading);
  const error = computed(() => globalAuthState.error);

  // Error handling
  const clearError = () => {
    globalAuthState.error = null;
  };

  const setError = (message: string) => {
    globalAuthState.error = message;
  };

  // Authentication methods
  const register = async (data: RegisterRequest): Promise<AuthResponse | null> => {
    globalAuthState.isLoading = true;
    globalAuthState.error = null;

    try {
      const response = await authClient!.register(data);

      globalAuthState.user = response.user;
      globalAuthState.tokens = {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        token_type: response.token_type,
      };
      globalAuthState.isAuthenticated = true;

      // Redirect if specified
      if (options.redirectOnLogin && typeof window !== 'undefined') {
        window.location.href = options.redirectOnLogin;
      }

      return response;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Registration failed';
      globalAuthState.error = message;
      return null;
    } finally {
      globalAuthState.isLoading = false;
    }
  };

  const login = async (data: LoginRequest): Promise<AuthResponse | null> => {
    globalAuthState.isLoading = true;
    globalAuthState.error = null;

    try {
      const response = await authClient!.login(data);

      globalAuthState.user = response.user;
      globalAuthState.tokens = {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        token_type: response.token_type,
      };
      globalAuthState.isAuthenticated = true;

      // Redirect if specified
      if (options.redirectOnLogin && typeof window !== 'undefined') {
        window.location.href = options.redirectOnLogin;
      }

      return response;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Login failed';
      globalAuthState.error = message;
      return null;
    } finally {
      globalAuthState.isLoading = false;
    }
  };

  const logout = async (): Promise<void> => {
    globalAuthState.isLoading = true;

    try {
      await authClient!.logout();
    } catch (err) {
      console.warn('Logout request failed:', err);
    } finally {
      globalAuthState.user = null;
      globalAuthState.tokens = null;
      globalAuthState.isAuthenticated = false;
      globalAuthState.isLoading = false;
      globalAuthState.error = null;

      // Redirect if specified
      if (options.redirectOnLogout && typeof window !== 'undefined') {
        window.location.href = options.redirectOnLogout;
      }
    }
  };

  const forgotPassword = async (data: ForgotPasswordRequest): Promise<boolean> => {
    globalAuthState.isLoading = true;
    globalAuthState.error = null;

    try {
      await authClient!.forgotPassword(data);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Password reset request failed';
      globalAuthState.error = message;
      return false;
    } finally {
      globalAuthState.isLoading = false;
    }
  };

  const resetPassword = async (data: ResetPasswordRequest): Promise<boolean> => {
    globalAuthState.isLoading = true;
    globalAuthState.error = null;

    try {
      await authClient!.resetPassword(data);
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Password reset failed';
      globalAuthState.error = message;
      return false;
    } finally {
      globalAuthState.isLoading = false;
    }
  };

  const refreshUser = async (): Promise<User | null> => {
    if (!isAuthenticated.value) return null;

    globalAuthState.isLoading = true;
    globalAuthState.error = null;

    try {
      const userData = await authClient!.getCurrentUser();
      globalAuthState.user = userData;
      return userData;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to refresh user data';
      globalAuthState.error = message;
      return null;
    } finally {
      globalAuthState.isLoading = false;
    }
  };

  // Session management
  const sessions: Ref<Session[]> = ref([]);
  const sessionsLoading = ref(false);

  const getSessions = async (): Promise<Session[]> => {
    if (!isAuthenticated.value) return [];

    sessionsLoading.value = true;

    try {
      const sessionData = await authClient!.getSessions();
      sessions.value = sessionData;
      return sessionData;
    } catch (err) {
      console.error('Failed to load sessions:', err);
      return [];
    } finally {
      sessionsLoading.value = false;
    }
  };

  const revokeSession = async (sessionId: string): Promise<boolean> => {
    if (!isAuthenticated.value) return false;

    try {
      await authClient!.revokeSession(sessionId);
      // Refresh sessions list
      await getSessions();
      return true;
    } catch (err) {
      console.error('Failed to revoke session:', err);
      return false;
    }
  };

  // GraphQL helper
  const graphqlQuery = async <T = any>(
    query: string,
    variables?: Record<string, any>
  ) => {
    if (!isAuthenticated.value) {
      throw new Error('Authentication required for GraphQL queries');
    }

    return authClient!.graphqlQuery<T>(query, variables);
  };

  // Utility functions
  const hasRole = (role: string): boolean => {
    return user.value?.roles.includes(role) ?? false;
  };

  const hasPermission = (permission: string): boolean => {
    return user.value?.permissions.includes(permission) ?? false;
  };

  const hasAnyRole = (roles: string[]): boolean => {
    return roles.some(role => hasRole(role));
  };

  const hasAnyPermission = (permissions: string[]): boolean => {
    return permissions.some(permission => hasPermission(permission));
  };

  const validatePassword = (password: string) => {
    return authClient!.validatePassword(password);
  };

  // Initialize authentication state on mount
  const initialize = async () => {
    if (typeof window === 'undefined') return; // Skip during SSR

    const storedTokens = authClient!.getCurrentTokens();
    const storedUser = authClient!.getStoredUserData();

    if (storedTokens && storedUser) {
      globalAuthState.tokens = storedTokens;
      globalAuthState.user = storedUser;
      globalAuthState.isAuthenticated = true;

      // Optionally refresh user data to ensure it's current
      if (options.autoRefresh !== false) {
        await refreshUser();
      }
    }
  };

  // Auto-initialize on first use
  onMounted(() => {
    initialize();
  });

  return {
    // Reactive state
    user: readonly(user),
    tokens: readonly(tokens),
    isAuthenticated: readonly(isAuthenticated),
    isLoading: readonly(isLoading),
    error: readonly(error),

    // Session state
    sessions: readonly(sessions),
    sessionsLoading: readonly(sessionsLoading),

    // Authentication methods
    register,
    login,
    logout,
    forgotPassword,
    resetPassword,
    refreshUser,

    // Session management
    getSessions,
    revokeSession,

    // GraphQL
    graphqlQuery,

    // Utilities
    hasRole,
    hasPermission,
    hasAnyRole,
    hasAnyPermission,
    validatePassword,
    clearError,
    setError,
    initialize,

    // Raw client access for advanced use cases
    client: authClient,
  };
}

// Helper function to make state readonly
function readonly<T>(ref: Ref<T>) {
  return computed(() => ref.value);
}
