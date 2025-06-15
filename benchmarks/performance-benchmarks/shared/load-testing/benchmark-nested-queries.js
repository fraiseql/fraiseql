import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

// Custom metrics
const fraiseqlDuration = new Trend('fraiseql_duration_nested');
const strawberryDuration = new Trend('strawberry_duration_nested');
const requestErrors = new Counter('request_errors_nested');

export const options = {
  stages: [
    { duration: '30s', target: 10 }, // Ramp up to 10 users
    { duration: '2m', target: 10 },  // Stay at 10 users
    { duration: '30s', target: 0 },  // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<2000'], // 95% of requests under 2s
    http_req_failed: ['rate<0.1'],     // Error rate under 10%
  },
};

const FRAISEQL_URL = 'http://benchmark-fraiseql:8000/graphql';
const STRAWBERRY_URL = 'http://benchmark-strawberry:8000/graphql';

// Nested queries that test N+1 query problems
const queries = {
  usersWithOrders: `{
    users(limit: 10) {
      id
      email
      username
      orders(limit: 5) {
        id
        status
        totalAmount
        orderItems {
          id
          quantity
          unitPrice
        }
      }
    }
  }`,

  productsWithReviews: `{
    products(pagination: { limit: 10 }) {
      id
      name
      price
      category {
        id
        name
      }
      reviews(limit: 5) {
        id
        rating
        comment
        user {
          id
          username
        }
      }
    }
  }`,

  ordersWithDetails: `{
    orders(limit: 10) {
      id
      status
      totalAmount
      user {
        id
        email
        username
      }
      orderItems {
        id
        quantity
        unitPrice
        product {
          id
          name
          price
        }
      }
    }
  }`,
};

export default function () {
  // Test users with orders query
  const fraiseqlUsersRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.usersWithOrders }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryUsersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.usersWithOrders }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  check(fraiseqlUsersRes, {
    'FraiseQL nested users status 200': (r) => r.status === 200,
  });

  check(strawberryUsersRes, {
    'Strawberry nested users status 200': (r) => r.status === 200,
  });

  // Record durations
  fraiseqlDuration.add(fraiseqlUsersRes.timings.duration, { query: 'usersWithOrders' });
  strawberryDuration.add(strawberryUsersRes.timings.duration, { query: 'usersWithOrders' });

  sleep(1);

  // Test products with reviews query
  const fraiseqlProductsRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.productsWithReviews }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryProductsRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.productsWithReviews }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  check(fraiseqlProductsRes, {
    'FraiseQL nested products status 200': (r) => r.status === 200,
  });

  check(strawberryProductsRes, {
    'Strawberry nested products status 200': (r) => r.status === 200,
  });

  // Record durations
  fraiseqlDuration.add(fraiseqlProductsRes.timings.duration, { query: 'productsWithReviews' });
  strawberryDuration.add(strawberryProductsRes.timings.duration, { query: 'productsWithReviews' });

  sleep(1);

  // Test orders with details query
  const fraiseqlOrdersRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.ordersWithDetails }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  const strawberryOrdersRes = http.post(
    STRAWBERRY_URL,
    JSON.stringify({ query: queries.ordersWithDetails }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  // Check responses
  check(fraiseqlOrdersRes, {
    'FraiseQL nested orders status 200': (r) => r.status === 200,
  });

  check(strawberryOrdersRes, {
    'Strawberry nested orders status 200': (r) => r.status === 200,
  });

  // Record durations
  fraiseqlDuration.add(fraiseqlOrdersRes.timings.duration, { query: 'ordersWithDetails' });
  strawberryDuration.add(strawberryOrdersRes.timings.duration, { query: 'ordersWithDetails' });

  sleep(2);
}
