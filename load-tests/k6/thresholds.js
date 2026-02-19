/**
 * K6 Performance Thresholds
 *
 * Defines pass/fail criteria for load tests:
 * - Latency: p50<10ms, p95<50ms, p99<200ms
 * - Error Rate: <1%
 * - Sustained throughput: >800 rps
 *
 * Thresholds are used by k6 to determine test success/failure.
 * A test fails if ANY threshold is not met.
 */

/**
 * Default thresholds for typical GraphQL queries
 * Use for read-heavy workloads
 */
export const defaultThresholds = {
  // HTTP Response Time - Percentiles
  "http_req_duration{type:query}": [
    { threshold: "p(50) < 10", abortOnFail: false },
    { threshold: "p(95) < 50", abortOnFail: false },
    { threshold: "p(99) < 200", abortOnFail: false },
  ],

  // Connection timings
  "http_req_connecting": [{ threshold: "p(95) < 5", abortOnFail: false }],
  "http_req_tls_handshaking": [{ threshold: "p(95) < 10", abortOnFail: false }],

  // Error rates
  "http_req_failed{type:query}": [
    { threshold: "rate < 0.01", abortOnFail: true }, // 1% error rate max
  ],

  // Success rate
  "http_reqs": [{ threshold: "count > 800", abortOnFail: false }], // Sustained throughput
};

/**
 * Aggressive thresholds for mutation operations
 * Mutations are typically slower than queries, so we relax some constraints
 */
export const mutationThresholds = {
  "http_req_duration{type:mutation}": [
    { threshold: "p(50) < 20", abortOnFail: false },
    { threshold: "p(95) < 100", abortOnFail: false },
    { threshold: "p(99) < 500", abortOnFail: false },
  ],

  "http_req_failed{type:mutation}": [
    { threshold: "rate < 0.01", abortOnFail: true }, // 1% error rate
  ],
};

/**
 * Tight thresholds for latency-sensitive operations (auth, health checks)
 */
export const tightThresholds = {
  "http_req_duration{type:auth}": [
    { threshold: "p(50) < 5", abortOnFail: false },
    { threshold: "p(95) < 20", abortOnFail: false },
    { threshold: "p(99) < 100", abortOnFail: false },
  ],

  "http_req_failed{type:auth}": [
    { threshold: "rate < 0.005", abortOnFail: true }, // 0.5% error rate
  ],
};

/**
 * Relaxed thresholds for spike/stress testing
 * Used when testing system behavior under extreme load
 */
export const spikeThresholds = {
  "http_req_duration": [
    { threshold: "p(95) < 500", abortOnFail: false }, // Much more relaxed
    { threshold: "p(99) < 1000", abortOnFail: false },
  ],

  "http_req_failed": [
    { threshold: "rate < 0.05", abortOnFail: false }, // 5% error rate acceptable
  ],
};

/**
 * APQ (Automatic Persisted Query) cache thresholds
 * Should show improvement on cache hits vs. misses
 */
export const apqThresholds = {
  // Cache hits should be significantly faster
  "http_req_duration{cache:hit}": [
    { threshold: "p(95) < 20", abortOnFail: false },
  ],

  // Cache misses are slower (query registration)
  "http_req_duration{cache:miss}": [
    { threshold: "p(95) < 100", abortOnFail: false },
  ],

  // Overall error rate
  "http_req_failed": [{ threshold: "rate < 0.01", abortOnFail: true }],
};

/**
 * Build combined thresholds object for k6 options
 *
 * @param {Object} thresholdsToMerge - Threshold objects to combine
 * @returns {Object} Combined thresholds in k6 format
 *
 * Example:
 *   const thresholds = combineThresholds(
 *     defaultThresholds,
 *     mutationThresholds
 *   );
 */
export function combineThresholds(...thresholdsToMerge) {
  const combined = {};

  for (const thresholdSet of thresholdsToMerge) {
    for (const [metric, constraints] of Object.entries(thresholdSet)) {
      if (combined[metric]) {
        // Merge with existing constraints for this metric
        combined[metric] = [...combined[metric], ...constraints];
      } else {
        combined[metric] = constraints;
      }
    }
  }

  return combined;
}

/**
 * Create thresholds for a specific scenario
 * Useful when different scenarios have different performance profiles
 *
 * @param {string} scenarioName - Name of the scenario
 * @param {Object} baseThresholds - Base thresholds to extend
 * @returns {Object} Scenario-specific thresholds
 */
export function createScenarioThresholds(scenarioName, baseThresholds) {
  const thresholds = {};

  for (const [metric, constraints] of Object.entries(baseThresholds)) {
    const scenarioMetric = `${metric}{scenario:${scenarioName}}`;
    thresholds[scenarioMetric] = constraints;
  }

  return thresholds;
}

/**
 * Get appropriate thresholds based on test type
 *
 * @param {string} testType - Type of test: "query", "mutation", "auth", "spike", "apq"
 * @returns {Object} Thresholds for the specified test type
 */
export function getThresholdsForTest(testType) {
  switch (testType) {
    case "query":
      return defaultThresholds;
    case "mutation":
      return mutationThresholds;
    case "auth":
      return tightThresholds;
    case "spike":
      return spikeThresholds;
    case "apq":
      return apqThresholds;
    default:
      return defaultThresholds;
  }
}

/**
 * Print threshold configuration for debugging
 * Useful to verify thresholds before running tests
 *
 * @param {Object} thresholds - Thresholds object
 */
export function printThresholds(thresholds) {
  console.log("Performance Thresholds:");
  console.log("======================");

  for (const [metric, constraints] of Object.entries(thresholds)) {
    console.log(`\n${metric}:`);
    for (const constraint of constraints) {
      console.log(`  ${constraint.threshold}`);
    }
  }
}
