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

async function renderRealtime(el: HTMLElement): Promise<void> {
  el.innerHTML = `<div class="section-placeholder">${skeleton(3)}</div>`;
  try {
    const r = await apiFetch('/admin/v1/realtime/stats');
    const data = await r.json() as {
      connections: number;
      channels: string[];
      presence_rooms: { room: string; members: number }[];
      cdc_lag_ms: number | null;
    };
    el.innerHTML = `
      <h2>Realtime Monitor</h2>
      <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:1rem;margin-bottom:1.5rem">
        <div class="stat-card">
          <div class="stat-label">Connections</div>
          <div class="stat-value">${data.connections}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Channels</div>
          <div class="stat-value">${data.channels.length}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Presence Rooms</div>
          <div class="stat-value">${data.presence_rooms.length}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">CDC Lag</div>
          <div class="stat-value">
            ${data.cdc_lag_ms != null
              ? `<l-tooltip content="Replication lag in ms">${data.cdc_lag_ms} ms</l-tooltip>`
              : 'N/A'}
          </div>
        </div>
      </div>
      ${data.channels.length > 0
        ? `<h3>Channels</h3>${table(['name'], data.channels.map(c => ({ name: c })))}`
        : ''}`;
  } catch {
    el.innerHTML = emptyState('Realtime stats unavailable.');
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
// Tab routing — preserve selected tab in location.hash
// ---------------------------------------------------------------------------
const SECTIONS: Record<string, (el: HTMLElement) => Promise<void>> = {
  data:      renderData,
  auth:      renderAuth,
  storage:   renderStorage,
  functions: renderFunctions,
  realtime:  renderRealtime,
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
