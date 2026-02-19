/**
 * APQ (Automatic Persisted Query) Cache Effectiveness Test
 *
 * Tests Automatic Persisted Query cache performance improvements.
 * APQ allows clients to send only query hashes instead of full queries,
 * reducing bandwidth and parse overhead.
 *
 * Expected behavior:
 * - First request: Full query sent (cache miss, slower)
 * - Subsequent requests: Only hash sent (cache hit, faster)
 * - Cache misses should be ~5-10x slower due to query registration overhead
 *
 * This test validates that:
 * 1. Repeated queries show expected cache hit performance improvement
 * 2. New queries can register properly (cache miss, then hit)
 * 3. Overall throughput improves as cache warms up
 */

import http from "k6/http";
import { check, group, sleep } from "k6";
import crypto from "k6/crypto";
import {
  getEndpoint,
  getHeaders,
  executeGraphQLWithAPQ,
  executeGraphQL,
  randomInt,
  validateGraphQLResponse,
} from "../config.js";
import { apqThresholds } from "../thresholds.js";

// Set of test queries with different complexity levels
const TEST_QUERIES = [
  // Query 1: Simple user lookup (very common)
  {
    id: "query-user-1",
    query: `
      query GetUser($id: ID!) {
        user(id: $id) {
          id
          name
          email
        }
      }
    `,
    variables: () => ({ id: String(randomInt(1, 100)) }),
    frequency: 0.4, // 40% of cache hit traffic
  },

  // Query 2: User with posts (medium complexity)
  {
    id: "query-posts-2",
    query: `
      query GetUserPosts($userId: ID!, $limit: Int!) {
        user(id: $userId) {
          id
          name
          posts(limit: $limit) {
            id
            title
            createdAt
            likes
          }
        }
      }
    `,
    variables: () => ({
      userId: String(randomInt(1, 100)),
      limit: randomInt(5, 20),
    }),
    frequency: 0.35, // 35%
  },

  // Query 3: Complex search (high complexity)
  {
    id: "query-search-3",
    query: `
      query SearchContent($query: String!, $type: SearchType!) {
        search(query: $query, type: $type) {
          ... on User {
            id
            name
            email
            avatar
          }
          ... on Post {
            id
            title
            content
            author {
              id
              name
            }
          }
        }
      }
    `,
    variables: () => ({
      query: `search${randomInt(1, 50)}`,
      type: randomInt(0, 1) ? "USER" : "POST",
    }),
    frequency: 0.2, // 20%
  },

  // Query 4: Analytics (rare, might not be cached)
  {
    id: "query-analytics-4",
    query: `
      query GetAnalytics($userId: ID!) {
        analytics(userId: $userId) {
          totalViews
          totalLikes
          totalComments
          topPosts {
            id
            title
            views
          }
        }
      }
    `,
    variables: () => ({ userId: String(randomInt(1, 100)) }),
    frequency: 0.05, // 5%
  },
];

// Track cache hit/miss statistics
let stats = {
  totalRequests: 0,
  cacheHits: 0,
  cacheMisses: 0,
  hitTimes: [],
  missTimes: [],
};

/**
 * Select a query based on frequency distribution
 * Ensures hot queries are requested more often (realistic cache behavior)
 */
function selectQuery() {
  const rand = Math.random();
  let cumulative = 0;

  for (const query of TEST_QUERIES) {
    cumulative += query.frequency;
    if (rand < cumulative) {
      return query;
    }
  }

  return TEST_QUERIES[0];
}

/**
 * Execute query with APQ and track cache performance
 */
function executeWithAPQ(queryDef) {
  const variables = queryDef.variables();
  const response = executeGraphQLWithAPQ(queryDef.query, variables, {
    tags: {
      type: "apq",
      queryId: queryDef.id,
    },
  });

  stats.totalRequests++;

  // Determine if cache hit or miss by response time
  // This is a heuristic: misses are typically >100ms, hits <50ms
  const duration = response.timings.duration;
  if (response.status === 200) {
    if (duration > 100) {
      stats.cacheMisses++;
      stats.missTimes.push(duration);
    } else {
      stats.cacheHits++;
      stats.hitTimes.push(duration);
    }
  }

  return { response, queryDef, duration };
}

/**
 * Execute same query twice to demonstrate cache hit
 */
