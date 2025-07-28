/**
 * TypeScript types for FraiseQL native authentication.
 *
 * These types correspond to the Pydantic models in the Python backend
 * and provide full type safety for frontend auth operations.
 */

export interface User {
  id: string;
  email: string;
  name: string;
  roles: string[];
  permissions: string[];
  is_active: boolean;
  email_verified: boolean;
  created_at: string; // ISO datetime string
  updated_at: string; // ISO datetime string
}

export interface AuthTokens {
  access_token: string;
  refresh_token: string;
  token_type: "bearer";
}

export interface AuthResponse {
  user: User;
  access_token: string;
  refresh_token: string;
  token_type: "bearer";
}

export interface RegisterRequest {
  email: string;
  password: string;
  name: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface RefreshRequest {
  refresh_token: string;
}

export interface ForgotPasswordRequest {
  email: string;
}

export interface ResetPasswordRequest {
  token: string;
  new_password: string;
}

export interface MessageResponse {
  message: string;
}

export interface Session {
  id: string;
  user_agent?: string;
  ip_address?: string;
  created_at: string;
  last_used_at: string;
  is_current: boolean;
}

export interface AuthError {
  detail: string;
  type?: string;
}

export interface AuthState {
  user: User | null;
  tokens: AuthTokens | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
}

export interface PasswordValidation {
  isValid: boolean;
  errors: string[];
}

// GraphQL Error types (for GraphQL operations with auth)
export interface GraphQLError {
  message: string;
  locations?: { line: number; column: number }[];
  path?: string[];
  extensions?: {
    code?: string;
    [key: string]: any;
  };
}

export interface GraphQLResponse<T = any> {
  data?: T;
  errors?: GraphQLError[];
}
