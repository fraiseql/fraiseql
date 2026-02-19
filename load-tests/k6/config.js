/**
 * Shared K6 Configuration
 *
 * Provides:
 * - Environment-based endpoint configuration
 * - Default headers and authentication setup
 * - Common helper functions for GraphQL requests
 * - Response validation utilities
 */

export const defaultOptions = {
  http: {
    timeout: "30s",
  },
};

/**
 * Get the GraphQL endpoint from environment or default
 * @returns {string} The base URL for the GraphQL endpoint
 */
export function getEndpoint() {
  return __ENV.ENDPOINT || "http://localhost:8000/graphql";
}

/**
 * Get authentication token from environment
 * @returns {string|null} The auth token or null if not configured
 */
export function getAuthToken() {
  return __ENV.AUTH_TOKEN || null;
}

/**
 * Build default request headers
 * @param {Object} overrides - Additional headers to merge
 * @returns {Object} Merged header object
 */
export function getHeaders(overrides = {}) {
  const headers = {
    "Content-Type": "application/json",
    "User-Agent": "k6/fraiseql-load-test",
    ...overrides,
  };

  const token = getAuthToken();
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  return headers;
}

/**
 * Execute a GraphQL query
 * @param {string} query - The GraphQL query string
 * @param {Object} variables - Query variables
 * @param {Object} options - k6 http options
 * @returns {Object} The k6 response object
 */
export function executeGraphQL(query, variables = {}, options = {}) {
  const http = require("k6/http");
  const payload = JSON.stringify({
    query,
    variables,
  });

  const mergedOptions = {
    headers: getHeaders(),
    ...options,
  };

  return http.post(getEndpoint(), payload, mergedOptions);
}

/**
 * Execute a GraphQL query with APQ (Automatic Persisted Query)
 * Sends query hash first, falls back to full query if not found
 *
 * @param {string} query - The GraphQL query string
 * @param {Object} variables - Query variables
 * @param {Object} options - k6 http options
 * @returns {Object} The k6 response object
 */
export function executeGraphQLWithAPQ(query, variables = {}, options = {}) {
  const http = require("k6/http");
  const crypto = require("k6/crypto");

  // Generate SHA256 hash of the query
  const queryHash = crypto.sha256(query, "hex");

  // First attempt: send only the hash (APQ cache hit)
  let payload = JSON.stringify({
    extensions: {
      persistedQuery: {
        version: 1,
        sha256Hash: queryHash,
      },
    },
    variables,
  });

  const mergedOptions = {
    headers: getHeaders(),
    ...options,
  };

  let response = http.post(getEndpoint(), payload, mergedOptions);

  // If we get PersistedQueryNotFound, send full query for registration
  if (
    response.status === 200 &&
    response.body.includes("PersistedQueryNotFound")
  ) {
    payload = JSON.stringify({
      query,
      extensions: {
        persistedQuery: {
          version: 1,
          sha256Hash: queryHash,
        },
      },
      variables,
    });
    response = http.post(getEndpoint(), payload, mergedOptions);
  }

  return response;
}

/**
 * Validate GraphQL response structure
 * @param {Object} response - The k6 response object
 * @param {Object} checks - check() function from k6
 * @returns {boolean} True if response is valid
 */
export function validateGraphQLResponse(response, checks) {
  if (!checks) {
    throw new Error("checks parameter required - pass k6 checks() function");
  }

  let data;
  try {
    data = JSON.parse(response.body);
  } catch (e) {
    checks({
      "graphql response is valid json": false,
    });
    return false;
  }

  const isValid = checks({
    "graphql response is valid json": data !== null,
    "graphql response has data or errors": data.data || data.errors,
    "graphql no unexpected errors": !data.errors || data.errors.length === 0,
  });

  return isValid;
}

/**
 * Generate a random string of specified length
 * Useful for creating unique test data
 *
 * @param {number} length - Length of the string
 * @returns {string} Random string
 */
export function randomString(length = 10) {
  const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
  let result = "";
  for (let i = 0; i < length; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
}

/**
 * Generate a random email
 * @returns {string} A random email address
 */
export function randomEmail() {
  return `test-${randomString(8)}@example.com`;
}

/**
 * Generate a random number within range
 * @param {number} min - Minimum value (inclusive)
 * @param {number} max - Maximum value (inclusive)
 * @returns {number} Random integer in range
 */
export function randomInt(min, max) {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

/**
 * Generate a random ID from a list
 * Useful for selecting random resources in tests
 *
 * @param {Array} items - Array to pick from
 * @returns {*} Random item from array
 */
export function randomChoice(items) {
  if (!Array.isArray(items) || items.length === 0) {
    throw new Error("randomChoice requires non-empty array");
  }
  return items[Math.floor(Math.random() * items.length)];
}

/**
 * Sleep for a given duration (in milliseconds)
 * @param {number} ms - Milliseconds to sleep
 */
export function sleep(ms) {
  const time = require("k6");
  time.sleep(ms / 1000);
}

/**
 * Parse response timing information for metrics
 * @param {Object} response - The k6 response object
 * @returns {Object} Timing metrics
 */
export function extractTimings(response) {
  return {
    bodySize: response.body.length,
    timeToFirstByte: response.timings.waiting,
    totalTime: response.timings.duration,
  };
}

/**
 * Check if response status is in success range
 * @param {number} status - HTTP status code
 * @returns {boolean} True if 200-299
 */
export function isSuccess(status) {
  return status >= 200 && status < 300;
}

/**
 * Check if response status indicates client error
 * @param {number} status - HTTP status code
 * @returns {boolean} True if 400-499
 */
export function isClientError(status) {
  return status >= 400 && status < 500;
}

/**
 * Check if response status indicates server error
 * @param {number} status - HTTP status code
 * @returns {boolean} True if 500-599
 */
export function isServerError(status) {
  return status >= 500 && status < 600;
}
