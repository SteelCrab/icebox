use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use icebox_task::store::TaskStore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "icebox-web", about = "Icebox local web UI")]
struct Cli {
    /// Path to the workspace (directory containing .icebox/)
    #[arg(long, default_value = ".")]
    path: PathBuf,

    /// Port to listen on
    #[arg(long, default_value_t = 3000)]
    port: u16,
}

struct AppState {
    store: TaskStore,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace = cli.path.canonicalize().unwrap_or(cli.path);
    let store = TaskStore::open(&workspace)?;
    let state = Arc::new(AppState { store });

    let app = Router::new()
        .route("/", get(serve_html))
        .route("/api/tasks", get(api_tasks))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", cli.port);
    println!("icebox web → http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_html() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    (StatusCode::OK, headers, HTML)
}

async fn api_tasks(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.list() {
        Ok(tasks) => {
            let json = serde_json::to_string(&tasks).unwrap_or_else(|_| "[]".into());
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, json)
        }
        Err(e) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                headers,
                format!(r#"{{"error":"{}"}}"#, e),
            )
        }
    }
}

static HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Icebox</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

  :root {
    --bg: #fafafa;
    --surface: #ffffff;
    --border: #e5e5e5;
    --text: #111111;
    --muted: #888888;
    --priority-critical: #ef4444;
    --priority-high: #f59e0b;
    font-size: 13px;
  }

  body {
    background: var(--bg);
    color: var(--text);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Header ── */
  header {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--surface);
    flex-shrink: 0;
  }

  header h1 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .subtitle {
    font-size: 12px;
    color: var(--muted);
  }

  .spacer { flex: 1; }

  .refresh-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 10px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
    white-space: nowrap;
  }
  .refresh-btn:hover { color: var(--text); border-color: #aaa; }

  /* ── Tab bar (mobile / tablet only) ── */
  .tab-bar {
    display: none;
    overflow-x: auto;
    flex-shrink: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 0 12px;
    gap: 0;
    scrollbar-width: none;
  }
  .tab-bar::-webkit-scrollbar { display: none; }

  .tab-btn {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 9px 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.15s, border-color 0.15s;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .tab-btn .tab-count {
    font-size: 11px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0 5px;
    line-height: 16px;
  }
  .tab-btn.active {
    color: var(--text);
    border-bottom-color: var(--tab-accent, #111);
  }
  .tab-btn.active .tab-count {
    background: var(--tab-accent, #111);
    border-color: var(--tab-accent, #111);
    color: #fff;
  }

  /* ── Board ── */
  .board {
    display: flex;
    flex: 1;
    overflow-x: auto;
    overflow-y: hidden;
    padding: 14px;
    gap: 12px;
  }

  /* ── Column ── */
  .column {
    flex: 0 0 220px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .col-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 2px 8px;
    border-bottom: 2px solid var(--col-accent);
    flex-shrink: 0;
  }

  .col-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--col-accent);
    flex-shrink: 0;
  }

  .col-name {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .col-count {
    font-size: 11px;
    color: var(--muted);
    margin-left: auto;
  }

  /* ── Cards ── */
  .cards {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
  }
  .cards::-webkit-scrollbar { width: 3px; }
  .cards::-webkit-scrollbar-track { background: transparent; }
  .cards::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 3px solid var(--p-color);
    border-radius: 6px;
    padding: 10px 11px;
    cursor: pointer;
    transition: box-shadow 0.12s, border-color 0.12s;
  }
  .card:hover {
    box-shadow: 0 2px 8px rgba(0,0,0,0.07);
    border-color: #ccc;
    border-left-color: var(--p-color);
  }

  .card-title {
    font-size: 13px;
    font-weight: 500;
    line-height: 1.45;
    margin-bottom: 7px;
  }

  .card-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }

  .priority-badge {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--p-color);
    padding: 1px 5px;
    border-radius: 3px;
    border: 1px solid var(--p-color);
    opacity: 0.85;
  }

  .tag {
    font-size: 10px;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
  }

  .due-date {
    font-size: 10px;
    color: var(--muted);
    margin-left: auto;
  }
  .due-date.overdue { color: var(--priority-critical); }
  .due-date.soon    { color: var(--priority-high); }

  /* ── Modal ── */
  .modal-overlay {
    display: none;
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.25);
    z-index: 100;
    align-items: center;
    justify-content: center;
  }
  .modal-overlay.open { display: flex; }

  .modal {
    background: var(--surface);
    border-radius: 10px;
    border: 1px solid var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.12);
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .modal-header {
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .modal-priority-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 4px;
  }

  .modal-title {
    font-size: 15px;
    font-weight: 600;
    line-height: 1.4;
    flex: 1;
  }

  .modal-close {
    background: none;
    border: none;
    font-size: 18px;
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
    padding: 0 2px;
  }
  .modal-close:hover { color: var(--text); }

  .modal-body {
    padding: 14px 18px;
    overflow-y: auto;
    flex: 1;
  }

  .modal-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    margin-bottom: 14px;
  }

  .meta-item { font-size: 11px; color: var(--muted); }
  .meta-item strong { color: var(--text); font-weight: 500; }

  .modal-content {
    font-size: 12.5px;
    line-height: 1.65;
    white-space: pre-wrap;
    font-family: inherit;
  }

  .empty {
    font-size: 12px;
    color: var(--muted);
    text-align: center;
    padding: 20px 0;
  }

  .loading {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    color: var(--muted);
    background: var(--bg);
    z-index: 200;
  }

  /* ── Responsive ── */

  /* Tablet: 600–899px — tab switching, single column visible */
  @media (max-width: 899px) {
    .tab-bar  { display: flex; }
    .board    { overflow-x: hidden; padding: 12px; }
    .column   { flex: 0 0 100%; display: none; }
    .column.active { display: flex; }
  }

  /* Mobile: <600px */
  @media (max-width: 599px) {
    header { padding: 10px 14px; }
    header h1 { font-size: 14px; }
    .subtitle { display: none; }
    .board { padding: 10px; }
    .card-title { font-size: 13px; }
    .modal { border-radius: 12px 12px 0 0; width: 100%; max-height: 85vh; position: fixed; bottom: 0; }
    .modal-overlay { align-items: flex-end; }
  }
