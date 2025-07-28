<template>
  <div class="login-form">
    <div class="form-header">
      <h2>Sign In</h2>
      <p>Welcome back! Please sign in to your account.</p>
    </div>

    <form @submit.prevent="handleSubmit" class="form">
      <!-- Email field -->
      <div class="form-group">
        <label for="email" class="form-label">
          Email Address
        </label>
        <input
          id="email"
          v-model="form.email"
          type="email"
          required
          autocomplete="email"
          :disabled="isLoading"
          :class="['form-input', { 'form-input--error': errors.email }]"
          placeholder="you@example.com"
          @blur="validateEmail"
        >
        <span v-if="errors.email" class="form-error">
          {{ errors.email }}
        </span>
      </div>

      <!-- Password field -->
      <div class="form-group">
        <label for="password" class="form-label">
          Password
        </label>
        <div class="password-input-wrapper">
          <input
            id="password"
            v-model="form.password"
            :type="showPassword ? 'text' : 'password'"
            required
            autocomplete="current-password"
            :disabled="isLoading"
            :class="['form-input', { 'form-input--error': errors.password }]"
            placeholder="Enter your password"
            @blur="validatePassword"
          >
          <button
            type="button"
            class="password-toggle"
            :disabled="isLoading"
            @click="showPassword = !showPassword"
          >
            {{ showPassword ? '👁️' : '👁️‍🗨️' }}
          </button>
        </div>
        <span v-if="errors.password" class="form-error">
          {{ errors.password }}
        </span>
      </div>

      <!-- Remember me & Forgot password -->
      <div class="form-options">
        <label class="checkbox-label">
          <input
            v-model="form.rememberMe"
            type="checkbox"
            :disabled="isLoading"
            class="checkbox-input"
          >
          <span class="checkbox-text">Remember me</span>
        </label>

        <router-link
          to="/auth/forgot-password"
          class="forgot-password-link"
        >
          Forgot password?
        </router-link>
      </div>

      <!-- Global error -->
      <div v-if="error" class="form-error form-error--global">
        {{ error }}
      </div>

      <!-- Submit button -->
      <button
        type="submit"
        :disabled="isLoading || !isFormValid"
        class="form-submit"
      >
        <span v-if="isLoading" class="loading-spinner"></span>
        {{ isLoading ? 'Signing In...' : 'Sign In' }}
      </button>
    </form>

    <!-- Sign up link -->
    <div class="form-footer">
      <p>
        Don't have an account?
        <router-link to="/auth/register" class="signup-link">
          Sign up here
        </router-link>
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive } from 'vue';
import { useAuth } from '../composables/useAuth';
import type { LoginRequest } from '../types';

// Props and emits
interface Props {
  redirectTo?: string;
}

interface Emits {
  (e: 'success', user: any): void;
  (e: 'error', error: string): void;
}

const props = withDefaults(defineProps<Props>(), {
  redirectTo: '/dashboard'
});

const emit = defineEmits<Emits>();

// Auth composable
const { login, isLoading, error, clearError } = useAuth({
  redirectOnLogin: props.redirectTo
});

// Form state
const form = reactive({
  email: '',
  password: '',
  rememberMe: false
});

const errors = reactive({
  email: '',
  password: ''
});

const showPassword = ref(false);

// Validation
const validateEmail = () => {
  errors.email = '';

  if (!form.email) {
    errors.email = 'Email is required';
    return false;
  }

  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  if (!emailRegex.test(form.email)) {
    errors.email = 'Please enter a valid email address';
    return false;
  }

  return true;
};

const validatePassword = () => {
  errors.password = '';

  if (!form.password) {
    errors.password = 'Password is required';
    return false;
  }

  if (form.password.length < 8) {
    errors.password = 'Password must be at least 8 characters';
    return false;
  }

  return true;
};

const isFormValid = computed(() => {
  return form.email &&
         form.password &&
         !errors.email &&
         !errors.password &&
         !isLoading.value;
});

// Form submission
const handleSubmit = async () => {
  clearError();

  // Validate all fields
  const emailValid = validateEmail();
  const passwordValid = validatePassword();

  if (!emailValid || !passwordValid) {
    return;
  }

  // Prepare login data
  const loginData: LoginRequest = {
    email: form.email.trim().toLowerCase(),
    password: form.password
  };

  try {
    const response = await login(loginData);

    if (response) {
      emit('success', response.user);
    } else {
      // Error is handled by the composable and shown in template
      emit('error', error.value || 'Login failed');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Login failed';
    emit('error', message);
  }
};

// Auto-focus email field on mount
import { onMounted } from 'vue';

onMounted(() => {
  const emailInput = document.getElementById('email');
  emailInput?.focus();
});
</script>

<style scoped>
.login-form {
  max-width: 400px;
  margin: 0 auto;
  padding: 2rem;
  background: white;
  border-radius: 8px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
}

.form-header {
  text-align: center;
  margin-bottom: 2rem;
}

.form-header h2 {
  margin: 0 0 0.5rem 0;
  color: #1f2937;
  font-size: 1.5rem;
  font-weight: 600;
}

.form-header p {
  margin: 0;
  color: #6b7280;
  font-size: 0.9rem;
}

.form {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.form-label {
  font-weight: 500;
  color: #374151;
  font-size: 0.9rem;
}

.form-input {
  padding: 0.75rem;
  border: 1px solid #d1d5db;
  border-radius: 6px;
  font-size: 1rem;
  transition: border-color 0.2s, box-shadow 0.2s;
}

.form-input:focus {
  outline: none;
  border-color: #3b82f6;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

.form-input--error {
  border-color: #ef4444;
}

.form-input--error:focus {
  border-color: #ef4444;
  box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.1);
}

.form-input:disabled {
  background-color: #f3f4f6;
  cursor: not-allowed;
  opacity: 0.6;
}

.password-input-wrapper {
  position: relative;
}

.password-toggle {
  position: absolute;
  right: 0.75rem;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  font-size: 1rem;
  color: #6b7280;
  padding: 0;
}

.password-toggle:hover {
  color: #374151;
}

.password-toggle:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.form-options {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.9rem;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
  color: #374151;
}

.checkbox-input {
  margin: 0;
}

.forgot-password-link {
  color: #3b82f6;
  text-decoration: none;
}

.forgot-password-link:hover {
  text-decoration: underline;
}

.form-error {
  color: #ef4444;
  font-size: 0.8rem;
  margin-top: 0.25rem;
}

.form-error--global {
  padding: 0.75rem;
  background-color: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 6px;
  text-align: center;
  margin-top: 0;
}

.form-submit {
  padding: 0.75rem;
  background-color: #3b82f6;
  color: white;
  border: none;
  border-radius: 6px;
  font-size: 1rem;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.2s;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
}

.form-submit:hover:not(:disabled) {
  background-color: #2563eb;
}

.form-submit:disabled {
  background-color: #9ca3af;
  cursor: not-allowed;
}

.loading-spinner {
  width: 1rem;
  height: 1rem;
  border: 2px solid transparent;
  border-top: 2px solid currentColor;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.form-footer {
  margin-top: 1.5rem;
  text-align: center;
  font-size: 0.9rem;
  color: #6b7280;
}

.signup-link {
  color: #3b82f6;
  text-decoration: none;
  font-weight: 500;
}

.signup-link:hover {
  text-decoration: underline;
}

/* Responsive design */
@media (max-width: 640px) {
  .login-form {
    margin: 1rem;
    padding: 1.5rem;
  }

  .form-options {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }
}
</style>
