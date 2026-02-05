/**
 * FraiseQL Admin Dashboard Server
 *
 * Provides:
 * - System health monitoring
 * - Schema exploration
 * - Query debugging
 * - Performance metrics
 * - Log aggregation
 */

import express from 'express';
import cors from 'cors';
import bodyParser from 'body-parser';
import fetch from 'node-fetch';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const app = express();
const port = process.env.ADMIN_PORT || 3002;
const fraiseqlApi = process.env.FRAISEQL_API_URL || 'http://fraiseql-server:8000';

// Middleware
app.use(cors());
app.use(bodyParser.json());

// In-memory metrics storage
const metrics = {
  startTime: Date.now(),
  requests: [],
  errors: [],
  queryTimes: [],
};

// ============================================================================
// Static Files
// ============================================================================
app.use(express.static(join(__dirname, '../public')));

// ============================================================================
// Health Check
// ============================================================================
app.get('/health', (req, res) => {
  res.json({ status: 'healthy', service: 'fraiseql-admin-dashboard' });
});

// ============================================================================
// System Health & Metrics
// ============================================================================

// Get system overview
app.get('/api/system/overview', async (req, res) => {
  try {
    // Check FraiseQL server health
    const serverHealth = await checkServerHealth();

    // Calculate uptime
    const uptime = Date.now() - metrics.startTime;
    const uptimeSeconds = Math.floor(uptime / 1000);
    const uptimeMinutes = Math.floor(uptimeSeconds / 60);
    const uptimeHours = Math.floor(uptimeMinutes / 60);
    const uptimeDays = Math.floor(uptimeHours / 24);

    // Calculate metrics
    const totalRequests = metrics.requests.length;
    const totalErrors = metrics.errors.length;
    const errorRate = totalRequests > 0 ? ((totalErrors / totalRequests) * 100).toFixed(2) : 0;
    const avgQueryTime = metrics.queryTimes.length > 0
      ? (metrics.queryTimes.reduce((a, b) => a + b, 0) / metrics.queryTimes.length).toFixed(2)
      : 0;

    res.json({
      status: serverHealth.ok ? 'healthy' : 'unhealthy',
      uptime: {
        milliseconds: uptime,
        human: formatUptime(uptime),
      },
      services: {
        fraiseqlServer: {
          status: serverHealth.ok ? 'healthy' : 'unhealthy',
          responseTime: serverHealth.responseTime,
        },
      },
      metrics: {
        totalRequests,
        totalErrors,
        errorRate: `${errorRate}%`,
        avgQueryTime: `${avgQueryTime}ms`,
      },
    });
  } catch (error) {
    res.status(500).json({
      error: 'Failed to get system overview',
      message: error.message,
    });
  }
});

// Check FraiseQL server health
async function checkServerHealth() {
  try {
    const start = Date.now();
    const response = await fetch(`${fraiseqlApi}/health`);
    const responseTime = Date.now() - start;
    return {
      ok: response.ok,
      responseTime,
    };
  } catch (error) {
    return {
      ok: false,
      responseTime: null,
      error: error.message,
    };
  }
}

function formatUptime(ms) {
  const seconds = Math.floor(ms / 1000) % 60;
  const minutes = Math.floor(ms / (1000 * 60)) % 60;
  const hours = Math.floor(ms / (1000 * 60 * 60)) % 24;
  const days = Math.floor(ms / (1000 * 60 * 60 * 24));

  const parts = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);
  if (seconds > 0) parts.push(`${seconds}s`);

  return parts.join(' ') || '0s';
}

// ============================================================================
// Schema Exploration
// ============================================================================

