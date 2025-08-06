import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');

// Test configuration
export const options = {
  stages: [
    { duration: '30s', target: 10 },   // Ramp up to 10 users
    { duration: '1m', target: 50 },    // Stay at 50 users
    { duration: '2m', target: 100 },   // Ramp up to 100 users
    { duration: '2m', target: 100 },   // Stay at 100 users
    { duration: '30s', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'], // 95% of requests should be below 500ms
    errors: ['rate<0.1'],              // Error rate should be below 10%
  },
};

const GRAPHQL_ENDPOINT = `${__ENV.TARGET}/graphql`;

// Simple queries
const queries = {
  getUsers: `
    query GetUsers($limit: Int!, $offset: Int!) {
      users(limit: $limit, offset: $offset) {
        id
        email
        username
        fullName
        createdAt
        isActive
      }
    }
  `,
  getProducts: `
    query GetProducts($limit: Int!, $offset: Int!) {
      products(
        pagination: { limit: $limit, offset: $offset }
      ) {
        id
        sku
        name
        description
        price
        stockQuantity
      }
    }
  `,
  searchProducts: `
    query SearchProducts($query: String!, $limit: Int!) {
      searchProducts(query: $query, limit: $limit) {
        id
        sku
        name
        price
        stockQuantity
      }
    }
  `,
  getOrders: `
    query GetOrders($limit: Int!, $offset: Int!) {
      orders(
        pagination: { limit: $limit, offset: $offset }
      ) {
        id
        orderNumber
        status
        totalAmount
        createdAt
        itemCount
      }
    }
  `,
};

// Test data
const searchTerms = ['widget', 'gadget', 'premium', 'professional', 'tool'];

export default function () {
  // Test 1: Get Users
  let response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.getUsers,
      variables: {
        limit: 20,
        offset: Math.floor(Math.random() * 1000),
      },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'GetUsers' },
    }
  );

  check(response, {
    'GetUsers status is 200': (r) => r.status === 200,
    'GetUsers has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.users && body.data.users.length > 0;
    },
  }) || errorRate.add(1);

  sleep(0.5);

  // Test 2: Get Products
  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.getProducts,
      variables: {
        limit: 50,
        offset: Math.floor(Math.random() * 1000),
      },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'GetProducts' },
    }
  );

  check(response, {
    'GetProducts status is 200': (r) => r.status === 200,
    'GetProducts has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.products && body.data.products.length > 0;
    },
  }) || errorRate.add(1);

  sleep(0.5);

  // Test 3: Search Products
  const searchTerm = searchTerms[Math.floor(Math.random() * searchTerms.length)];
  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.searchProducts,
      variables: {
        query: searchTerm,
        limit: 20,
      },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'SearchProducts' },
    }
  );

  check(response, {
    'SearchProducts status is 200': (r) => r.status === 200,
    'SearchProducts has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.searchProducts;
    },
  }) || errorRate.add(1);

  sleep(0.5);

  // Test 4: Get Orders
  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.getOrders,
      variables: {
        limit: 10,
        offset: Math.floor(Math.random() * 100),
      },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'GetOrders' },
    }
  );

  check(response, {
    'GetOrders status is 200': (r) => r.status === 200,
    'GetOrders has data': (r) => {
      const body = JSON.parse(r.body);
      return body.data && body.data.orders && body.data.orders.length > 0;
    },
  }) || errorRate.add(1);

  sleep(1);
}
