/**
 * FraiseQL Basic Load Test
 *
 * Measures steady-state throughput and latency under sustained concurrent load.
 * Designed to run against a local or CI-hosted FraiseQL server with a test schema.
 *
 * Usage:
 *   k6 run benchmarks/load/basic.js
 *
 * Targets (enforced as thresholds):
 *   - p(99) latency < 500 ms under 50 concurrent users
 *   - Error rate < 1%
 *
 * Environment variables:
 *   BASE_URL   Server URL (default: http://localhost:8000)
 *   AUTH_TOKEN Bearer token if the schema requires authentication (optional)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// ---------------------------------------------------------------------------
// Custom metrics
// ---------------------------------------------------------------------------

const errorRate = new Rate('graphql_errors');
const queryDuration = new Trend('graphql_query_duration', true);

// ---------------------------------------------------------------------------
// Load profile
// ---------------------------------------------------------------------------

export const options = {
  stages: [
    { duration: '10s', target: 10 },  // Warm-up: ramp to 10 VUs
    { duration: '30s', target: 50 },  // Sustained load: ramp to 50 VUs
    { duration: '10s', target: 0 },   // Cool-down: ramp back to 0
  ],
  thresholds: {
    // P99 latency must stay under 500 ms
    http_req_duration: ['p(99)<500'],
    // GraphQL-level errors (not just HTTP errors) must be < 1%
    graphql_errors: ['rate<0.01'],
  },
};

// ---------------------------------------------------------------------------
// Queries — representative workload
// ---------------------------------------------------------------------------

const QUERIES = [
  // Simple list query (most common pattern)
  {
    name: 'list_users',
    body: JSON.stringify({
      query: '{ users(first: 10) { id name email } }',
    }),
  },
  // Single entity fetch (node pattern)
  {
    name: 'single_user',
    body: JSON.stringify({
      query: '{ user(id: "1") { id name email } }',
    }),
  },
  // Introspection (used by tooling; should be fast)
  {
    name: 'introspection',
    body: JSON.stringify({
      query: '{ __schema { queryType { name } } }',
    }),
  },
];

// ---------------------------------------------------------------------------
// Test setup
// ---------------------------------------------------------------------------

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8000';
const GRAPHQL_URL = `${BASE_URL}/graphql`;

const HEADERS = {
  'Content-Type': 'application/json',
};

if (__ENV.AUTH_TOKEN) {
  HEADERS['Authorization'] = `Bearer ${__ENV.AUTH_TOKEN}`;
}

// ---------------------------------------------------------------------------
// Default function — runs once per virtual user iteration
// ---------------------------------------------------------------------------

export default function () {
  // Cycle through query types
  const query = QUERIES[Math.floor(Math.random() * QUERIES.length)];

  const start = Date.now();
  const res = http.post(GRAPHQL_URL, query.body, { headers: HEADERS });
  const duration = Date.now() - start;

  queryDuration.add(duration, { query: query.name });

  // Validate HTTP response
  const httpOk = check(res, {
    'HTTP 200': (r) => r.status === 200,
  });

  if (!httpOk) {
    errorRate.add(1, { query: query.name });
    return;
  }

  // Validate GraphQL response — absence of "errors" key
  let body;
  try {
    body = JSON.parse(res.body);
  } catch (_) {
    errorRate.add(1, { query: query.name });
    return;
  }

  const hasGraphQLErrors = Array.isArray(body.errors) && body.errors.length > 0;
  errorRate.add(hasGraphQLErrors ? 1 : 0, { query: query.name });

  // Small think-time between requests (realistic user behaviour)
  sleep(0.1);
}

// ---------------------------------------------------------------------------
// Setup — verify the server is reachable before starting load
// ---------------------------------------------------------------------------

export function setup() {
  const res = http.get(`${BASE_URL}/health`);
  if (res.status !== 200) {
    throw new Error(
      `FraiseQL server at ${BASE_URL} returned HTTP ${res.status}. ` +
      'Ensure the server is running before starting the load test.'
    );
  }
  console.log(`Load test targeting: ${GRAPHQL_URL}`);
}
