import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';

// Custom metrics
const queryDuration = new Trend('query_duration_sustained');
const errorRate = new Rate('errors');
const successRate = new Rate('success');

export const options = {
  stages: [
    { duration: '30s', target: 50 },  // Ramp up to 50 users
    { duration: '5m', target: 50 },   // Stay at 50 users for 5 minutes
    { duration: '30s', target: 0 },   // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<1000'], // 95% of requests under 1s
    http_req_failed: ['rate<0.05'],    // Error rate under 5%
    errors: ['rate<0.05'],             // Custom error rate under 5%
  },
};

const STRAWBERRY_URL = 'http://benchmark-strawberry:8000/graphql';

// Mix of queries to simulate real load
const queryTemplates = [
  // Simple queries (60% of load)
  {
    name: 'getUserById',
    weight: 0.2,
    query: `query GetUser($id: ID!) {
      user(id: $id) {
        id
        email
        username
        isActive
      }
    }`,
    variables: () => ({
      id: Math.floor(Math.random() * 100000) + 1
    })
  },
  {
    name: 'listProducts',
    weight: 0.2,
    query: `{
      products(pagination: { limit: 20, offset: ${Math.floor(Math.random() * 1000)} }) {
        id
        name
        price
        stockQuantity
      }
    }`,
    variables: () => ({})
  },
  {
    name: 'searchProducts',
    weight: 0.2,
    query: `query SearchProducts($minPrice: Float!, $maxPrice: Float!) {
      products(filter: { minPrice: $minPrice, maxPrice: $maxPrice }, pagination: { limit: 10 }) {
        id
        name
        price
        category {
          name
        }
      }
    }`,
    variables: () => ({
      minPrice: Math.floor(Math.random() * 100),
      maxPrice: Math.floor(Math.random() * 900) + 100
    })
  },

  // Complex queries (40% of load)
  {
    name: 'userWithOrders',
    weight: 0.2,
    query: `query GetUserWithOrders($id: ID!) {
      user(id: $id) {
        id
        email
        orders(limit: 10) {
          id
          status
          totalAmount
          orderItems {
            quantity
            unitPrice
            product {
              name
            }
          }
        }
      }
    }`,
    variables: () => ({
      id: Math.floor(Math.random() * 100000) + 1
    })
  },
  {
    name: 'productWithReviews',
    weight: 0.2,
    query: `query GetProductWithReviews($id: ID!) {
      product(id: $id) {
        id
        name
        price
        reviews(limit: 5) {
          rating
          comment
          user {
            username
          }
        }
      }
    }`,
    variables: () => ({
      id: Math.floor(Math.random() * 1000000) + 1
    })
  }
];

// Calculate cumulative weights for random selection
let cumulativeWeight = 0;
const cumulativeWeights = queryTemplates.map(q => {
  cumulativeWeight += q.weight;
  return cumulativeWeight;
});

function selectRandomQuery() {
  const random = Math.random();
  for (let i = 0; i < cumulativeWeights.length; i++) {
    if (random < cumulativeWeights[i]) {
      return queryTemplates[i];
    }
  }
  return queryTemplates[queryTemplates.length - 1];
}

export default function () {
  // Select a random query based on weights
  const queryTemplate = selectRandomQuery();
  const variables = queryTemplate.variables();

  const payload = {
    query: queryTemplate.query,
  };

  if (Object.keys(variables).length > 0) {
    payload.variables = variables;
  }

  const start = Date.now();
  const res = http.post(
    STRAWBERRY_URL,
    JSON.stringify(payload),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { query: queryTemplate.name }
    }
  );
  const duration = Date.now() - start;

  // Check response
  const success = check(res, {
    'status is 200': (r) => r.status === 200,
    'no errors': (r) => {
      if (r.status !== 200) return false;
      const body = JSON.parse(r.body);
      return !body.errors;
    },
  });

  // Record metrics
  queryDuration.add(duration, { query: queryTemplate.name });
  successRate.add(success);
  errorRate.add(!success);

  // Variable think time based on query complexity
  const thinkTime = queryTemplate.weight < 0.3 ? 0.5 : 1;
  sleep(thinkTime + Math.random() * 0.5);
}

export function handleSummary(data) {
  const timestamp = new Date().toISOString();

  console.log('\\n=== SUSTAINED LOAD TEST RESULTS ===');
  console.log(`Test completed at: ${timestamp}`);
  console.log(`\\nTotal Requests: ${data.metrics.http_reqs.values.count}`);
  console.log(`Request Rate: ${data.metrics.http_reqs.values.rate.toFixed(2)} req/s`);
  console.log(`\\nSuccess Rate: ${((1 - data.metrics.errors.values.rate) * 100).toFixed(2)}%`);
  console.log(`Failed Requests: ${data.metrics.http_req_failed.values.passes}`);

  console.log('\\nResponse Time Percentiles:');
  console.log(`  50th: ${data.metrics.http_req_duration.values.med.toFixed(2)}ms`);
  console.log(`  75th: ${data.metrics.http_req_duration.values['p(75)'].toFixed(2)}ms`);
  console.log(`  90th: ${data.metrics.http_req_duration.values['p(90)'].toFixed(2)}ms`);
  console.log(`  95th: ${data.metrics.http_req_duration.values['p(95)'].toFixed(2)}ms`);
  console.log(`  99th: ${data.metrics.http_req_duration.values['p(99)'].toFixed(2)}ms`);

  return {
    '/results/sustained-load-results.json': JSON.stringify(data, null, 2),
    '/results/sustained-load-summary.txt': `
Sustained Load Test Results
===========================
Timestamp: ${timestamp}
Duration: 6 minutes
Peak Users: 50

Performance Metrics:
- Total Requests: ${data.metrics.http_reqs.values.count}
- Request Rate: ${data.metrics.http_reqs.values.rate.toFixed(2)} req/s
- Success Rate: ${((1 - data.metrics.errors.values.rate) * 100).toFixed(2)}%
- Error Rate: ${(data.metrics.errors.values.rate * 100).toFixed(2)}%

Response Times:
- Median: ${data.metrics.http_req_duration.values.med.toFixed(2)}ms
- 95th percentile: ${data.metrics.http_req_duration.values['p(95)'].toFixed(2)}ms
- 99th percentile: ${data.metrics.http_req_duration.values['p(99)'].toFixed(2)}ms

Thresholds:
- 95% < 1000ms: ${data.metrics.http_req_duration.thresholds['p(95)<1000'].ok ? 'PASSED' : 'FAILED'}
- Error Rate < 5%: ${data.metrics.http_req_failed.thresholds['rate<0.05'].ok ? 'PASSED' : 'FAILED'}
`
  };
}
