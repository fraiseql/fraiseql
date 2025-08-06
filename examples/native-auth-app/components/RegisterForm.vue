<template>
  <div class="register-form">
    <div class="form-header">
      <h2>Create Account</h2>
      <p>Join us today! Please fill in your information to get started.</p>
    </div>

    <form @submit.prevent="handleSubmit" class="form">
      <!-- Name field -->
      <div class="form-group">
        <label for="name" class="form-label">
          Full Name
        </label>
        <input
          id="name"
          v-model="form.name"
          type="text"
          required
          autocomplete="name"
          :disabled="isLoading"
          :class="['form-input', { 'form-input--error': errors.name }]"
          placeholder="Your full name"
          @blur="validateName"
        >
        <span v-if="errors.name" class="form-error">
          {{ errors.name }}
        </span>
      </div>

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
            autocomplete="new-password"
            :disabled="isLoading"
            :class="['form-input', { 'form-input--error': errors.password }]"
            placeholder="Create a strong password"
            @blur="validatePassword"
            @input="onPasswordInput"
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

        <!-- Password strength indicator -->
        <div v-if="form.password" class="password-strength">
          <div class="strength-bar">
            <div
              class="strength-fill"
              :class="`strength-fill--${passwordStrength.level}`"
              :style="{ width: `${passwordStrength.score * 25}%` }"
            ></div>
          </div>
          <span class="strength-text">
            Password strength: {{ passwordStrength.level }}
          </span>
        </div>

        <!-- Password requirements -->
        <div v-if="form.password" class="password-requirements">
          <div
            v-for="requirement in passwordRequirements"
            :key="requirement.label"
            :class="['requirement', { 'requirement--met': requirement.met }]"
          >
            {{ requirement.met ? '✓' : '○' }} {{ requirement.label }}
          </div>
        </div>

        <span v-if="errors.password" class="form-error">
          {{ errors.password }}
        </span>
      </div>

      <!-- Confirm password field -->
      <div class="form-group">
        <label for="confirmPassword" class="form-label">
          Confirm Password
        </label>
        <input
          id="confirmPassword"
          v-model="form.confirmPassword"
          :type="showConfirmPassword ? 'text' : 'password'"
          required
          autocomplete="new-password"
          :disabled="isLoading"
          :class="['form-input', { 'form-input--error': errors.confirmPassword }]"
          placeholder="Confirm your password"
          @blur="validateConfirmPassword"
        >
        <button
          type="button"
          class="password-toggle"
          :disabled="isLoading"
          @click="showConfirmPassword = !showConfirmPassword"
        >
          {{ showConfirmPassword ? '👁️' : '👁️‍🗨️' }}
        </button>
        <span v-if="errors.confirmPassword" class="form-error">
          {{ errors.confirmPassword }}
        </span>
      </div>

      <!-- Terms and privacy -->
      <div class="form-group">
        <label class="checkbox-label">
          <input
            v-model="form.acceptTerms"
            type="checkbox"
            required
            :disabled="isLoading"
            class="checkbox-input"
          >
          <span class="checkbox-text">
            I agree to the
            <a href="/terms" target="_blank" class="terms-link">Terms of Service</a>
            and
            <a href="/privacy" target="_blank" class="terms-link">Privacy Policy</a>
          </span>
        </label>
        <span v-if="errors.acceptTerms" class="form-error">
          {{ errors.acceptTerms }}
        </span>
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
        {{ isLoading ? 'Creating Account...' : 'Create Account' }}
      </button>
    </form>

    <!-- Sign in link -->
    <div class="form-footer">
      <p>
        Already have an account?
        <router-link to="/auth/login" class="signin-link">
          Sign in here
        </router-link>
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive } from 'vue';
import { useAuth } from '../composables/useAuth';
import type { RegisterRequest } from '../types';

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
const { register, isLoading, error, clearError, validatePassword } = useAuth({
  redirectOnLogin: props.redirectTo
});

// Form state
const form = reactive({
  name: '',
  email: '',
  password: '',
  confirmPassword: '',
  acceptTerms: false
});

const errors = reactive({
  name: '',
  email: '',
  password: '',
  confirmPassword: '',
  acceptTerms: ''
});

const showPassword = ref(false);
const showConfirmPassword = ref(false);

// Password strength calculation
const passwordStrength = computed(() => {
  const password = form.password;
  if (!password) return { level: 'none', score: 0 };

  const validation = validatePassword(password);
  const requirements = passwordRequirements.value;
  const metCount = requirements.filter(r => r.met).length;

  let level = 'weak';
  let score = metCount;

  if (metCount >= 5) {
    level = 'strong';
    score = 4;
  } else if (metCount >= 3) {
    level = 'medium';
    score = 3;
  } else if (metCount >= 2) {
    level = 'fair';
    score = 2;
  }

  return { level, score };
});

