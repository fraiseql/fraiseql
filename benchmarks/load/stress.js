/**
 * FraiseQL Stress Load Test
 *
 * Simulates a realistic mixed workload at sustained high concurrency.
 * Tests 200 VUs for 5 minutes to detect memory leaks, connection pool
 * exhaustion, and degradation under prolonged pressure.
 *
 * Workload mix (weighted by scenario exec proportion):
 *   70% reads  — list + single entity + nested queries
 *   20% mutations — create + update operations
 *   10% introspection — tooling / schema queries
 *
 * Usage:
 *   k6 run benchmarks/load/stress.js -e BASE_URL=http://localhost:8000
 *
 * Environment variables:
 *   BASE_URL    Server URL (default: http://localhost:8000)
 *   AUTH_TOKEN  Bearer token for authenticated scenarios (optional)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// ---------------------------------------------------------------------------
// Custom metrics
// ---------------------------------------------------------------------------

const errorRate = new Rate('graphql_errors');
const queryDuration = new Trend('graphql_query_duration', true);
const mutationCount = new Counter('graphql_mutations');

// ---------------------------------------------------------------------------
// Load profile — sustained peak
// ---------------------------------------------------------------------------

export const options = {
  scenarios: {
    reads: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 140 },   // Ramp to 140 VUs (70%)
        { duration: '5m',  target: 140 },   // Sustain
        { duration: '30s', target: 0 },     // Cool-down
      ],
      exec: 'readWorkload',
    },
    mutations: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 40 },    // Ramp to 40 VUs (20%)
        { duration: '5m',  target: 40 },    // Sustain
        { duration: '30s', target: 0 },     // Cool-down
      ],
      exec: 'mutationWorkload',
    },
    introspection: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '30s', target: 20 },    // Ramp to 20 VUs (10%)
        { duration: '5m',  target: 20 },    // Sustain
        { duration: '30s', target: 0 },     // Cool-down
      ],
      exec: 'introspectionWorkload',
    },
  },
  thresholds: {
    http_req_duration: ['p(99)<1000'],       // Relaxed for sustained load
    graphql_errors: ['rate<0.02'],           // 2% error budget under stress
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
// Queries
// ---------------------------------------------------------------------------

const READ_QUERIES = [
  JSON.stringify({ query: '{ users(first: 10) { id name email } }' }),
  JSON.stringify({ query: '{ user(id: "1") { id name email } }' }),
  JSON.stringify({
    query: `{
      users(first: 5) {
        id name
        posts(first: 3) {
          id title
          comments(first: 3) { id body author { id name } }
        }
      }
    }`,
  }),
];

const MUTATION_QUERIES = [
  JSON.stringify({
    query: `mutation { createUser(input: { name: "k6-stress", email: "k6@test.local" }) { id } }`,
  }),
  JSON.stringify({
    query: `mutation { updateUser(id: "1", input: { name: "k6-updated" }) { id name } }`,
  }),
];

const INTROSPECTION_QUERIES = [
  JSON.stringify({ query: '{ __schema { queryType { name } mutationType { name } } }' }),
  JSON.stringify({ query: '{ __type(name: "User") { name fields { name type { name } } } }' }),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function sendQuery(body, label) {
  const start = Date.now();
  const res = http.post(GRAPHQL_URL, body, { headers: HEADERS });
  queryDuration.add(Date.now() - start, { query: label });

  const httpOk = check(res, { 'HTTP 200': (r) => r.status === 200 });
  if (!httpOk) {
    errorRate.add(1, { query: label });
    return;
  }

  try {
    const parsed = JSON.parse(res.body);
    const hasErrors = Array.isArray(parsed.errors) && parsed.errors.length > 0;
    errorRate.add(hasErrors ? 1 : 0, { query: label });
  } catch (_) {
    errorRate.add(1, { query: label });
  }
}

// ---------------------------------------------------------------------------
// Scenario functions
// ---------------------------------------------------------------------------

export function readWorkload() {
  const body = READ_QUERIES[Math.floor(Math.random() * READ_QUERIES.length)];
  sendQuery(body, 'read');
  sleep(0.05 + Math.random() * 0.1); // 50-150ms think time
}

export function mutationWorkload() {
  const body = MUTATION_QUERIES[Math.floor(Math.random() * MUTATION_QUERIES.length)];
  sendQuery(body, 'mutation');
  mutationCount.add(1);
  sleep(0.2 + Math.random() * 0.3); // 200-500ms think time (mutations are less frequent)
}

export function introspectionWorkload() {
  const body = INTROSPECTION_QUERIES[Math.floor(Math.random() * INTROSPECTION_QUERIES.length)];
  sendQuery(body, 'introspection');
  sleep(0.5 + Math.random() * 0.5); // 500ms-1s think time (tooling polls slowly)
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

export function setup() {
  const res = http.get(`${BASE_URL}/health`);
  if (res.status !== 200) {
    throw new Error(
      `FraiseQL server at ${BASE_URL} returned HTTP ${res.status}. ` +
      'Ensure the server is running before starting the stress test.'
    );
  }
  console.log(`Stress test targeting: ${GRAPHQL_URL} (200 VUs, 5 min sustained)`);
}
