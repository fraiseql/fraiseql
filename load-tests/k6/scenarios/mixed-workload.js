/**
 * Mixed Workload Load Test
 *
 * Simulates realistic traffic patterns:
 * - 80% read operations (queries)
 * - 15% write operations (mutations)
 * - 5% health checks / diagnostic endpoints
 *
 * This represents typical production GraphQL API usage where reads
 * vastly outnumber writes, with occasional health/monitoring traffic.
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
  randomString,
  isSuccess,
} from "../config.js";
import { defaultThresholds } from "../thresholds.js";

// Query Operations (80% of traffic)
const QUERIES = {
  getUserProfile: `
    query GetUser($id: ID!) {
      user(id: $id) {
        id
        name
        email
        avatar
        bio
      }
    }
  `,

  listPosts: `
    query ListPosts($limit: Int!, $offset: Int!) {
      posts(limit: $limit, offset: $offset) {
        id
        title
        content
        author {
          id
          name
        }
        likeCount
        commentCount
      }
    }
  `,

  searchUsers: `
    query SearchUsers($query: String!) {
      search(query: $query, type: USER) {
        ... on User {
          id
          name
          email
        }
      }
    }
  `,

  getTimeline: `
    query GetTimeline($userId: ID!, $limit: Int!) {
      user(id: $userId) {
        timeline(limit: $limit) {
          id
          type
          content
          timestamp
          actor {
            id
            name
          }
        }
      }
    }
  `,
};

// Mutation Operations (15% of traffic)
const MUTATIONS = {
  createPost: `
    mutation CreatePost($input: CreatePostInput!) {
      createPost(input: $input) {
        id
        title
        createdAt
      }
    }
  `,

  likePost: `
    mutation LikePost($postId: ID!) {
      likePost(postId: $postId) {
        success
        likeCount
      }
    }
  `,

  updateProfile: `
    mutation UpdateProfile($userId: ID!, $input: UpdateProfileInput!) {
      updateProfile(userId: $userId, input: $input) {
        id
        name
        bio
        updatedAt
      }
    }
  `,
};

// Health/Diagnostic Operations (5% of traffic)
const HEALTH_CHECKS = `
  query HealthCheck {
    health {
      status
      uptime
      timestamp
    }
  }
`;

// Generate test variables for queries
function getQueryVariables(operationType) {
  switch (operationType) {
    case "getUserProfile":
      return { id: String(randomInt(1, 10000)) };
    case "listPosts":
      return { limit: randomInt(10, 50), offset: randomInt(0, 1000) };
    case "searchUsers":
      return { query: `user${randomInt(1, 100)}` };
    case "getTimeline":
      return {
        userId: String(randomInt(1, 10000)),
        limit: randomInt(20, 100),
      };
    default:
      return {};
  }
}

// Generate test variables for mutations
function getMutationVariables(operationType) {
  switch (operationType) {
    case "createPost":
      return {
        input: {
          title: `Post ${randomString(15)}`,
          content: `Content: ${randomString(100)}`,
        },
      };
    case "likePost":
      return { postId: String(randomInt(1, 50000)) };
    case "updateProfile":
      return {
        userId: String(randomInt(1, 10000)),
        input: {
          name: `User ${randomString(8)}`,
          bio: randomString(50),
        },
      };
    default:
      return {};
  }
}

// Execute operations
function executeQuery(operationType) {
  const variables = getQueryVariables(operationType);
  const response = executeGraphQL(QUERIES[operationType], variables, {
    tags: { type: "read", operationType },
  });
  return response;
}

function executeMutation(operationType) {
  const variables = getMutationVariables(operationType);
  const response = executeGraphQL(MUTATIONS[operationType], variables, {
    tags: { type: "write", operationType },
  });
  return response;
}

function executeHealthCheck() {
  const response = executeGraphQL(HEALTH_CHECKS, {}, {
    tags: { type: "health" },
  });
  return response;
}

// Test options
export const options = {
  thresholds: defaultThresholds,

  scenarios: {
    // Daytime traffic pattern
    daytime_load: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      preAllocatedVUs: 200,
      maxVUs: 600,
      stages: [
        { duration: "2m", target: 500 }, // Warm up
        { duration: "5m", target: 1000 }, // Peak daytime traffic
        { duration: "5m", target: 1000 }, // Sustain peak
        { duration: "2m", target: 500 }, // Cool down
      ],
    },

    // Burst/spike pattern
    burst_traffic: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      preAllocatedVUs: 300,
      maxVUs: 800,
      stages: [
        { duration: "1m", target: 200 },
        { duration: "30s", target: 2000 }, // Sudden spike
        { duration: "1m", target: 2000 },
        { duration: "2m", target: 200 }, // Return to normal
      ],
    },
  },
};

export default function () {
  // Weighted random selection: 80% read, 15% write, 5% health
  const operation = Math.random();

  if (operation < 0.80) {
    // 80% - Read operations
    const readOps = Object.keys(QUERIES);
    const op = readOps[Math.floor(Math.random() * readOps.length)];

    group(`Read: ${op}`, () => {
      const response = executeQuery(op);

      check(response, {
        "status is 200": (r) => r.status === 200,
        "response time < 500ms": (r) => r.timings.duration < 500,
        "response contains data": (r) => !r.body.includes('"errors"'),
      });

      validateGraphQLResponse(response, check);
    });
  } else if (operation < 0.95) {
    // 15% - Write operations
    const writeOps = Object.keys(MUTATIONS);
    const op = writeOps[Math.floor(Math.random() * writeOps.length)];

    group(`Write: ${op}`, () => {
      const response = executeMutation(op);

      check(response, {
        "status is 200": (r) => r.status === 200,
        "response time < 1s": (r) => r.timings.duration < 1000,
      });

      validateGraphQLResponse(response, check);
    });
  } else {
    // 5% - Health checks
    group("Health Check", () => {
      const response = executeHealthCheck();

      check(response, {
        "status is 200": (r) => r.status === 200,
        "health check fast": (r) => r.timings.duration < 100,
      });
    });
  }

  // Small jitter to avoid thundering herd
  sleep(Math.random() * 0.05);
}

/**
 * Setup function
 */
export function setup() {
  console.log("Mixed Workload Load Test");
  console.log(`Endpoint: ${getEndpoint()}`);
  console.log("Traffic Distribution:");
  console.log("  80% - Queries (read operations)");
  console.log("  15% - Mutations (write operations)");
  console.log("  5%  - Health checks");
}

/**
 * Teardown function
 */
export function teardown() {
  console.log("Mixed Workload Load Test Complete");
}
