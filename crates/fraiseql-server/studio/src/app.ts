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
let loginToast: HTMLElement | null = null;

function showLogin(): void {
  const dialog = document.getElementById('login-dialog') as HTMLDialogElement | null;
  dialog?.showModal?.();
}

function hideLogin(): void {
  const dialog = document.getElementById('login-dialog') as HTMLDialogElement | null;
  dialog?.close?.();
}

// ---------------------------------------------------------------------------
// Section renderers (placeholders — wired to real endpoints in Cycle 9)
// ---------------------------------------------------------------------------

function renderData(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:60%;margin-bottom:1rem"></l-skeleton>
      <l-skeleton style="height:1rem;width:80%;margin-bottom:0.5rem"></l-skeleton>
      <l-skeleton style="height:1rem;width:70%"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Data browser loading…</p>
    </div>`;
  apiFetch('/admin/v1/health/detailed').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Could not load data browser. Check admin credentials.</p>`;
  });
}

function renderAuth(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:50%;margin-bottom:1rem"></l-skeleton>
      <l-skeleton style="height:1rem;width:75%;margin-bottom:0.5rem"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Auth user management loading…</p>
    </div>`;
  apiFetch('/admin/v1/users').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Auth management unavailable.</p>`;
  });
}

function renderStorage(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:55%;margin-bottom:1rem"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Storage browser loading…</p>
    </div>`;
  apiFetch('/admin/v1/storage/buckets').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Storage browser unavailable.</p>`;
  });
}

function renderFunctions(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:45%;margin-bottom:1rem"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Functions loading…</p>
    </div>`;
  apiFetch('/admin/v1/functions').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Function operations unavailable.</p>`;
  });
}

function renderRealtime(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:50%;margin-bottom:1rem"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Realtime monitor loading…</p>
    </div>`;
  apiFetch('/admin/v1/realtime/stats').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Realtime stats unavailable.</p>`;
  });
}

function renderMetrics(el: HTMLElement): void {
  el.innerHTML = `
    <div class="section-placeholder">
      <l-skeleton style="height:2rem;width:40%;margin-bottom:1rem"></l-skeleton>
      <p style="margin-top:1.5rem;color:var(--color-text-secondary)">Metrics loading…</p>
    </div>`;
  apiFetch('/admin/v1/metrics/summary').then(r => r.json()).then((data: unknown) => {
    el.innerHTML = `<pre class="log-viewer">${JSON.stringify(data, null, 2)}</pre>`;
  }).catch(() => {
    el.innerHTML = `<p class="empty-state">Metrics unavailable.</p>`;
  });
}

// ---------------------------------------------------------------------------
// Tab routing — preserve selected tab in location.hash
// ---------------------------------------------------------------------------
const SECTIONS: Record<string, (el: HTMLElement) => void> = {
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
  renderer(content);
  location.hash = name;
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

  // Wire tabs
  const tabs = document.querySelector('l-tabs');
  tabs?.addEventListener('l-tab-change', (e: Event) => {
    const detail = (e as CustomEvent<{ value: string }>).detail;
    activateSection(detail.value);
  });

  // Restore active section from hash
  const initial = location.hash.slice(1) || 'data';
  activateSection(initial);

  loginToast = document.getElementById('login-toast');
});
