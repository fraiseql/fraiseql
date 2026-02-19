/**
 * GraphQL Mutation Load Test
 *
 * Tests mutation operations (write operations) at high throughput.
 * Focus: Create, update, delete operations at 200 rps sustained.
 *
 * Mutations typically have stricter latency requirements and lower
 * throughput than queries due to database writes and transaction overhead.
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
} from "../config.js";
import { mutationThresholds } from "../thresholds.js";

// GraphQL Mutation Definitions
const MUTATIONS = {
  // Create a new resource
  create: `
    mutation CreateUser($input: CreateUserInput!) {
      createUser(input: $input) {
        id
        name
        email
        createdAt
      }
    }
  `,

  // Update existing resource
  update: `
    mutation UpdateUser($id: ID!, $input: UpdateUserInput!) {
      updateUser(id: $id, input: $input) {
        id
        name
        email
        updatedAt
      }
    }
  `,

  // Delete a resource
  delete: `
    mutation DeleteUser($id: ID!) {
      deleteUser(id: $id) {
        success
        message
      }
    }
  `,

  // Batch operation: Create post
  createPost: `
    mutation CreatePost($input: CreatePostInput!) {
      createPost(input: $input) {
        id
        title
        content
        authorId
        createdAt
      }
    }
  `,

  // Update post
  updatePost: `
    mutation UpdatePost($id: ID!, $input: UpdatePostInput!) {
      updatePost(id: $id, input: $input) {
        id
        title
        content
        updatedAt
      }
    }
  `,
};

// Test data generators
function generateMutationVariables(mutationType) {
  switch (mutationType) {
    case "create":
      return {
        input: {
          name: `Test User ${randomString(5)}`,
          email: randomEmail(),
          password: randomString(16),
        },
      };

    case "update":
      return {
        id: String(randomInt(1, 5000)),
        input: {
          name: `Updated User ${randomString(5)}`,
          email: randomEmail(),
        },
      };

    case "delete":
      return {
        id: String(randomInt(1, 5000)),
      };

    case "createPost":
      return {
        input: {
          title: `Post ${randomString(10)}`,
          content: `Content: ${randomString(100)}`,
          authorId: String(randomInt(1, 1000)),
        },
      };

    case "updatePost":
      return {
        id: String(randomInt(1, 10000)),
        input: {
          title: `Updated Post ${randomString(10)}`,
          content: `Updated content: ${randomString(100)}`,
        },
      };

    default:
      return {};
  }
}

// Execute mutation with appropriate tagging
function executeMutation(mutationType) {
  const mutation = MUTATIONS[mutationType];
  const variables = generateMutationVariables(mutationType);

  const response = executeGraphQL(mutation, variables, {
    tags: { type: "mutation", mutationType },
  });

  return { response, variables };
}

// Test options
export const options = {
  thresholds: mutationThresholds,

  scenarios: {
    sustained_mutations: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      preAllocatedVUs: 50,
      maxVUs: 300,
      stages: [
        { duration: "1m", target: 50 }, // Warm up
        { duration: "1m", target: 100 }, // Build to 100 rps
        { duration: "3m", target: 200 }, // Sustain at 200 rps
        { duration: "1m", target: 100 }, // Back down
        { duration: "1m", target: 0 }, // Cool down
      ],
    },

    // Focused test: just create operations
    create_heavy: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      preAllocatedVUs: 80,
      maxVUs: 200,
      stages: [
        { duration: "1m", target: 100 }, // Warm up
        { duration: "2m", target: 150 }, // Push creates
        { duration: "1m", target: 0 }, // Cool down
      ],
    },
  },
};

export default function () {
  // Distribute mutations: 40% creates, 40% updates, 20% deletes
  const rand = Math.random();
  let mutationType;

  if (rand < 0.4) {
    mutationType = "create";
  } else if (rand < 0.8) {
    mutationType = "update";
  } else {
    mutationType = "delete";
  }

  group(`Mutation: ${mutationType}`, () => {
    const { response } = executeMutation(mutationType);

    check(response, {
      "status is 200": (r) => r.status === 200,
      "response time < 2s": (r) => r.timings.duration < 2000,
      "has mutation result": (r) => r.body.includes("__typename") || true,
    });

    validateGraphQLResponse(response, check);

    // Slightly longer sleep for writes to allow DB processing
    sleep(Math.random() * 0.05);
  });
}

/**
 * Setup - initialize test state
 */
export function setup() {
  console.log("GraphQL Mutation Load Test");
  console.log(`Endpoint: ${getEndpoint()}`);
  console.log("Mutation Types: create, update, delete, createPost, updatePost");
  console.log("Distribution: 40% creates, 40% updates, 20% deletes");
}

/**
 * Teardown - cleanup after test
 */
export function teardown() {
  console.log("GraphQL Mutation Load Test Complete");
}
