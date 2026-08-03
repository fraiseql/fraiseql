// FraiseQL Studio — Runtime Admin Dashboard
// Luxen UI components (HTML-first web components built on Lit)
// Import via direct dist paths to ensure side effects (customElements.define) run.
import 'luxen-ui/dist/elements/tabs/index.js';
import 'luxen-ui/dist/elements/dialog/index.js';
import 'luxen-ui/dist/elements/toast/index.js';
import 'luxen-ui/dist/elements/skeleton/index.js';
import 'luxen-ui/dist/elements/spinner/index.js';
import 'luxen-ui/dist/elements/badge/index.js';
import 'luxen-ui/dist/elements/tooltip/index.js';
import 'luxen-ui/dist/elements/dropdown/index.js';
import 'luxen-ui/dist/elements/dropdown-item/index.js';
import 'luxen-ui/dist/elements/avatar/index.js';
// Note: l-details not in v0.1.2 — using native <details> for collapsible sections.

// ---------------------------------------------------------------------------
// Fetch wrapper — injects admin bearer token and maps 401 → redirect to login
// ---------------------------------------------------------------------------
function getAdminToken(): string {
  return sessionStorage.getItem('studio_admin_token') ?? '';
}

async function apiFetch(path: string, options: RequestInit = {}): Promise<Response> {
  const token = getAdminToken();
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> | undefined ?? {}),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  const res = await fetch(path, { ...options, headers });
  if (res.status === 401) {
    showLogin();
    throw new Error('Unauthorized — redirected to login');
  }
  return res;
}

// ---------------------------------------------------------------------------
// Login overlay
// ---------------------------------------------------------------------------

function showLogin(): void {
  const dialog = document.getElementById('login-dialog') as HTMLDialogElement | null;
  dialog?.showModal?.();
}

function hideLogin(): void {
  const dialog = document.getElementById('login-dialog') as HTMLDialogElement | null;
  dialog?.close?.();
}

// ---------------------------------------------------------------------------
// Shared rendering helpers
// ---------------------------------------------------------------------------

function skeleton(lines = 3): string {
  return Array.from({ length: lines }, (_, i) =>
    `<l-skeleton style="height:1rem;width:${80 - i * 10}%;margin-bottom:0.5rem"></l-skeleton>`
  ).join('\n');
}

function emptyState(msg: string): string {
  return `<p class="empty-state">${msg}</p>`;
}

