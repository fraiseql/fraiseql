/**
 * GraphQL Query Load Test
 *
 * Tests standard query operations with two scenarios:
 * 1. Sustained Load: 1000 rps for 5 minutes
 * 2. Spike Test: Rapid escalation 100→5000→100 rps
 *
 * Tests 4 different query types:
 * - Simple field queries (small payload)
 * - Nested queries (medium complexity)
 * - Complex queries with aliases (GraphQL features)
 * - Batch queries (multiple operations)
 */

import http from "k6/http";
import { check, group, sleep } from "k6";
import {
  getEndpoint,
  getHeaders,
  executeGraphQL,
  validateGraphQLResponse,
  randomInt,
  randomEmail,
} from "../config.js";
import { defaultThresholds, combineThresholds } from "../thresholds.js";

// GraphQL Query Definitions
const QUERIES = {
  // Simple field query - baseline performance
  simple: `
    query GetUser($id: ID!) {
      user(id: $id) {
        id
        name
        email
      }
    }
  `,

  // Nested query with relationships
  nested: `
    query GetUserWithPosts($id: ID!) {
      user(id: $id) {
        id
        name
        email
        posts {
          id
          title
          createdAt
          author {
            id
            name
          }
        }
      }
    }
  `,

  // Complex query with aliases and fragments
  complex: `
    query GetUserAnalytics($id: ID!) {
      user(id: $id) {
        id
        name
        postCount: posts {
          __typename
        }
        recentPosts: posts(limit: 5) {
          id
          title
          likes
          comments {
            id
            text
          }
        }
        following: connections(type: FOLLOWING) {
          id
          name
        }
      }
    }
  `,

  // Batch query - multiple roots (less common but valid)
  list: `
    query GetUsers($limit: Int!, $offset: Int!) {
      users(limit: $limit, offset: $offset) {
        id
        name
        email
        createdAt
      }
    }
  `,
};

// Test data generators
function generateTestVariables(queryType) {
  switch (queryType) {
    case "simple":
    case "nested":
    case "complex":
      return { id: String(randomInt(1, 10000)) };

    case "list":
      return {
        limit: randomInt(10, 100),
        offset: randomInt(0, 1000),
      };

    default:
      return {};
  }
}

// Metrics tagging
function executeQuery(queryType) {
  const query = QUERIES[queryType];
  const variables = generateTestVariables(queryType);

  const response = executeGraphQL(query, variables, {
    tags: { type: "query", queryType },
  });

  return { response, variables };
}

// Test setup
export const options = {
  thresholds: combineThresholds(defaultThresholds),

  scenarios: {
    sustained_load: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      preAllocatedVUs: 200,
      maxVUs: 500,
      stages: [
        { duration: "2m", target: 1000 }, // Ramp up to 1000 rps
        { duration: "5m", target: 1000 }, // Maintain for 5 minutes
        { duration: "2m", target: 0 }, // Ramp down
      ],
    },

    spike_test: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      preAllocatedVUs: 300,
      maxVUs: 1000,
      stages: [
        { duration: "1m", target: 100 }, // Warm up
        { duration: "30s", target: 5000 }, // Spike to 5000 rps
        { duration: "30s", target: 5000 }, // Hold spike
        { duration: "1m", target: 100 }, // Drop back
        { duration: "1m", target: 0 }, // Cool down
      ],
    },
  },
};

export default function () {
  // Distribute load across query types
  const queryTypes = ["simple", "nested", "complex", "list"];
  const queryType = queryTypes[Math.floor(Math.random() * queryTypes.length)];

  group(`Query: ${queryType}`, () => {
    const { response } = executeQuery(queryType);

    check(response, {
      "status is 200": (r) => r.status === 200,
      "response time < 1s": (r) => r.timings.duration < 1000,
    });

    validateGraphQLResponse(response, check);

    // Small jitter between requests
    sleep(Math.random() * 0.1);
  });
}

/**
 * Setup function - runs once before all test execution
 * Could initialize test data here
 */
export function setup() {
  console.log(`GraphQL Query Load Test`);
  console.log(`Endpoint: ${getEndpoint()}`);
  console.log(`Query Types: simple, nested, complex, list`);
}

/**
 * Teardown function - runs once after all test execution
 */
export function teardown() {
  console.log("GraphQL Query Load Test Complete");
}
