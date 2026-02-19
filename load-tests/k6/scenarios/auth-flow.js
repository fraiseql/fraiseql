/**
 * Authentication Flow Load Test
 *
 * Tests authentication endpoints under stress:
 * - Login operations
 * - Token refresh
 * - Logout operations
 * - Session validation
 *
 * These operations are typically rate-limited and have strict latency
 * requirements for security reasons. Tests varying load patterns.
 */

import http from "k6/http";
import { check, group, sleep } from "k6";
import {
  getEndpoint,
  getHeaders,
  executeGraphQL,
  validateGraphQLResponse,
  randomInt,
  randomEmail,
  randomString,
} from "../config.js";
import { tightThresholds } from "../thresholds.js";

// Authentication mutations
const AUTH_MUTATIONS = {
  login: `
    mutation Login($email: String!, $password: String!) {
      login(email: $email, password: $password) {
        success
        token
        refreshToken
        expiresIn
        user {
          id
          name
          email
        }
      }
    }
  `,

  refreshToken: `
    mutation RefreshToken($refreshToken: String!) {
      refreshToken(token: $refreshToken) {
        success
        token
        expiresIn
      }
    }
  `,

  logout: `
    mutation Logout {
      logout {
        success
        message
      }
    }
  `,

  validateSession: `
    query ValidateSession {
      currentUser {
        id
        name
        email
        roles
      }
    }
  `,

  changePassword: `
    mutation ChangePassword($oldPassword: String!, $newPassword: String!) {
      changePassword(oldPassword: $oldPassword, newPassword: $newPassword) {
        success
        message
      }
    }
  `,
};

// Test data
const TEST_CREDENTIALS = [
  { email: "user1@test.com", password: "password123" },
  { email: "user2@test.com", password: "password123" },
  { email: "user3@test.com", password: "password123" },
  { email: "user4@test.com", password: "password123" },
  { email: "user5@test.com", password: "password123" },
];

// Store tokens for token refresh operations
const tokenStore = {};

function getTestCredentials() {
  return TEST_CREDENTIALS[Math.floor(Math.random() * TEST_CREDENTIALS.length)];
}

function executeAuthMutation(mutationType, variables = {}) {
  const mutation = AUTH_MUTATIONS[mutationType];

  const response = executeGraphQL(mutation, variables, {
    tags: { type: "auth", authOp: mutationType },
  });

  return response;
}

function executeAuthQuery(queryType) {
  const query = AUTH_MUTATIONS[queryType];

  const response = executeGraphQL(query, {}, {
    tags: { type: "auth", authOp: queryType },
  });

  return response;
}

// Test options
export const options = {
  thresholds: tightThresholds,

  scenarios: {
    // Baseline auth traffic
    baseline_auth: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      preAllocatedVUs: 50,
      maxVUs: 150,
      stages: [
        { duration: "1m", target: 50 }, // Warm up
        { duration: "3m", target: 100 }, // Sustain typical auth load
        { duration: "1m", target: 0 }, // Cool down
      ],
    },

    // Auth brute force stress test
    brute_force_stress: {
      executor: "ramping-arrival-rate",
      startRate: 20,
      timeUnit: "1s",
      preAllocatedVUs: 100,
      maxVUs: 300,
      stages: [
        { duration: "30s", target: 100 }, // Warm up
        { duration: "1m", target: 300 }, // Burst of login attempts
        { duration: "1m", target: 300 }, // Sustain high load
        { duration: "30s", target: 0 }, // Cool down
      ],
    },

    // Token refresh burst (happens when many tokens near expiry)
    token_refresh_burst: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      preAllocatedVUs: 80,
      maxVUs: 250,
      stages: [
        { duration: "1m", target: 50 },
        { duration: "30s", target: 200 }, // Sudden refresh spike
        { duration: "1m", target: 200 },
        { duration: "30s", target: 0 },
      ],
    },
  },
};

export default function () {
  // Simulate realistic auth flow: mostly validation, some login/refresh
  const operation = Math.random();

  if (operation < 0.6) {
    // 60% - Session validation (cheapest operation)
    group("Auth: Validate Session", () => {
      const response = executeAuthQuery("validateSession");

      check(response, {
        "status is 200": (r) => r.status === 200,
        "validation is fast": (r) => r.timings.duration < 50,
        "has current user": (r) => r.body.includes("currentUser"),
      });

      validateGraphQLResponse(response, check);
    });
  } else if (operation < 0.85) {
    // 25% - Login attempts
    group("Auth: Login", () => {
      const creds = getTestCredentials();

      const response = executeAuthMutation("login", {
        email: creds.email,
        password: creds.password,
      });

      check(response, {
        "status is 200": (r) => r.status === 200,
        "login succeeds": (r) => r.body.includes("token"),
        "login is reasonably fast": (r) => r.timings.duration < 200,
      });

      validateGraphQLResponse(response, check);

      // Store token for potential refresh
      try {
        const data = JSON.parse(response.body);
        if (data.data?.login?.token) {
          tokenStore[creds.email] = {
            token: data.data.login.token,
            refreshToken: data.data.login.refreshToken,
          };
        }
      } catch (e) {
        // Ignore parsing errors
      }
    });
  } else {
    // 15% - Token refresh
    group("Auth: Token Refresh", () => {
      const creds = getTestCredentials();
      const stored = tokenStore[creds.email];

      if (stored?.refreshToken) {
        const response = executeAuthMutation("refreshToken", {
          refreshToken: stored.refreshToken,
        });

        check(response, {
          "status is 200": (r) => r.status === 200,
          "refresh succeeds": (r) => r.body.includes("token"),
          "refresh is fast": (r) => r.timings.duration < 100,
        });

        validateGraphQLResponse(response, check);

        // Update stored token
        try {
          const data = JSON.parse(response.body);
          if (data.data?.refreshToken?.token) {
            tokenStore[creds.email].token = data.data.refreshToken.token;
          }
        } catch (e) {
          // Ignore parsing errors
        }
      } else {
        // No stored token, do login instead
        const response = executeAuthMutation("login", {
          email: creds.email,
          password: creds.password,
        });

        check(response, {
          "status is 200": (r) => r.status === 200,
        });
      }
    });
  }

  sleep(Math.random() * 0.1);
}

/**
 * Setup function - Initialize test state
 */
export function setup() {
  console.log("Authentication Flow Load Test");
  console.log(`Endpoint: ${getEndpoint()}`);
  console.log("Test Operations:");
  console.log("  60% - Session validation (cheapest)");
  console.log("  25% - Login attempts");
  console.log("  15% - Token refresh");
  console.log("\nTest Credentials:");
  TEST_CREDENTIALS.forEach((cred) => {
    console.log(`  ${cred.email}:${cred.password}`);
  });

  return { credentials: TEST_CREDENTIALS };
}

/**
 * Teardown function
 */
export function teardown(data) {
  console.log("Authentication Flow Load Test Complete");
  console.log(`Tokens issued during test: ${Object.keys(tokenStore).length}`);
}
