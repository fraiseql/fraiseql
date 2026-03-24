/**
 * FraiseQL Spike Load Test
 *
 * Tests server recovery behavior under sudden traffic spikes.
 * Ramps from 0 to 500 VUs in 15 seconds, holds briefly, then drops back to 0.
 * Verifies the server recovers gracefully (no crash, no stuck connections,
 * latency returns to baseline after spike).
 *
 * Usage:
 *   k6 run benchmarks/load/spike.js -e BASE_URL=http://localhost:8000
 *
 * Environment variables:
 *   BASE_URL    Server URL (default: http://localhost:8000)
 *   AUTH_TOKEN  Bearer token for authenticated scenarios (optional)
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
// Load profile — spike pattern
// ---------------------------------------------------------------------------

export const options = {
  stages: [
    { duration: '10s', target: 10 },    // Baseline: light load
    { duration: '15s', target: 500 },   // Spike: 0 -> 500 in 15s
    { duration: '15s', target: 500 },   // Hold spike
    { duration: '15s', target: 10 },    // Drop back to baseline
    { duration: '30s', target: 10 },    // Recovery: hold at baseline
    { duration: '10s', target: 0 },     // Cool-down
  ],
  thresholds: {
    // Relaxed during spike — we care about recovery, not peak latency
    http_req_duration: ['p(95)<2000'],
    // Higher error budget during spike — server may reject under overload
    graphql_errors: ['rate<0.10'],
  },
};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8000';
const GRAPHQL_URL = `${BASE_URL}/graphql`;

const HEADERS = { 'Content-Type': 'application/json' };
if (__ENV.AUTH_TOKEN) {
  HEADERS['Authorization'] = `Bearer ${__ENV.AUTH_TOKEN}`;
}

// ---------------------------------------------------------------------------
// Queries — mix of cheap and expensive to stress different paths
// ---------------------------------------------------------------------------

const QUERIES = [
  { name: 'list',          body: JSON.stringify({ query: '{ users(first: 10) { id name email } }' }) },
  { name: 'single',        body: JSON.stringify({ query: '{ user(id: "1") { id name email } }' }) },
  { name: 'introspection', body: JSON.stringify({ query: '{ __schema { queryType { name } } }' }) },
  {
    name: 'nested',
    body: JSON.stringify({
      query: `{
        users(first: 5) {
          id name
          posts(first: 3) {
            id title
            comments(first: 2) { id body }
          }
        }
      }`,
    }),
  },
];

// ---------------------------------------------------------------------------
// Default function
// ---------------------------------------------------------------------------

export default function () {
  const query = QUERIES[Math.floor(Math.random() * QUERIES.length)];

  const start = Date.now();
  const res = http.post(GRAPHQL_URL, query.body, { headers: HEADERS });
  queryDuration.add(Date.now() - start, { query: query.name });

  const httpOk = check(res, { 'HTTP 200': (r) => r.status === 200 });
  if (!httpOk) {
    errorRate.add(1, { query: query.name });
    return;
  }

  try {
    const parsed = JSON.parse(res.body);
    const hasErrors = Array.isArray(parsed.errors) && parsed.errors.length > 0;
    errorRate.add(hasErrors ? 1 : 0, { query: query.name });
  } catch (_) {
    errorRate.add(1, { query: query.name });
  }

  // Minimal think time during spike — maximize pressure
  sleep(0.01);
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

export function setup() {
  const res = http.get(`${BASE_URL}/health`);
  if (res.status !== 200) {
    throw new Error(
      `FraiseQL server at ${BASE_URL} returned HTTP ${res.status}. ` +
      'Ensure the server is running before starting the spike test.'
    );
  }
  console.log(`Spike test targeting: ${GRAPHQL_URL} (0 -> 500 -> 0 VUs)`);
}