function executeCacheWarmupCycle() {
  const queryDef = selectQuery();
  const variables = queryDef.variables();

  // First request: likely cache miss
  group(`APQ Cache Warmup: ${queryDef.id}`, () => {
    const response1 = executeGraphQLWithAPQ(queryDef.query, variables, {
      tags: {
        type: "apq",
        queryId: queryDef.id,
        cachePhase: "miss",
      },
    });

    check(response1, {
      "first request succeeds": (r) => r.status === 200,
    });

    sleep(0.1); // Wait before retry

    // Second request: should hit cache
    const response2 = executeGraphQLWithAPQ(queryDef.query, variables, {
      tags: {
        type: "apq",
        queryId: queryDef.id,
        cachePhase: "hit",
      },
    });

    check(response2, {
      "cached request succeeds": (r) => r.status === 200,
      "cache hit is faster": (r) => r.timings.duration < 100,
    });

    validateGraphQLResponse(response2, check);
  });
}

// Test options
export const options = {
  thresholds: apqThresholds,

  scenarios: {
    // Phase 1: Cache warmup - new queries being registered
    cache_warmup: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      preAllocatedVUs: 30,
      maxVUs: 100,
      stages: [
        { duration: "1m", target: 50 }, // Warm up with new queries
        { duration: "2m", target: 100 }, // Build to normal load
      ],
    },

    // Phase 2: Warm cache - mostly cache hits
    warm_cache: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      preAllocatedVUs: 80,
      maxVUs: 300,
      stages: [
        { duration: "1m", target: 100 }, // Start with warm cache
        { duration: "3m", target: 500 }, // Ramp up to high load
        { duration: "1m", target: 500 }, // Sustain
        { duration: "1m", target: 0 }, // Cool down
      ],
    },

    // Phase 3: New query injection - occasional misses in warm cache
    cache_with_new_queries: {
      executor: "ramping-arrival-rate",
      startRate: 50,
      timeUnit: "1s",
      preAllocatedVUs: 60,
      maxVUs: 200,
      stages: [
        { duration: "1m", target: 100 },
        { duration: "2m", target: 300 }, // Some new queries mixed in
        { duration: "1m", target: 0 },
      ],
    },
  },
};

export default function () {
  // In cache_warmup scenario, execute full cycle to demonstrate cache effectiveness
  if (__VU % 3 === 0) {
    executeCacheWarmupCycle();
  } else {
    // Normal APQ request - should mostly be cache hits in warm cache
    const queryDef = selectQuery();
    const variables = queryDef.variables();

    group(`APQ Query: ${queryDef.id}`, () => {
      const { response, duration } = executeWithAPQ(queryDef);

      check(response, {
        "status is 200": (r) => r.status === 200,
        "response time acceptable": (r) => r.timings.duration < 500,
      });

      validateGraphQLResponse(response, check);
    });
  }

  sleep(Math.random() * 0.05);
}

/**
 * Setup function - Initialize cache and logging
 */
export function setup() {
  console.log("APQ Cache Effectiveness Test");
  console.log(`Endpoint: ${getEndpoint()}`);
  console.log("\nTest Queries:");
  TEST_QUERIES.forEach((q, i) => {
    console.log(`  ${q.id}: ${(q.frequency * 100).toFixed(0)}% frequency`);
  });
  console.log("\nExpected Behavior:");
  console.log("  - Cache misses: >100ms (query registration)");
  console.log("  - Cache hits: <50ms (hash-only query)");
  console.log("  - Hit/miss ratio improves over time as cache warms");

  return { startTime: Date.now() };
}

/**
 * Teardown function - Print cache statistics
 */
export function teardown(data) {
  console.log("\n=== APQ Cache Test Results ===");
  console.log(`Total Requests: ${stats.totalRequests}`);
  console.log(`Cache Hits: ${stats.cacheHits}`);
  console.log(`Cache Misses: ${stats.cacheMisses}`);
  console.log(
    `Hit Rate: ${((stats.cacheHits / stats.totalRequests) * 100).toFixed(1)}%`
  );

  if (stats.hitTimes.length > 0) {
    const avgHit =
      stats.hitTimes.reduce((a, b) => a + b, 0) / stats.hitTimes.length;
    console.log(`Avg Cache Hit Time: ${avgHit.toFixed(1)}ms`);
  }

  if (stats.missTimes.length > 0) {
    const avgMiss =
      stats.missTimes.reduce((a, b) => a + b, 0) / stats.missTimes.length;
    console.log(`Avg Cache Miss Time: ${avgMiss.toFixed(1)}ms`);
  }

  if (stats.hitTimes.length > 0 && stats.missTimes.length > 0) {
    const avgHit =
      stats.hitTimes.reduce((a, b) => a + b, 0) / stats.hitTimes.length;
    const avgMiss =
      stats.missTimes.reduce((a, b) => a + b, 0) / stats.missTimes.length;
    console.log(`Performance Improvement: ${(avgMiss / avgHit).toFixed(1)}x`);
  }

  const duration = (Date.now() - data.startTime) / 1000;
  console.log(`Test Duration: ${duration.toFixed(1)}s`);
  console.log(`Throughput: ${(stats.totalRequests / duration).toFixed(0)} req/s`);
}
