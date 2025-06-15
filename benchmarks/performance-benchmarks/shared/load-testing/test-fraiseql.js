import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

// Custom metrics
const requestDuration = new Trend('graphql_request_duration');
const requestErrors = new Counter('graphql_request_errors');

export const options = {
  stages: [
    { duration: '30s', target: 10 }, // Ramp up
    { duration: '1m', target: 10 },  // Stay at 10 users
    { duration: '30s', target: 0 },  // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'], // 95% of requests should be below 500ms
    http_req_failed: ['rate<0.1'],    // Error rate should be below 10%
  },
};

const FRAISEQL_URL = 'http://fraiseql:8000/graphql';

// Simple queries for testing
const queries = {
  introspection: `{
    __schema {
      queryType {
        fields {
          name
          description
        }
      }
    }
  }`,

  simpleProducts: `{
    products {
      id
      name
      price
    }
  }`,

  simpleUsers: `{
    users {
      id
      email
      username
    }
  }`,
};

export default function () {
  // Test introspection query
  const introspectionRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.introspection }),
    {
      headers: { 'Content-Type': 'application/json' },
    }
  );

  check(introspectionRes, {
    'introspection status is 200': (r) => r.status === 200,
    'introspection has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.__schema;
    },
  });

  requestDuration.add(introspectionRes.timings.duration);

  // Test products query
  const productsRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.simpleProducts }),
    {
      headers: { 'Content-Type': 'application/json' },
    }
  );

  const productsSuccess = check(productsRes, {
    'products status is 200': (r) => r.status === 200,
  });

  if (!productsSuccess) {
    requestErrors.add(1);
    console.log('Products query failed:', productsRes.body);
  }

  // Test users query
  const usersRes = http.post(
    FRAISEQL_URL,
    JSON.stringify({ query: queries.simpleUsers }),
    {
      headers: { 'Content-Type': 'application/json' },
    }
  );

  const usersSuccess = check(usersRes, {
    'users status is 200': (r) => r.status === 200,
  });

  if (!usersSuccess) {
    requestErrors.add(1);
    console.log('Users query failed:', usersRes.body);
  }

  sleep(1);
}