function jsonViewer(data: unknown): string {
  return `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
}

function table(
  headers: string[],
  rows: Record<string, unknown>[],
  emptyMsg = 'No records found.'
): string {
  if (rows.length === 0) return emptyState(emptyMsg);
  const head = headers.map(h => `<th>${h}</th>`).join('');
  const body = rows.map(row =>
    `<tr>${headers.map(h => `<td>${row[h] ?? ''}</td>`).join('')}</tr>`
  ).join('\n');
  return `
    <div style="overflow-x:auto">
      <table class="admin-table">
        <thead><tr>${head}</tr></thead>
        <tbody>${body}</tbody>
      </table>
    </div>`;
}

// ---------------------------------------------------------------------------
// Section renderers
// ---------------------------------------------------------------------------

async function renderData(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(4)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/schema');
    const { schema } = await r.json() as { schema: { types?: { name: string }[] } };
    const types = schema.types ?? [];
    if (types.length === 0) {
      el.innerHTML = emptyState('No entity types found in the compiled schema.');
      return;
    }
    el.innerHTML = `
      <h2>Data Browser</h2>
      <p style="color:var(--color-text-secondary)">
        ${types.length} entity type(s) available. Select an entity to browse rows.
      </p>
      ${table(['name'], types)}`;
  } catch {
    el.innerHTML = emptyState('Data browser unavailable. Check admin credentials.');
  }
}

async function renderAuth(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(3)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/users');
    const data = await r.json() as {
      users: { sub: string; email: string; provider: string; mfa_enrolled: boolean }[];
      total: number;
    };
    el.innerHTML = `
      <h2>Auth Users <l-badge variant="neutral">${data.total}</l-badge></h2>
      ${table(['sub', 'email', 'provider', 'mfa_enrolled'], data.users, 'No users found.')}`;
  } catch {
    el.innerHTML = emptyState('Auth management unavailable.');
  }
}

async function renderStorage(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(3)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/storage/buckets');
    const data = await r.json() as { buckets: { name: string; object_count: number }[] };
    el.innerHTML = `
      <h2>Storage Buckets</h2>
      ${table(['name', 'object_count'], data.buckets, 'No buckets configured.')}`;
  } catch {
    el.innerHTML = emptyState('Storage browser unavailable.');
  }
}

async function renderFunctions(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(3)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/functions');
    const data = await r.json() as {
      functions: { name: string; version: number; runtime: string; status: string }[];
    };
    el.innerHTML = `
      <h2>Deployed Functions</h2>
      ${table(['name', 'version', 'runtime', 'status'], data.functions, 'No functions deployed.')}`;
  } catch {
    el.innerHTML = emptyState('Function operations unavailable.');
  }
}

async function renderMetrics(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(4)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/metrics/summary');
    const data = await r.json() as {
      latency: { p50_ms: number; p95_ms: number; p99_ms: number };
      errors: { rate_5m: number; rate_1h: number; rate_24h: number };
      pool: { active: number; idle: number; max: number; utilization: number };
      cache: { hit_rate: number; entries: number };
      subscriptions: { active: number };
    };
    el.innerHTML = `
      <h2>Metrics</h2>
      <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:1rem">
        <div class="stat-card">
          <div class="stat-label">Latency (P50 / P95 / P99)</div>
          <div class="stat-value">
            <l-tooltip content="Median latency">${data.latency.p50_ms} ms</l-tooltip> /
            <l-tooltip content="95th percentile">${data.latency.p95_ms} ms</l-tooltip> /
            <l-tooltip content="99th percentile">${data.latency.p99_ms} ms</l-tooltip>
          </div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Error Rate (5m / 1h / 24h)</div>
          <div class="stat-value">
            ${(data.errors.rate_5m * 100).toFixed(2)}% /
            ${(data.errors.rate_1h * 100).toFixed(2)}% /
            ${(data.errors.rate_24h * 100).toFixed(2)}%
          </div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Pool (active / idle / max)</div>
          <div class="stat-value">${data.pool.active} / ${data.pool.idle} / ${data.pool.max}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Cache Hit Rate</div>
          <div class="stat-value">${(data.cache.hit_rate * 100).toFixed(1)}%
            <l-badge variant="neutral">${data.cache.entries} entries</l-badge>
          </div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Active Subscriptions</div>
          <div class="stat-value">${data.subscriptions.active}</div>
        </div>
      </div>`;
  } catch {
    el.innerHTML = emptyState('Metrics unavailable.');
  }
}

// ---------------------------------------------------------------------------
// Schema browser (#373) — types, queries, mutations from the compiled schema
// ---------------------------------------------------------------------------

/** Render a compiled-schema field type (externally-tagged serde enum) tersely. */
function fmtFieldType(ft: unknown): string {
  if (typeof ft === 'string') return ft;
  if (ft && typeof ft === 'object') {
    const [tag, inner] = Object.entries(ft as Record<string, unknown>)[0] ?? ['?', null];
    if (tag === 'List') return `[${fmtFieldType(inner)}]`;
    return typeof inner === 'string' ? inner : tag;
  }
  return String(ft ?? '');
}

interface SchemaField { name: string; field_type: unknown; nullable?: boolean }
interface SchemaType { name: string; sql_source?: string; fields?: SchemaField[] }
interface SchemaQuery { name: string; return_type: string; returns_list?: boolean; relay?: boolean; sql_source?: string }
interface SchemaMutation { name: string; return_type: string; sql_source?: string }

async function renderSchema(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(4)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/schema');
    const { schema } = await r.json() as {
      schema: { types?: SchemaType[]; queries?: SchemaQuery[]; mutations?: SchemaMutation[] };
    };
    const types = schema.types ?? [];
    const queries = schema.queries ?? [];
    const mutations = schema.mutations ?? [];

    const typeBlocks = types.map(t => {
      const fields = (t.fields ?? []).map(f => ({
        field: f.name,
        type: fmtFieldType(f.field_type) + (f.nullable ? '' : '!'),
      }));
      return `
        <details>
          <summary><strong>${t.name}</strong>
            <span style="color:var(--color-text-secondary)"> — backed by ${t.sql_source || '(no view)'}</span>
          </summary>
          ${table(['field', 'type'], fields, 'No fields declared.')}
        </details>`;
    }).join('\n');

    el.innerHTML = `
      <h2>Schema <l-badge variant="neutral">${types.length} types</l-badge></h2>
      ${typeBlocks || emptyState('No entity types in the compiled schema.')}
      <h3>Queries</h3>
      ${table(['name', 'return_type', 'kind'], queries.map(q => ({
        name: q.name,
        return_type: q.return_type,
        kind: q.relay ? 'relay connection' : q.returns_list ? 'list' : 'single',
      })), 'No queries.')}
      <h3>Mutations</h3>
      ${table(['name', 'return_type', 'sql_source'], mutations.map(m => ({
        name: m.name,
        return_type: m.return_type,
        sql_source: m.sql_source ?? '',
      })), 'No mutations.')}`;
  } catch {
    el.innerHTML = emptyState('Schema browser unavailable. Check admin credentials.');
  }
}

// ---------------------------------------------------------------------------
// Observers (#373) — delivery health + DLQ viewer with retry
// ---------------------------------------------------------------------------

interface DlqItem {
  id: string; entity_type: string; event_type: string; action_type: string;
  error_message: string; attempts: number;
}

async function renderObservers(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(4)}</div>`;
  try {
    const [healthRes, dlqRes] = await Promise.all([
      apiFetch('/api/observers/delivery/health'),
      apiFetch('/api/observers/dlq?limit=25'),
    ]);
    if (!healthRes.ok || !dlqRes.ok) {
      el.innerHTML = emptyState('Observers unavailable (feature disabled or not configured).');
      return;
    }
    const health = await healthRes.json() as {
      running: boolean; observer_count: number; events_processed: number;
      errors: number; dlq_count: number; dlq_dropped: number;
    };
    const dlq = await dlqRes.json() as { items: DlqItem[]; total: number };

    const rows = dlq.items.map(item => `
      <tr>
        <td>${item.entity_type}</td>
        <td>${item.event_type}</td>
        <td>${item.action_type}</td>
        <td>${item.attempts}</td>
        <td style="max-width:24rem;overflow:hidden;text-overflow:ellipsis">${item.error_message}</td>
        <td><button class="retry-btn" data-retry-id="${item.id}">Retry</button></td>
      </tr>`).join('\n');

    el.innerHTML = `
      <h2>Observers
        <l-badge variant="${health.running ? 'success' : 'danger'}">
          ${health.running ? 'running' : 'stopped'}
        </l-badge>
      </h2>
      <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:1rem">
        <div class="stat-card"><div class="stat-label">Observers</div>
          <div class="stat-value">${health.observer_count}</div></div>
        <div class="stat-card"><div class="stat-label">Events Processed</div>
          <div class="stat-value">${health.events_processed}</div></div>
        <div class="stat-card"><div class="stat-label">Errors</div>
          <div class="stat-value">${health.errors}</div></div>
        <div class="stat-card"><div class="stat-label">DLQ (held / dropped)</div>
          <div class="stat-value">${health.dlq_count} / ${health.dlq_dropped}</div></div>
      </div>
      <h3>Dead Letter Queue <l-badge variant="neutral">${dlq.total}</l-badge>
        <button id="retry-all-btn" ${dlq.total === 0 ? 'disabled' : ''}>Retry all</button>
      </h3>
      ${dlq.items.length === 0 ? emptyState('DLQ is empty.') : `
      <div style="overflow-x:auto"><table class="admin-table">
        <thead><tr><th>entity</th><th>event</th><th>action</th><th>attempts</th><th>error</th><th></th></tr></thead>
        <tbody>${rows}</tbody>
      </table></div>`}`;

    // Wire the retry actions to the DLQ endpoints; re-render to show the effect.
    el.querySelectorAll<HTMLButtonElement>('[data-retry-id]').forEach(btn => {
      btn.addEventListener('click', async () => {
        btn.disabled = true;
        try {
          await apiFetch(`/api/observers/dlq/${btn.dataset.retryId}/retry`, { method: 'POST' });
        } finally {
          void renderObservers(el);
        }
      });
    });
    document.getElementById('retry-all-btn')?.addEventListener('click', async () => {
      try {
        await apiFetch('/api/observers/dlq/retry-all', { method: 'POST' });
      } finally {
        void renderObservers(el);
      }
    });
  } catch {
    el.innerHTML = emptyState('Observers unavailable (feature disabled or not configured).');
  }
}

