//! The dev page served at `/`. Reads `/api/v1/schema` to build a form for
//! each text/toggle/dropdown/color/datetime field, then debounces config
//! changes into `POST /api/v1/preview.webp`. Reconnects its SSE stream on
//! watcher restart.
//!
//! This is intentionally a tiny vanilla-JS page, not the full pixlet React
//! frontend. It covers the core dev-loop for apps whose schemas only use
//! the simple field types. Typeahead, locationbased, and generated schemas
//! require the React frontend (phase 8b) to render their own UIs, but the
//! server-side endpoints are already wired so that port will not need
//! backend changes.

pub const INDEX_HTML: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>rustlet - {path}</title>
<style>
  :root {
    color-scheme: dark;
    --bg: #0b0b0f;
    --fg: #ddd;
    --muted: #777;
    --accent: #6b6;
    --error: #f55;
    --border: #222;
    --input-bg: #16171c;
  }
  html, body { height: 100%; margin: 0; }
  body {
    background: var(--bg);
    color: var(--fg);
    font: 14px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    display: flex;
    gap: 1.5rem;
    padding: 1.5rem;
    box-sizing: border-box;
  }
  #preview-pane {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
  }
  img#preview {
    image-rendering: pixelated;
    width: 512px;
    height: auto;
    border: 1px solid var(--border);
    background: #000;
  }
  #status { font-size: 0.8rem; color: var(--muted); }
  #status.live { color: var(--accent); }
  #status.error { color: var(--error); font-family: monospace; white-space: pre-wrap; text-align: left; max-width: 512px; }
  #form-pane {
    flex: 1;
    max-width: 420px;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  h1 { font-size: 1rem; margin: 0 0 0.5rem 0; color: #aaa; font-weight: 600; }
  .field { display: flex; flex-direction: column; gap: 0.25rem; }
  .field label { font-size: 0.8rem; color: #aaa; }
  .field .desc { font-size: 0.75rem; color: var(--muted); }
  .field input[type="text"],
  .field input[type="color"],
  .field input[type="datetime-local"],
  .field select {
    background: var(--input-bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.35rem 0.5rem;
    font: inherit;
  }
  .field input[type="checkbox"] {
    width: 1rem;
    height: 1rem;
  }
  .unsupported {
    font-size: 0.75rem;
    color: var(--muted);
    font-style: italic;
  }
</style>
</head>
<body>
<div id="preview-pane">
  <img id="preview" alt="preview">
  <div id="status">connecting...</div>
</div>
<div id="form-pane">
  <h1>{path}</h1>
  <div id="fields">loading schema...</div>
</div>
<script>
(() => {
  const img = document.getElementById('preview');
  const status = document.getElementById('status');
  const fieldsEl = document.getElementById('fields');
  const config = {};
  let debounce = 0;

  function setStatus(text, cls) {
    status.textContent = text;
    status.className = cls || '';
  }

  function setError(msg) {
    setStatus(msg, 'error');
  }

  function refreshPreview() {
    clearTimeout(debounce);
    debounce = setTimeout(doPreview, 200);
  }

  async function doPreview() {
    const form = new FormData();
    for (const [k, v] of Object.entries(config)) {
      form.append(k, v);
    }
    try {
      const resp = await fetch('api/v1/preview.webp', { method: 'POST', body: form });
      if (!resp.ok) {
        const text = await resp.text();
        setError(`preview ${resp.status}: ${text}`);
        return;
      }
      const blob = await resp.blob();
      img.src = URL.createObjectURL(blob);
      setStatus('live', 'live');
    } catch (e) {
      setError(`fetch: ${e}`);
    }
  }

  function makeField(field) {
    const wrap = document.createElement('div');
    wrap.className = 'field';
    const label = document.createElement('label');
    label.textContent = field.name || field.id;
    wrap.appendChild(label);

    const type = field.type;
    const id = field.id;

    let input;
    switch (type) {
      case 'text': {
        input = document.createElement('input');
        input.type = 'text';
        input.value = field.default || '';
        config[id] = input.value;
        input.addEventListener('input', () => {
          config[id] = input.value;
          refreshPreview();
        });
        break;
      }
      case 'onoff': {
        input = document.createElement('input');
        input.type = 'checkbox';
        const def = String(field.default) === 'true';
        input.checked = def;
        config[id] = def ? 'true' : 'false';
        input.addEventListener('change', () => {
          config[id] = input.checked ? 'true' : 'false';
          refreshPreview();
        });
        break;
      }
      case 'dropdown':
      case 'radio': {
        input = document.createElement('select');
        for (const opt of field.options || []) {
          const o = document.createElement('option');
          o.value = opt.value;
          o.textContent = opt.display || opt.text || opt.value;
          input.appendChild(o);
        }
        input.value = field.default || (field.options && field.options[0] ? field.options[0].value : '');
        config[id] = input.value;
        input.addEventListener('change', () => {
          config[id] = input.value;
          refreshPreview();
        });
        break;
      }
      case 'color': {
        input = document.createElement('input');
        input.type = 'color';
        input.value = field.default || '#000000';
        config[id] = input.value;
        input.addEventListener('input', () => {
          config[id] = input.value;
          refreshPreview();
        });
        break;
      }
      case 'datetime': {
        input = document.createElement('input');
        input.type = 'datetime-local';
        input.value = field.default || '';
        config[id] = input.value;
        input.addEventListener('input', () => {
          config[id] = input.value;
          refreshPreview();
        });
        break;
      }
      default: {
        const msg = document.createElement('div');
        msg.className = 'unsupported';
        msg.textContent = `(${type || 'unknown'} fields need the full UI)`;
        wrap.appendChild(msg);
        return wrap;
      }
    }

    wrap.appendChild(input);
    if (field.description) {
      const desc = document.createElement('div');
      desc.className = 'desc';
      desc.textContent = field.description;
      wrap.appendChild(desc);
    }
    return wrap;
  }

  async function loadSchema() {
    try {
      const resp = await fetch('api/v1/schema');
      const data = await resp.json();
      const fields = (data && Array.isArray(data.schema)) ? data.schema : [];
      fieldsEl.innerHTML = '';
      if (fields.length === 0) {
        const msg = document.createElement('div');
        msg.className = 'unsupported';
        msg.textContent = '(no schema fields declared)';
        fieldsEl.appendChild(msg);
      } else {
        for (const field of fields) {
          fieldsEl.appendChild(makeField(field));
        }
      }
    } catch (e) {
      fieldsEl.textContent = `schema load failed: ${e}`;
    }
    doPreview();
  }

  function connectSse() {
    const es = new EventSource('events');
    es.onopen = () => setStatus('live', 'live');
    es.onmessage = () => refreshPreview();
    es.onerror = () => setStatus('disconnected', 'error');
  }

  loadSchema();
  connectSse();
})();
</script>
</body>
</html>
"#;
