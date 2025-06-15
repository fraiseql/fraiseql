import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const productQueryDuration = new Trend('product_query_duration');
const orderQueryDuration = new Trend('order_query_duration');
const deepNestedDuration = new Trend('deep_nested_duration');

// Test configuration - Lower concurrent users for complex queries
export const options = {
  stages: [
    { duration: '30s', target: 5 },    // Ramp up to 5 users
    { duration: '1m', target: 20 },    // Ramp up to 20 users
    { duration: '2m', target: 50 },    // Ramp up to 50 users
    { duration: '2m', target: 50 },    // Stay at 50 users
    { duration: '30s', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<2000'], // 95% of requests should be below 2s
    errors: ['rate<0.1'],               // Error rate should be below 10%
    product_query_duration: ['p(95)<1000'],
    order_query_duration: ['p(95)<1500'],
    deep_nested_duration: ['p(95)<3000'],
  },
};

const GRAPHQL_ENDPOINT = `${__ENV.TARGET}/graphql`;

// Complex nested queries
const queries = {
  productWithReviews: `
    query GetProductWithReviews($id: ID!) {
      product(id: $id) {
        id
        sku
        name
        description
        price
        stockQuantity
        category {
          id
          name
          slug
        }
        averageRating
        reviewCount
        reviews {
          id
          rating
          title
          comment
          createdAt
          user {
            id
            username
            fullName
          }
        }
      }
    }
  `,
  orderWithItems: `
    query GetOrderWithItems($id: ID!) {
      order(id: $id) {
        id
        orderNumber
        status
        totalAmount
        createdAt
        user {
          id
          email
          username
          fullName
        }
        items {
          id
          quantity
          unitPrice
          totalPrice
          product {
            id
            sku
            name
            price
          }
        }
        itemCount
      }
    }
  `,
  userOrderHistory: `
    query GetUserOrderHistory($userId: ID!, $limit: Int!) {
      userOrders(userId: $userId, limit: $limit) {
        id
        orderNumber
        status
        totalAmount
        createdAt
        user {
          id
          email
          username
          fullName
          orderCount
          totalSpent
        }
        items {
          id
          quantity
          unitPrice
          totalPrice
          product {
            id
            sku
            name
            price
            category {
              id
              name
            }
          }
        }
      }
    }
  `,
  deepNestedOrder: `
    query DeepNestedOrderQuery($status: String!) {
      orders(
        filter: { status: $status }
        pagination: { limit: 5, offset: 0 }
      ) {
        id
        orderNumber
        status
        totalAmount
        createdAt
        user {
          id
          email
          username
          fullName
          orderCount
          totalSpent
          reviewCount
          averageRating
        }
        items {
          id
          quantity
          unitPrice
          totalPrice
          product {
            id
            sku
            name
            description
            price
            stockQuantity
            category {
              id
              name
              slug
              description
            }
            averageRating
            reviewCount
            reviews {
              id
              rating
              title
              comment
              user {
                id
                username
              }
            }
          }
        }
      }
    }
  `,
};

// Sample IDs (these would be populated from the database in a real scenario)
const productIds = [
  '00000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000002',
  '00000000-0000-0000-0000-000000000003',
  '00000000-0000-0000-0000-000000000004',
  '00000000-0000-0000-0000-000000000005',
];

const orderIds = [
  '00000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000002',
  '00000000-0000-0000-0000-000000000003',
];

const userIds = [
  '00000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000002',
];

const orderStatuses = ['pending', 'processing', 'shipped', 'delivered'];

export default function () {
  // Test 1: Product with Reviews (Medium complexity)
  const productId = productIds[Math.floor(Math.random() * productIds.length)];
  const startTime = new Date();

  let response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.productWithReviews,
      variables: { id: productId },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'ProductWithReviews' },
    }
  );

  const productDuration = new Date() - startTime;
  productQueryDuration.add(productDuration);

  check(response, {
    'ProductWithReviews status is 200': (r) => r.status === 200,
    'ProductWithReviews has no errors': (r) => {
      const body = JSON.parse(r.body);
      return !body.errors;
    },
  }) || errorRate.add(1);

  sleep(1);

  // Test 2: Order with Items (Medium complexity)
  const orderId = orderIds[Math.floor(Math.random() * orderIds.length)];
  const orderStartTime = new Date();

  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.orderWithItems,
      variables: { id: orderId },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'OrderWithItems' },
    }
  );

  const orderDuration = new Date() - orderStartTime;
  orderQueryDuration.add(orderDuration);

  check(response, {
    'OrderWithItems status is 200': (r) => r.status === 200,
    'OrderWithItems has no errors': (r) => {
      const body = JSON.parse(r.body);
      return !body.errors;
    },
  }) || errorRate.add(1);

  sleep(1);

  // Test 3: User Order History (High complexity)
  const userId = userIds[Math.floor(Math.random() * userIds.length)];

  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.userOrderHistory,
      variables: {
        userId: userId,
        limit: 10
      },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'UserOrderHistory' },
    }
  );

  check(response, {
    'UserOrderHistory status is 200': (r) => r.status === 200,
    'UserOrderHistory has no errors': (r) => {
      const body = JSON.parse(r.body);
      return !body.errors;
    },
  }) || errorRate.add(1);

  sleep(2);

  // Test 4: Deep Nested Order Query (Very high complexity)
  const status = orderStatuses[Math.floor(Math.random() * orderStatuses.length)];
  const deepStartTime = new Date();

  response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: queries.deepNestedOrder,
      variables: { status: status },
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'DeepNestedOrder' },
    }
  );

  const deepDuration = new Date() - deepStartTime;
  deepNestedDuration.add(deepDuration);

  check(response, {
    'DeepNestedOrder status is 200': (r) => r.status === 200,
    'DeepNestedOrder has no errors': (r) => {
      const body = JSON.parse(r.body);
      return !body.errors;
    },
  }) || errorRate.add(1);

  sleep(2);
}