// Get full schema
app.get('/api/schema', async (req, res) => {
  try {
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __schema {
              types {
                name
                kind
                description
                fields {
                  name
                  description
                  type {
                    name
                    kind
                  }
                }
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();
    res.json(result);
  } catch (error) {
    res.status(500).json({
      error: 'Failed to fetch schema',
      message: error.message,
    });
  }
});

// Get types summary
app.get('/api/schema/types', async (req, res) => {
  try {
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __schema {
              types {
                name
                kind
                description
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();

    if (result.data && result.data.__schema) {
      const types = result.data.__schema.types
        .filter(t =>
          !t.name.startsWith('__') &&
          ['OBJECT', 'INTERFACE', 'ENUM', 'SCALAR', 'INPUT'].includes(t.kind)
        )
        .sort((a, b) => a.name.localeCompare(b.name));

      res.json({ types });
    } else {
      res.json({ types: [] });
    }
  } catch (error) {
    res.status(500).json({
      error: 'Failed to fetch schema types',
      message: error.message,
    });
  }
});

// Get type details
app.get('/api/schema/type/:name', async (req, res) => {
  try {
    const typeName = req.params.name;

    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __type(name: "${typeName}") {
              name
              kind
              description
              fields {
                name
                description
                type {
                  name
                  kind
                  ofType {
                    name
                    kind
                  }
                }
              }
              interfaces {
                name
              }
              enumValues {
                name
                description
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();
    res.json(result.data);
  } catch (error) {
    res.status(500).json({
      error: 'Failed to fetch type details',
      message: error.message,
    });
  }
});

// ============================================================================
// Query Debugging
// ============================================================================

// Execute query with debugging
app.post('/api/debug/query', async (req, res) => {
  try {
    const { query, variables } = req.body;

    if (!query) {
      return res.status(400).json({ error: 'Query is required' });
    }

    const start = Date.now();
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query, variables: variables || {} }),
    });

    const result = await response.json();
    const duration = Date.now() - start;

    // Record metrics
    metrics.requests.push({
      timestamp: Date.now(),
      duration,
      hasError: result.errors ? true : false,
    });
    metrics.queryTimes.push(duration);

    if (result.errors) {
      metrics.errors.push({
        timestamp: Date.now(),
        error: result.errors[0],
      });
    }

    // Analyze query
    const analysis = analyzeQuery(query);

    res.json({
      result,
      debug: {
        duration,
        complexity: analysis.complexity,
        fieldCount: analysis.fieldCount,
        hasNestedFields: analysis.hasNestedFields,
        estimatedRows: analysis.estimatedRows,
      },
    });
  } catch (error) {
    metrics.errors.push({
      timestamp: Date.now(),
      error: error.message,
    });

    res.status(500).json({
      error: 'Failed to execute query',
      message: error.message,
    });
  }
});

// Analyze GraphQL query for insights
function analyzeQuery(query) {
  const fieldCount = (query.match(/{|}/g) || []).length / 2;
  const hasNestedFields = query.includes('{') && query.split('{').length > 3;

  let complexity = 'simple';
  if (fieldCount > 10) complexity = 'complex';
  else if (fieldCount > 5) complexity = 'moderate';

  let estimatedRows = '10-100';
  if (query.includes('limit')) {
    const match = query.match(/limit[:\s]+(\d+)/i);
    if (match) estimatedRows = `${match[1]}`;
  }

  return {
    fieldCount: Math.floor(fieldCount),
    hasNestedFields,
    complexity,
    estimatedRows,
  };
}

// ============================================================================
// Metrics & Performance
// ============================================================================

// Get metrics data
app.get('/api/metrics', (req, res) => {
  const timeWindow = parseInt(req.query.minutes || '60') * 60 * 1000;
  const cutoffTime = Date.now() - timeWindow;

  const recentRequests = metrics.requests.filter(r => r.timestamp > cutoffTime);
  const recentErrors = metrics.errors.filter(e => e.timestamp > cutoffTime);

  // Calculate buckets for histogram
  const buckets = {};
  for (let i = 0; i <= 1000; i += 100) {
    buckets[`${i}-${i + 100}ms`] = 0;
  }
  buckets['1000+ms'] = 0;

  recentRequests.forEach(req => {
    if (req.duration <= 1000) {
      const bucket = Math.floor(req.duration / 100) * 100;
      const key = `${bucket}-${bucket + 100}ms`;
      buckets[key]++;
    } else {
      buckets['1000+ms']++;
    }
  });

  res.json({
    period: {
      minutes: req.query.minutes || 60,
      from: new Date(cutoffTime),
      to: new Date(),
    },
    requests: {
      total: recentRequests.length,
      avgDuration: recentRequests.length > 0
        ? (recentRequests.reduce((a, b) => a + b.duration, 0) / recentRequests.length).toFixed(2)
        : 0,
      maxDuration: recentRequests.length > 0
        ? Math.max(...recentRequests.map(r => r.duration))
        : 0,
      minDuration: recentRequests.length > 0
        ? Math.min(...recentRequests.map(r => r.duration))
        : 0,
    },
    errors: {
      total: recentErrors.length,
      rate: recentRequests.length > 0
        ? ((recentErrors.length / recentRequests.length) * 100).toFixed(2)
        : 0,
    },
    histogram: buckets,
  });
});

// ============================================================================
// Logs
// ============================================================================

// Get system logs
app.get('/api/logs', (req, res) => {
  const limit = parseInt(req.query.limit || '100');
  const type = req.query.type || 'all'; // all, error, request

  let logs = [];

  // Combine and format logs
  if (type === 'all' || type === 'request') {
    logs = logs.concat(
      metrics.requests.map(r => ({
        timestamp: new Date(r.timestamp),
        type: 'request',
        duration: r.duration,
        status: r.hasError ? 'error' : 'success',
        message: `Query executed in ${r.duration}ms`,
      }))
    );
  }

  if (type === 'all' || type === 'error') {
    logs = logs.concat(
      metrics.errors.map(e => ({
        timestamp: new Date(e.timestamp),
        type: 'error',
        status: 'error',
        message: e.error.message || String(e.error),
      }))
    );
  }

  // Sort by timestamp descending
  logs.sort((a, b) => b.timestamp - a.timestamp);

  // Limit results
  logs = logs.slice(0, limit);

  res.json({ logs });
});

// ============================================================================
// Serve Main HTML
// ============================================================================

app.get('/', (req, res) => {
  res.sendFile(join(__dirname, '../public/index.html'));
});

app.get('/pages/:page', (req, res) => {
  res.sendFile(join(__dirname, '../public/index.html'));
});

// ============================================================================
// Error Handling
// ============================================================================

app.use((err, req, res, next) => {
  console.error('Server error:', err);
  res.status(500).json({
    error: 'Internal server error',
    message: process.env.NODE_ENV === 'development' ? err.message : undefined,
  });
});

app.use((req, res) => {
  res.status(404).json({ error: 'Not found' });
});

// ============================================================================
// Start Server
// ============================================================================

app.listen(port, () => {
  console.log(`\n✅ FraiseQL Admin Dashboard running on http://localhost:${port}`);
  console.log(`📊 API: http://localhost:${port}/api`);
  console.log(`🗣️  FraiseQL Server: ${fraiseqlApi}`);
  console.log(`\n🌐 Open your browser: http://localhost:${port}\n`);
});
