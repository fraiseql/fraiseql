import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const requestsPerSecond = new Counter('requests_per_second');
const successfulRequests = new Counter('successful_requests');
const failedRequests = new Counter('failed_requests');

// Test configuration - High load test
export const options = {
  stages: [
    { duration: '1m', target: 50 },     // Ramp up to 50 users
    { duration: '2m', target: 100 },    // Ramp up to 100 users
    { duration: '3m', target: 200 },    // Ramp up to 200 users
    { duration: '5m', target: 500 },    // Ramp up to 500 users
    { duration: '5m', target: 500 },    // Stay at 500 users
    { duration: '2m', target: 100 },    // Ramp down to 100
    { duration: '1m', target: 0 },      // Ramp down to 0
  ],
  thresholds: {
    http_req_duration: ['p(95)<1000', 'p(99)<2000'], // Performance thresholds
    errors: ['rate<0.05'],                             // Error rate < 5%
    http_req_failed: ['rate<0.05'],                    // HTTP failure rate < 5%
  },
};

const GRAPHQL_ENDPOINT = `${__ENV.TARGET}/graphql`;

// Mix of queries for realistic load
const queryTemplates = [
  {
    name: 'SimpleUserQuery',
    weight: 30,
    query: `
      query GetUsers($limit: Int!) {
        users(limit: $limit) {
          id
          email
          username
        }
      }
    `,
    variables: () => ({ limit: 10 }),
  },
  {
    name: 'SimpleProductQuery',
    weight: 25,
    query: `
      query GetProducts($limit: Int!) {
        products(pagination: { limit: $limit, offset: 0 }) {
          id
          name
          price
          stockQuantity
        }
      }
    `,
    variables: () => ({ limit: 20 }),
  },
  {
    name: 'ProductSearch',
    weight: 20,
    query: `
      query SearchProducts($query: String!) {
        searchProducts(query: $query, limit: 10) {
          id
          name
          price
        }
      }
    `,
    variables: () => ({
      query: ['widget', 'gadget', 'tool', 'device'][Math.floor(Math.random() * 4)]
    }),
  },
  {
    name: 'ProductWithCategory',
    weight: 15,
    query: `
      query GetProductsWithCategory($limit: Int!) {
        products(pagination: { limit: $limit, offset: 0 }) {
          id
          name
          price
          category {
            id
            name
          }
        }
      }
    `,
    variables: () => ({ limit: 10 }),
  },
  {
    name: 'OrderWithUser',
    weight: 10,
    query: `
      query GetOrdersWithUser($limit: Int!) {
        orders(pagination: { limit: $limit, offset: 0 }) {
          id
          orderNumber
          totalAmount
          user {
            id
            username
            email
          }
        }
      }
    `,
    variables: () => ({ limit: 5 }),
  },
];

// Calculate cumulative weights for weighted random selection
let cumulativeWeight = 0;
const weightedQueries = queryTemplates.map(q => {
  cumulativeWeight += q.weight;
  return { ...q, cumulativeWeight };
});

function selectRandomQuery() {
  const random = Math.random() * cumulativeWeight;
  return weightedQueries.find(q => random <= q.cumulativeWeight);
}

export default function () {
  // Select a random query based on weights
  const selectedQuery = selectRandomQuery();

  // Record start time
  const startTime = new Date();

  // Execute the query
  const response = http.post(
    GRAPHQL_ENDPOINT,
    JSON.stringify({
      query: selectedQuery.query,
      variables: selectedQuery.variables(),
    }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: selectedQuery.name },
      timeout: '10s',
    }
  );

  // Track metrics
  requestsPerSecond.add(1);

  // Check response
  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'no errors': (r) => {
      if (r.status !== 200) return false;
      try {
        const body = JSON.parse(r.body);
        return !body.errors;
      } catch (e) {
        return false;
      }
    },
    'response time < 1s': (r) => r.timings.duration < 1000,
  });

  if (success) {
    successfulRequests.add(1);
  } else {
    failedRequests.add(1);
    errorRate.add(1);
  }

  // Vary sleep time based on load
  const currentVUs = __VU;
  if (currentVUs < 100) {
    sleep(Math.random() * 2 + 0.5); // 0.5-2.5s
  } else if (currentVUs < 300) {
    sleep(Math.random() * 1 + 0.2); // 0.2-1.2s
  } else {
    sleep(Math.random() * 0.5 + 0.1); // 0.1-0.6s
  }
}

export function handleSummary(data) {
  // Custom summary report
  const customData = {
    framework: __ENV.TARGET.includes('fraiseql') ? 'FraiseQL' :
               __ENV.TARGET.includes('strawberry') ? 'Strawberry+SQLAlchemy' :
               'Unknown',
    testType: 'load-test',
    timestamp: new Date().toISOString(),
    summary: {
      totalRequests: data.metrics.http_reqs.values.count,
      successfulRequests: data.metrics.successful_requests ? data.metrics.successful_requests.values.count : 0,
      failedRequests: data.metrics.failed_requests ? data.metrics.failed_requests.values.count : 0,
      avgResponseTime: data.metrics.http_req_duration.values.avg,
      p95ResponseTime: data.metrics.http_req_duration.values['p(95)'],
      p99ResponseTime: data.metrics.http_req_duration.values['p(99)'],
      errorRate: data.metrics.errors ? data.metrics.errors.values.rate : 0,
      maxVUs: data.metrics.vus_max.values.value,
    },
    queryBreakdown: {},
  };

  // Add per-query metrics
  queryTemplates.forEach(q => {
    const taggedMetrics = data.metrics[`http_req_duration{name:${q.name}}`];
    if (taggedMetrics) {
      customData.queryBreakdown[q.name] = {
        count: taggedMetrics.values.count,
        avg: taggedMetrics.values.avg,
        p95: taggedMetrics.values['p(95)'],
        p99: taggedMetrics.values['p(99)'],
      };
    }
  });

  return {
    'stdout': JSON.stringify(customData, null, 2),
  };
}