// ---------------------------------------------------------------------------
// Logs (#373) — observer dispatch history
// ---------------------------------------------------------------------------

interface ObserverLogRow {
  entity_type: string; event_type: string; status: string;
  action_type?: string | null; duration_ms?: number | null;
  error_message?: string | null; started_at?: string | null;
}

async function renderLogs(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(5)}</div>`;
  try {
    const r = await apiFetch('/api/observers/logs?page_size=50');
    if (!r.ok) {
      el.innerHTML = emptyState('Logs unavailable (observers feature disabled).');
      return;
    }
    const logs = await r.json() as { data: ObserverLogRow[]; total_count: number };
    el.innerHTML = `
      <h2>Observer Logs <l-badge variant="neutral">${logs.total_count}</l-badge></h2>
      ${table(
        ['started_at', 'entity_type', 'event_type', 'action_type', 'status', 'duration_ms', 'error_message'],
        logs.data.map(row => ({
          started_at: row.started_at ?? '',
          entity_type: row.entity_type,
          event_type: row.event_type,
          action_type: row.action_type ?? '',
          status: row.status,
          duration_ms: row.duration_ms ?? '',
          error_message: row.error_message ?? '',
        })),
        'No dispatch history recorded.'
      )}`;
  } catch {
    el.innerHTML = emptyState('Logs unavailable (observers feature disabled).');
  }
}

// ---------------------------------------------------------------------------
// Tab routing — preserve selected tab in location.hash
// ---------------------------------------------------------------------------
const SECTIONS: Record<string, (el: HTMLElement) => Promise<void>> = {
  schema:    renderSchema,
  data:      renderData,
  auth:      renderAuth,
  storage:   renderStorage,
  functions: renderFunctions,
  observers: renderObservers,
  logs:      renderLogs,
  metrics:   renderMetrics,
};

function activateSection(name: string): void {
  const content = document.getElementById('section-content');
  if (!content) return;
  const renderer = SECTIONS[name] ?? renderData;
  // Show spinner while loading
  content.innerHTML = `<l-spinner></l-spinner>`;
  renderer(content).catch(() => {
    content.innerHTML = `<p class="empty-state">Failed to load ${name} section.</p>`;
  });
  location.hash = name;

  // Sync the l-tabs value attribute
  const tabs = document.querySelector('l-tabs');
  if (tabs) {
    tabs.setAttribute('value', name);
  }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------
document.addEventListener('DOMContentLoaded', () => {
  // Wire login form
  const loginForm = document.getElementById('login-form');
  loginForm?.addEventListener('submit', (e) => {
    e.preventDefault();
    const input = document.getElementById('token-input') as HTMLInputElement | null;
    if (input?.value) {
      sessionStorage.setItem('studio_admin_token', input.value);
      hideLogin();
      const active = location.hash.slice(1) || 'data';
      activateSection(active);
    }
  });

  // Wire tab change events
  const tabs = document.querySelector('l-tabs');
  tabs?.addEventListener('l-tab-change', (e: Event) => {
    const detail = (e as CustomEvent<{ value: string }>).detail;
    activateSection(detail.value);
  });

  // Add inline styles for stat cards and table
  const style = document.createElement('style');
  style.textContent = `
    .stat-card {
      background: var(--color-surface-raised, #fff);
      border: 1px solid var(--color-border, #e2e2e2);
      border-radius: 8px;
      padding: 1rem;
    }
    .stat-label {
      font-size: 0.75rem;
      color: var(--color-text-secondary, #666);
      margin-bottom: 0.25rem;
    }
    .stat-value {
      font-size: 1.25rem;
      font-weight: 600;
    }
    .admin-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.875rem;
    }
    .admin-table th {
      text-align: left;
      padding: 0.5rem 0.75rem;
      background: var(--color-surface, #f8f8f8);
      border-bottom: 2px solid var(--color-border, #e2e2e2);
      font-weight: 600;
    }
    .admin-table td {
      padding: 0.5rem 0.75rem;
      border-bottom: 1px solid var(--color-border, #e2e2e2);
    }
    .admin-table tr:hover td {
      background: var(--color-surface, #f8f8f8);
    }
  `;
  document.head.appendChild(style);

  // Load the initial section
  const initial = location.hash.slice(1) || 'data';
  // Show login if no token stored
  if (!getAdminToken()) {
    showLogin();
  } else {
    activateSection(initial);
  }
});
