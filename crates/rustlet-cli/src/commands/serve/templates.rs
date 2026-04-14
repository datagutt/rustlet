// The dev page is served by `handlers::root`. `{path}` is substituted with the
// watched applet path at request time via `str::replace` so the title bar
// reflects what's being served.
pub const INDEX_HTML: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>rustlet - {path}</title>
<style>
  html, body { height: 100%; margin: 0; }
  body {
    background: #111;
    color: #ddd;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 1rem;
  }
  img#preview {
    image-rendering: pixelated;
    width: 512px;
    height: auto;
    border: 1px solid #333;
    background: #000;
  }
  #status {
    font-size: 0.8rem;
    color: #777;
  }
  #status.live { color: #6b6; }
  #status.error { color: #f55; white-space: pre-wrap; font-family: monospace; }
</style>
</head>
<body>
<img id="preview" src="/preview.webp">
<div id="status">connecting...</div>
<script>
  const img = document.getElementById('preview');
  const status = document.getElementById('status');
  const es = new EventSource('/events');
  es.onopen = () => {
    status.textContent = 'live';
    status.className = 'live';
  };
  es.onmessage = () => {
    img.src = '/preview.webp?t=' + Date.now();
  };
  es.onerror = () => {
    status.textContent = 'disconnected';
    status.className = 'error';
  };
</script>
</body>
</html>
"#;
