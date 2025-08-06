/**
 * FraiseQL Native Auth Client
 *
 * Provides a complete client-side authentication solution for FraiseQL
 * with automatic token management, refresh handling, and error recovery.
 */

import type {
  AuthResponse,
  AuthTokens,
  ForgotPasswordRequest,
  GraphQLResponse,
  LoginRequest,
  MessageResponse,
  RefreshRequest,
  RegisterRequest,
  ResetPasswordRequest,
  Session,
  User,
} from './types';

export interface AuthClientConfig {
  baseUrl: string;
  authPrefix?: string;
  graphqlEndpoint?: string;
  storage?: Storage;
  onTokenExpired?: () => void;
  onError?: (error: Error) => void;
}

export class FraiseQLAuthClient {
  private baseUrl: string;
  private authPrefix: string;
  private graphqlEndpoint: string;
  private storage: Storage;
  private onTokenExpired?: () => void;
  private onError?: (error: Error) => void;
  private refreshPromise: Promise<AuthTokens> | null = null;

  constructor(config: AuthClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, '');
    this.authPrefix = config.authPrefix || '/auth';
    this.graphqlEndpoint = config.graphqlEndpoint || '/graphql';
    this.storage = config.storage || (typeof window !== 'undefined' ? localStorage : {} as Storage);
    this.onTokenExpired = config.onTokenExpired;
    this.onError = config.onError;
  }

  // Token management
  private getStoredTokens(): AuthTokens | null {
    try {
      const stored = this.storage.getItem('fraiseql_tokens');
      return stored ? JSON.parse(stored) : null;
    } catch {
      return null;
    }
  }

  private setStoredTokens(tokens: AuthTokens | null): void {
    if (tokens) {
      this.storage.setItem('fraiseql_tokens', JSON.stringify(tokens));
    } else {
      this.storage.removeItem('fraiseql_tokens');
    }
  }

  private getStoredUser(): User | null {
    try {
      const stored = this.storage.getItem('fraiseql_user');
      return stored ? JSON.parse(stored) : null;
    } catch {
      return null;
    }
  }

  private setStoredUser(user: User | null): void {
    if (user) {
      this.storage.setItem('fraiseql_user', JSON.stringify(user));
    } else {
      this.storage.removeItem('fraiseql_user');
    }
  }

  // HTTP utilities
  private async makeRequest<T>(
    path: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;

    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
    });

    if (!response.ok) {
      const error = await response.json().catch(() => ({ detail: 'Request failed' }));
      throw new Error(error.detail || `HTTP ${response.status}`);
    }

    return response.json();
  }

  private async makeAuthenticatedRequest<T>(
    path: string,
    options: RequestInit = {}
  ): Promise<T> {
    const tokens = await this.getValidTokens();

    return this.makeRequest<T>(path, {
      ...options,
      headers: {
        ...options.headers,
        Authorization: `Bearer ${tokens.access_token}`,
      },
    });
  }

  // Token refresh logic
  private async getValidTokens(): Promise<AuthTokens> {
    const tokens = this.getStoredTokens();

    if (!tokens) {
      throw new Error('Not authenticated');
    }

    // Check if access token is expired (simple JWT decode)
    try {
      const payload = JSON.parse(atob(tokens.access_token.split('.')[1]));
      const now = Math.floor(Date.now() / 1000);

      // If token expires in less than 1 minute, refresh it
      if (payload.exp && payload.exp - now < 60) {
        return await this.refreshTokens();
      }
    } catch {
      // If we can't decode the token, try to refresh
      return await this.refreshTokens();
    }

    return tokens;
  }

  private async refreshTokens(): Promise<AuthTokens> {
    // Prevent multiple simultaneous refresh requests
    if (this.refreshPromise) {
      return this.refreshPromise;
    }

    this.refreshPromise = this._performTokenRefresh();

    try {
      const newTokens = await this.refreshPromise;
      this.setStoredTokens(newTokens);
      return newTokens;
    } catch (error) {
      // If refresh fails, clear tokens and notify
      this.clearAuth();
      this.onTokenExpired?.();
      throw error;
    } finally {
      this.refreshPromise = null;
    }
  }

  private async _performTokenRefresh(): Promise<AuthTokens> {
    const tokens = this.getStoredTokens();

    if (!tokens?.refresh_token) {
      throw new Error('No refresh token available');
    }

    const response = await this.makeRequest<AuthTokens>(
      `${this.authPrefix}/refresh`,
      {
        method: 'POST',
        body: JSON.stringify({ refresh_token: tokens.refresh_token }),
      }
    );

    return response;
  }

  // Authentication methods
  async register(data: RegisterRequest): Promise<AuthResponse> {
    const response = await this.makeRequest<AuthResponse>(
      `${this.authPrefix}/register`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );

    // Store tokens and user
    this.setStoredTokens({
      access_token: response.access_token,
      refresh_token: response.refresh_token,
      token_type: response.token_type,
    });
    this.setStoredUser(response.user);

    return response;
  }

  async login(data: LoginRequest): Promise<AuthResponse> {
    const response = await this.makeRequest<AuthResponse>(
      `${this.authPrefix}/login`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );

    // Store tokens and user
    this.setStoredTokens({
      access_token: response.access_token,
      refresh_token: response.refresh_token,
      token_type: response.token_type,
    });
    this.setStoredUser(response.user);

    return response;
  }

  async logout(): Promise<void> {
    const tokens = this.getStoredTokens();

    if (tokens?.refresh_token) {
      try {
        await this.makeRequest<MessageResponse>(
          `${this.authPrefix}/logout`,
          {
            method: 'POST',
            body: JSON.stringify({ refresh_token: tokens.refresh_token }),
            headers: {
              Authorization: `Bearer ${tokens.access_token}`,
            },
          }
        );
      } catch (error) {
        // Ignore logout errors - still clear local storage
        console.warn('Logout request failed:', error);
      }
    }

    this.clearAuth();
  }

  async forgotPassword(data: ForgotPasswordRequest): Promise<MessageResponse> {
    return this.makeRequest<MessageResponse>(
      `${this.authPrefix}/forgot-password`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
  }

  async resetPassword(data: ResetPasswordRequest): Promise<MessageResponse> {
    return this.makeRequest<MessageResponse>(
      `${this.authPrefix}/reset-password`,
      {
        method: 'POST',
        body: JSON.stringify(data),
      }
    );
  }

  async getCurrentUser(): Promise<User> {
    return this.makeAuthenticatedRequest<User>(`${this.authPrefix}/me`);
  }

  async getSessions(): Promise<Session[]> {
    return this.makeAuthenticatedRequest<Session[]>(`${this.authPrefix}/sessions`);
  }

  async revokeSession(sessionId: string): Promise<MessageResponse> {
    return this.makeAuthenticatedRequest<MessageResponse>(
      `${this.authPrefix}/sessions/${sessionId}`,
      { method: 'DELETE' }
    );
  }

  // GraphQL integration
  async graphqlQuery<T = any>(
    query: string,
    variables?: Record<string, any>
  ): Promise<GraphQLResponse<T>> {
    const tokens = await this.getValidTokens();

    return this.makeRequest<GraphQLResponse<T>>(
      this.graphqlEndpoint,
      {
        method: 'POST',
        body: JSON.stringify({ query, variables }),
        headers: {
          Authorization: `Bearer ${tokens.access_token}`,
        },
      }
    );
  }

  // State management
  isAuthenticated(): boolean {
    const tokens = this.getStoredTokens();
    return !!tokens?.access_token;
  }

  getCurrentTokens(): AuthTokens | null {
    return this.getStoredTokens();
  }

  getStoredUserData(): User | null {
    return this.getStoredUser();
  }

  clearAuth(): void {
    this.setStoredTokens(null);
    this.setStoredUser(null);
  }

  // Utility methods
  validatePassword(password: string): { isValid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (password.length < 8) {
      errors.push('Password must be at least 8 characters long');
    }

    if (!/[A-Z]/.test(password)) {
      errors.push('Password must contain at least one uppercase letter');
    }

    if (!/[a-z]/.test(password)) {
      errors.push('Password must contain at least one lowercase letter');
    }

    if (!/\d/.test(password)) {
      errors.push('Password must contain at least one digit');
    }

    if (!/[!@#$%^&*(),.?":{}|<>]/.test(password)) {
      errors.push('Password must contain at least one special character');
    }

    return {
      isValid: errors.length === 0,
      errors,
    };
  }

  // Create an authenticated fetch function for custom requests
  createAuthenticatedFetch() {
    return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const tokens = await this.getValidTokens();

      return fetch(input, {
        ...init,
        headers: {
          ...init?.headers,
          Authorization: `Bearer ${tokens.access_token}`,
        },
      });
    };
  }
}