const passwordRequirements = computed(() => [
  {
    label: 'At least 8 characters',
    met: form.password.length >= 8
  },
  {
    label: 'One uppercase letter',
    met: /[A-Z]/.test(form.password)
  },
  {
    label: 'One lowercase letter',
    met: /[a-z]/.test(form.password)
  },
  {
    label: 'One number',
    met: /\d/.test(form.password)
  },
  {
    label: 'One special character',
    met: /[!@#$%^&*(),.?":{}|<>]/.test(form.password)
  }
]);

// Validation functions
const validateName = () => {
  errors.name = '';

  if (!form.name.trim()) {
    errors.name = 'Name is required';
    return false;
  }

  if (form.name.trim().length < 2) {
    errors.name = 'Name must be at least 2 characters';
    return false;
  }

  return true;
};

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

  const validation = validatePassword(form.password);

  if (!validation.isValid) {
    errors.password = validation.errors[0]; // Show first error
    return false;
  }

  return true;
};

const validateConfirmPassword = () => {
  errors.confirmPassword = '';

  if (!form.confirmPassword) {
    errors.confirmPassword = 'Please confirm your password';
    return false;
  }

  if (form.password !== form.confirmPassword) {
    errors.confirmPassword = 'Passwords do not match';
    return false;
  }

  return true;
};

const validateTerms = () => {
  errors.acceptTerms = '';

  if (!form.acceptTerms) {
    errors.acceptTerms = 'You must accept the terms and conditions';
    return false;
  }

  return true;
};

const isFormValid = computed(() => {
  return form.name &&
         form.email &&
         form.password &&
         form.confirmPassword &&
         form.acceptTerms &&
         !errors.name &&
         !errors.email &&
         !errors.password &&
         !errors.confirmPassword &&
         !errors.acceptTerms &&
         !isLoading.value;
});

// Event handlers
const onPasswordInput = () => {
  // Clear password error when user starts typing
  errors.password = '';

  // Re-validate confirm password if it was entered
  if (form.confirmPassword) {
    validateConfirmPassword();
  }
};

// Form submission
const handleSubmit = async () => {
  clearError();

  // Validate all fields
  const nameValid = validateName();
  const emailValid = validateEmail();
  const passwordValid = validatePassword();
  const confirmPasswordValid = validateConfirmPassword();
  const termsValid = validateTerms();

  if (!nameValid || !emailValid || !passwordValid || !confirmPasswordValid || !termsValid) {
    return;
  }

  // Prepare registration data
  const registrationData: RegisterRequest = {
    name: form.name.trim(),
    email: form.email.trim().toLowerCase(),
    password: form.password
  };

  try {
    const response = await register(registrationData);

    if (response) {
      emit('success', response.user);
    } else {
      // Error is handled by the composable and shown in template
      emit('error', error.value || 'Registration failed');
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Registration failed';
    emit('error', message);
  }
};

// Auto-focus name field on mount
import { onMounted } from 'vue';

onMounted(() => {
  const nameInput = document.getElementById('name');
  nameInput?.focus();
});
</script>

<style scoped>
.register-form {
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

/* Password strength indicator */
.password-strength {
  margin-top: 0.5rem;
}

.strength-bar {
  width: 100%;
  height: 4px;
  background-color: #e5e7eb;
  border-radius: 2px;
  overflow: hidden;
}

.strength-fill {
  height: 100%;
  transition: width 0.3s, background-color 0.3s;
}

.strength-fill--weak { background-color: #ef4444; }
.strength-fill--fair { background-color: #f97316; }
.strength-fill--medium { background-color: #eab308; }
.strength-fill--strong { background-color: #22c55e; }

.strength-text {
  font-size: 0.8rem;
  color: #6b7280;
  margin-top: 0.25rem;
  display: block;
}

/* Password requirements */
.password-requirements {
  margin-top: 0.5rem;
  padding: 0.75rem;
  background-color: #f9fafb;
  border-radius: 4px;
  font-size: 0.8rem;
}

.requirement {
  color: #6b7280;
  margin-bottom: 0.25rem;
}

.requirement--met {
  color: #22c55e;
}

.requirement:last-child {
  margin-bottom: 0;
}

.checkbox-label {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  cursor: pointer;
  color: #374151;
  font-size: 0.9rem;
  line-height: 1.4;
}

.checkbox-input {
  margin: 0;
  margin-top: 0.1rem; /* Align with text baseline */
}

.terms-link {
  color: #3b82f6;
  text-decoration: none;
}

.terms-link:hover {
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

.signin-link {
  color: #3b82f6;
  text-decoration: none;
  font-weight: 500;
}

.signin-link:hover {
  text-decoration: underline;
}

/* Responsive design */
@media (max-width: 640px) {
  .register-form {
    margin: 1rem;
    padding: 1.5rem;
  }
}
</style>
