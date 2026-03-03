/**
 * FraiseQL Mutation Load Test
 *
 * Tests write-path performance: mutations through fn_* PostgreSQL functions.
 * Run this separately from basic.js to isolate read vs. write latency profiles.
 *
 * Usage:
 *   k6 run benchmarks/load/mutations.js
 *
 * Environment variables:
 *   BASE_URL   Server URL (default: http://localhost:8000)
 *   AUTH_TOKEN Bearer token (required if schema uses authentication)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const errorRate = new Rate('mutation_errors');
const mutationDuration = new Trend('mutation_duration', true);

export const options = {
  stages: [
    { duration: '10s', target: 5 },   // Mutations are heavier; use fewer VUs
    { duration: '30s', target: 20 },
    { duration: '10s', target: 0 },
  ],
  thresholds: {
    http_req_duration: ['p(99)<1000'],  // Mutations allowed 1 s P99
    mutation_errors: ['rate<0.01'],
  },
};

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8000';
const GRAPHQL_URL = `${BASE_URL}/graphql`;

const HEADERS = {
  'Content-Type': 'application/json',
};

if (__ENV.AUTH_TOKEN) {
  HEADERS['Authorization'] = `Bearer ${__ENV.AUTH_TOKEN}`;
}

export default function () {
  const id = uuidv4();

  const res = http.post(
    GRAPHQL_URL,
    JSON.stringify({
      query: `
        mutation CreateUser($name: String!, $email: String!) {
          createUser(name: $name, email: $email) {
            status
            message
            entityId
          }
        }
      `,
      variables: {
        name: `Test User ${id.slice(0, 8)}`,
        email: `user-${id.slice(0, 8)}@example.com`,
      },
    }),
    { headers: HEADERS }
  );

  mutationDuration.add(res.timings.duration);

  const httpOk = check(res, { 'HTTP 200': (r) => r.status === 200 });

  if (!httpOk) {
    errorRate.add(1);
    return;
  }

  let body;
  try {
    body = JSON.parse(res.body);
  } catch (_) {
    errorRate.add(1);
    return;
  }

  const hasErrors = Array.isArray(body.errors) && body.errors.length > 0;
  errorRate.add(hasErrors ? 1 : 0);

  sleep(0.2);
}

export function setup() {
  const res = http.get(`${BASE_URL}/health`);
  if (res.status !== 200) {
    throw new Error(`Server at ${BASE_URL} not reachable (HTTP ${res.status})`);
  }
}
