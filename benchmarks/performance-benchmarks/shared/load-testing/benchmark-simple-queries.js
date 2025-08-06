import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

// Custom metrics
const fraiseqlDuration = new Trend('fraiseql_duration');
const strawberryDuration = new Trend('strawberry_duration');
const requestErrors = new Counter('request_errors');

export const options = {
  stages: [
    { duration: '30s', target: 20 }, // Ramp up to 20 users
    { duration: '2m', target: 20 },  // Stay at 20 users
    { duration: '30s', target: 0 },  // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<1000'], // 95% of requests under 1s
    http_req_failed: ['rate<0.1'],     // Error rate under 10%
  },
};

const FRAISEQL_URL = 'http://benchmark-fraiseql:8000/graphql';
const STRAWBERRY_URL = 'http://benchmark-strawberry:8000/graphql';

// Simple queries
const queries = {
  users: `{
    users(limit: 10) {
      id
      email
      username
      fullName
      isActive
    }
  }`,

  products: `{
    products(pagination: { limit: 10 }) {
      id
      name
      description
      price
      stockQuantity
    }
  }`,

  orders: `{
    orders(limit: 10) {
      id
      status
      totalAmount
      createdAt
    }
  }`,
};

export default function () {
  // Test users query on both frameworks
  const fraiseqlUsersRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.users }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryUsersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.users }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  const fraiseqlUsersOk = check(fraiseqlUsersRes, {
    'FraiseQL users status 200': (r) => r.status === 200,
  });

  const strawberryUsersOk = check(strawberryUsersRes, {
    'Strawberry users status 200': (r) => r.status === 200,
  });

  if (!fraiseqlUsersOk) requestErrors.add(1, { framework: 'fraiseql', query: 'users' });
  if (!strawberryUsersOk) requestErrors.add(1, { framework: 'strawberry', query: 'users' });

  // Record durations
  fraiseqlDuration.add(fraiseqlUsersRes.timings.duration, { query: 'users' });
  strawberryDuration.add(strawberryUsersRes.timings.duration, { query: 'users' });

  sleep(0.5);

  // Test products query
  const fraiseqlProductsRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.products }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryProductsRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.products }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  const fraiseqlProductsOk = check(fraiseqlProductsRes, {
    'FraiseQL products status 200': (r) => r.status === 200,
  });

  const strawberryProductsOk = check(strawberryProductsRes, {
    'Strawberry products status 200': (r) => r.status === 200,
  });

  if (!fraiseqlProductsOk) requestErrors.add(1, { framework: 'fraiseql', query: 'products' });
  if (!strawberryProductsOk) requestErrors.add(1, { framework: 'strawberry', query: 'products' });

  // Record durations
  fraiseqlDuration.add(fraiseqlProductsRes.timings.duration, { query: 'products' });
  strawberryDuration.add(strawberryProductsRes.timings.duration, { query: 'products' });

  sleep(0.5);

  // Test orders query
  const fraiseqlOrdersRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.orders }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryOrdersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.orders }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  const fraiseqlOrdersOk = check(fraiseqlOrdersRes, {
    'FraiseQL orders status 200': (r) => r.status === 200,
  });

  const strawberryOrdersOk = check(strawberryOrdersRes, {
    'Strawberry orders status 200': (r) => r.status === 200,
  });

  if (!fraiseqlOrdersOk) requestErrors.add(1, { framework: 'fraiseql', query: 'orders' });
  if (!strawberryOrdersOk) requestErrors.add(1, { framework: 'strawberry', query: 'orders' });

  // Record durations
  fraiseqlDuration.add(fraiseqlOrdersRes.timings.duration, { query: 'orders' });
  strawberryDuration.add(strawberryOrdersRes.timings.duration, { query: 'orders' });

  sleep(1);
}