</style>
</head>
<body>

<div class="loading" id="loading">Loading…</div>

<header>
  <h1>Icebox</h1>
  <span class="subtitle" id="subtitle"></span>
  <div class="spacer"></div>
  <button class="refresh-btn" onclick="loadTasks()">Refresh</button>
</header>

<div class="tab-bar" id="tab-bar"></div>

<div class="board" id="board"></div>

<div class="modal-overlay" id="modal" onclick="closeModal(event)">
  <div class="modal" id="modal-inner"></div>
</div>

<script>
const COLUMNS = [
  { key: 'icebox',     label: 'Icebox',      accent: '#94a3b8' },
  { key: 'emergency',  label: 'Emergency',   accent: '#ef4444' },
  { key: 'inprogress', label: 'In Progress', accent: '#3b82f6' },
  { key: 'testing',    label: 'Testing',     accent: '#f59e0b' },
  { key: 'complete',   label: 'Complete',    accent: '#22c55e' },
];

const PRIORITY = {
  low:      { label: 'Low',      color: '#94a3b8' },
  medium:   { label: 'Medium',   color: '#3b82f6' },
  high:     { label: 'High',     color: '#f59e0b' },
  critical: { label: 'Critical', color: '#ef4444' },
};

let allTasks = [];
let activeTab = COLUMNS[0].key;

async function loadTasks() {
  try {
    const res = await fetch('/api/tasks');
    allTasks = await res.json();
    render(allTasks);
    document.getElementById('loading').style.display = 'none';
    document.getElementById('subtitle').textContent =
      `${allTasks.length} task${allTasks.length !== 1 ? 's' : ''}`;
  } catch (e) {
    document.getElementById('loading').textContent = 'Failed to load tasks.';
  }
}

function render(tasks) {
  renderBoard(tasks);
  renderTabs(tasks);
  applyActiveTab();
}

function renderBoard(tasks) {
  const board = document.getElementById('board');
  board.innerHTML = '';
  for (const col of COLUMNS) {
    const colTasks = tasks.filter(t => t.column === col.key);
    const el = document.createElement('div');
    el.className = 'column';
    el.dataset.col = col.key;
    el.innerHTML = `
      <div class="col-header" style="--col-accent:${col.accent}">
        <div class="col-dot"></div>
        <span class="col-name">${col.label}</span>
        <span class="col-count">${colTasks.length}</span>
      </div>
      <div class="cards" id="col-${col.key}"></div>
    `;
    board.appendChild(el);

    const container = el.querySelector(`#col-${col.key}`);
    if (colTasks.length === 0) {
      container.innerHTML = '<div class="empty">—</div>';
    } else {
      for (const task of colTasks) container.appendChild(makeCard(task));
    }
  }
}

