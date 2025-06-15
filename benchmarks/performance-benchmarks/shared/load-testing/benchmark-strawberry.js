import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

// Custom metrics
const queryDuration = new Trend('query_duration');
const requestErrors = new Counter('request_errors');

export const options = {
  stages: [
    { duration: '15s', target: 10 }, // Ramp up
    { duration: '30s', target: 10 }, // Stay at 10 users
    { duration: '15s', target: 0 },  // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'], // 95% of requests under 500ms
    http_req_failed: ['rate<0.1'],    // Error rate under 10%
  },
};

const STRAWBERRY_URL = 'http://benchmark-strawberry:8000/graphql';

// Test queries
const queries = {
  simpleUsers: `{
    users(limit: 10) {
      id
      email
      username
    }
  }`,

  simpleProducts: `{
    products(pagination: { limit: 10 }) {
      id
      name
      price
    }
  }`,

  nestedUsersOrders: `{
    users(limit: 5) {
      id
      email
      orders(limit: 3) {
        id
        status
        totalAmount
      }
    }
  }`,

  nestedProductsReviews: `{
    products(pagination: { limit: 5 }) {
      id
      name
      reviews(limit: 3) {
        id
        rating
        comment
      }
    }
  }`,
};

export default function () {
  // Test simple users query
  const simpleUsersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.simpleUsers }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(simpleUsersRes, {
    'simple users status 200': (r) => r.status === 200,
    'simple users has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.users && body.data.users.length > 0;
    },
  });

  queryDuration.add(simpleUsersRes.timings.duration, { query: 'simpleUsers' });

  sleep(0.5);

  // Test simple products query
  const simpleProductsRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.simpleProducts }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(simpleProductsRes, {
    'simple products status 200': (r) => r.status === 200,
    'simple products has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.products && body.data.products.length > 0;
    },
  });

  queryDuration.add(simpleProductsRes.timings.duration, { query: 'simpleProducts' });

  sleep(0.5);

  // Test nested users with orders
  const nestedUsersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.nestedUsersOrders }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(nestedUsersRes, {
    'nested users status 200': (r) => r.status === 200,
  });

  queryDuration.add(nestedUsersRes.timings.duration, { query: 'nestedUsersOrders' });

  sleep(0.5);

  // Test nested products with reviews
  const nestedProductsRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.nestedProductsReviews }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  check(nestedProductsRes, {
    'nested products status 200': (r) => r.status === 200,
  });

  queryDuration.add(nestedProductsRes.timings.duration, { query: 'nestedProductsReviews' });

  sleep(1);
}

export function handleSummary(data) {
  console.log('=== BENCHMARK SUMMARY ===\n');

  // Extract query durations
  const metrics = data.metrics.query_duration;
  if (metrics && metrics.values) {
    console.log('Query Performance:');
    console.log(`  Average: ${metrics.values.avg.toFixed(2)}ms`);
    console.log(`  Median: ${metrics.values.med.toFixed(2)}ms`);
    console.log(`  95th percentile: ${metrics.values['p(95)'].toFixed(2)}ms`);
    console.log(`  99th percentile: ${metrics.values['p(99)'].toFixed(2)}ms`);
  }

  // Success rate
  const checks = data.metrics.checks;
  if (checks && checks.values) {
    console.log(`\nSuccess Rate: ${(checks.values.rate * 100).toFixed(2)}%`);
  }

  // Request rate
  const httpReqs = data.metrics.http_reqs;
  if (httpReqs && httpReqs.values) {
    console.log(`Request Rate: ${httpReqs.values.rate.toFixed(2)} req/s`);
  }

  return {
    '/results/strawberry-benchmark.json': JSON.stringify(data, null, 2),
  };
}