function renderTabs(tasks) {
  const bar = document.getElementById('tab-bar');
  bar.innerHTML = '';
  for (const col of COLUMNS) {
    const count = tasks.filter(t => t.column === col.key).length;
    const btn = document.createElement('button');
    btn.className = 'tab-btn';
    btn.dataset.col = col.key;
    btn.style.setProperty('--tab-accent', col.accent);
    btn.innerHTML = `${col.label}<span class="tab-count">${count}</span>`;
    btn.onclick = () => switchTab(col.key);
    bar.appendChild(btn);
  }
}

function switchTab(key) {
  activeTab = key;
  applyActiveTab();
  // scroll active tab button into view
  const btn = document.querySelector(`.tab-btn[data-col="${key}"]`);
  if (btn) btn.scrollIntoView({ inline: 'nearest', behavior: 'smooth' });
}

function applyActiveTab() {
  document.querySelectorAll('.column').forEach(el => {
    el.classList.toggle('active', el.dataset.col === activeTab);
  });
  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.col === activeTab);
  });
}

function makeCard(task) {
  const p = PRIORITY[task.priority] || PRIORITY.medium;
  const el = document.createElement('div');
  el.className = 'card';
  el.style.setProperty('--p-color', p.color);

  const tags = (task.tags || []).slice(0, 3)
    .map(t => `<span class="tag">${esc(t)}</span>`).join('');
  const due = task.due_date ? formatDue(task.due_date) : '';

  el.innerHTML = `
    <div class="card-title">${esc(task.title)}</div>
    <div class="card-meta">
      <span class="priority-badge">${p.label}</span>
      ${tags}${due}
    </div>
  `;
  el.onclick = () => openModal(task);
  return el;
}

function formatDue(iso) {
  const days = Math.ceil((new Date(iso) - Date.now()) / 86400000);
  let cls = 'due-date';
  if (days < 0) cls += ' overdue';
  else if (days <= 3) cls += ' soon';
  const label = days < 0 ? `${-days}d overdue` : days === 0 ? 'today' : `${days}d left`;
  return `<span class="${cls}">${label}</span>`;
}

function openModal(task) {
  const p = PRIORITY[task.priority] || PRIORITY.medium;
  const col = COLUMNS.find(c => c.key === task.column) || COLUMNS[0];
  const tags = (task.tags || []).map(t => `<span class="tag">${esc(t)}</span>`).join(' ');
  const body = task.body && task.body.trim()
    ? `<pre class="modal-content">${esc(task.body.trim())}</pre>` : '';
  const created = task.created_at ? new Date(task.created_at).toLocaleDateString() : '—';
  const due     = task.due_date   ? new Date(task.due_date).toLocaleDateString()   : '—';
  const start   = task.start_date ? new Date(task.start_date).toLocaleDateString() : '—';

  document.getElementById('modal-inner').innerHTML = `
    <div class="modal-header">
      <div class="modal-priority-dot" style="background:${p.color}"></div>
      <div class="modal-title">${esc(task.title)}</div>
      <button class="modal-close" onclick="closeModal()">×</button>
    </div>
    <div class="modal-body">
      <div class="modal-meta">
        <span class="meta-item"><strong>Column</strong> ${col.label}</span>
        <span class="meta-item"><strong>Priority</strong> ${p.label}</span>
        ${task.swimlane ? `<span class="meta-item"><strong>Swimlane</strong> ${esc(task.swimlane)}</span>` : ''}
        <span class="meta-item"><strong>Created</strong> ${created}</span>
        ${start !== '—' ? `<span class="meta-item"><strong>Start</strong> ${start}</span>` : ''}
        ${due   !== '—' ? `<span class="meta-item"><strong>Due</strong> ${due}</span>`     : ''}
        ${task.progress ? `<span class="meta-item"><strong>Progress</strong> ${task.progress.done}/${task.progress.total}</span>` : ''}
      </div>
      ${tags ? `<div style="margin-bottom:12px;display:flex;gap:5px;flex-wrap:wrap">${tags}</div>` : ''}
      ${body}
    </div>
  `;
  document.getElementById('modal').classList.add('open');
}

function closeModal(e) {
  if (!e || e.target === document.getElementById('modal'))
    document.getElementById('modal').classList.remove('open');
}

document.addEventListener('keydown', e => { if (e.key === 'Escape') closeModal(); });

function esc(s) {
  return String(s)
    .replace(/&/g,'&amp;').replace(/</g,'&lt;')
    .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

loadTasks();
</script>
</body>
</html>
"#;
