use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use ureq::http;

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::{AllocDict, DictRef};
use starlark::values::structs::AllocStruct;
use starlark::values::Value;
use starlark::values::none::NoneType;

// --- cache ---

struct CachedResponse {
    url: String,
    status_code: u16,
    status: String,
    body: String,
    headers: Vec<(String, String)>,
    expires_at: Instant,
}

static HTTP_CACHE: LazyLock<Mutex<HashMap<u64, CachedResponse>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache_key(method: &str, url: &str, headers: &[(String, String)], body: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    method.hash(&mut hasher);
    url.hash(&mut hasher);
    for (k, v) in headers {
        k.hash(&mut hasher);
        v.hash(&mut hasher);
    }
    body.hash(&mut hasher);
    hasher.finish()
}

fn get_cached(key: u64) -> Option<CachedResponse> {
    let mut cache = HTTP_CACHE.lock().ok()?;
    if let Some(entry) = cache.get(&key) {
        if Instant::now() < entry.expires_at {
            return Some(CachedResponse {
                url: entry.url.clone(),
                status_code: entry.status_code,
                status: entry.status.clone(),
                body: entry.body.clone(),
                headers: entry.headers.clone(),
                expires_at: entry.expires_at,
            });
        }
        cache.remove(&key);
    }
    None
}

fn put_cached(key: u64, resp: &CachedResponse) {
    if let Ok(mut cache) = HTTP_CACHE.lock() {
        // Evict expired entries if cache is getting large
        if cache.len() > 256 {
            let now = Instant::now();
            cache.retain(|_, v| v.expires_at > now);
        }
        cache.insert(key, CachedResponse {
            url: resp.url.clone(),
            status_code: resp.status_code,
            status: resp.status.clone(),
            body: resp.body.clone(),
            headers: resp.headers.clone(),
            expires_at: resp.expires_at,
        });
    }
}

// --- helpers ---

fn extract_string_dict(v: Value) -> anyhow::Result<Vec<(String, String)>> {
    if v.is_none() {
        return Ok(Vec::new());
    }
    let dict = DictRef::from_value(v)
        .ok_or_else(|| anyhow::anyhow!("expected dict, got {}", v.get_type()))?;
    let mut out = Vec::new();
    for (k, val) in dict.iter() {
        let key = k
            .unpack_str()
            .ok_or_else(|| anyhow::anyhow!("dict keys must be strings"))?
            .to_string();
        let value = val
            .unpack_str()
            .ok_or_else(|| anyhow::anyhow!("dict values must be strings"))?
            .to_string();
        out.push((key, value));
    }
    Ok(out)
}

fn alloc_cached_response<'v>(
    cached: &CachedResponse,
    eval: &mut Evaluator<'v, '_, '_>,
) -> Value<'v> {
    let heap = eval.heap();
    let headers_dict = heap.alloc(AllocDict(
        cached.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    ));
    heap.alloc(AllocStruct([
        ("url", heap.alloc(cached.url.as_str())),
        ("status_code", heap.alloc(cached.status_code as i32)),
        ("status", heap.alloc(cached.status.as_str())),
        ("body", heap.alloc(cached.body.as_str())),
        ("headers", headers_dict),
    ]))
}

fn do_request<'v>(
    method: &str,
    url: &str,
    headers: Value<'v>,
    body: &str,
    json_body: Value<'v>,
    params: Value<'v>,
    ttl_seconds: i32,
    eval: &mut Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    let mut final_url = url.to_string();

    let query_params = extract_string_dict(params)?;
    if !query_params.is_empty() {
        let sep = if final_url.contains('?') { "&" } else { "?" };
        let qs: Vec<String> = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect();
        final_url = format!("{final_url}{sep}{}", qs.join("&"));
    }

    let header_pairs = extract_string_dict(headers)?;
    let body_str = if !json_body.is_none() {
        json_body.unpack_str().unwrap_or("").to_string()
    } else {
        body.to_string()
    };

    // Check cache
    let key = cache_key(method, &final_url, &header_pairs, &body_str);
    if ttl_seconds > 0 {
        if let Some(cached) = get_cached(key) {
            return Ok(alloc_cached_response(&cached, eval));
        }
    }

    // Make the actual request
    let has_body = !body.is_empty() || !json_body.is_none();

    let response = if has_body {
        let mut req = match method {
            "POST" => ureq::post(&final_url),
            "PUT" => ureq::put(&final_url),
            "PATCH" => ureq::patch(&final_url),
            _ => ureq::post(&final_url),
        };
        for (k, v) in &header_pairs {
            req = req.header(k.as_str(), v.as_str());
        }
        if !json_body.is_none() {
            req.header("Content-Type", "application/json")
                .send(body_str.as_bytes())
        } else {
            req.send(body_str.as_bytes())
        }
    } else {
        let mut req = match method {
            "GET" => ureq::get(&final_url),
            "DELETE" => ureq::delete(&final_url),
            "HEAD" => ureq::head(&final_url),
            "OPTIONS" => ureq::options(&final_url),
            _ => ureq::get(&final_url),
        };
        for (k, v) in &header_pairs {
            req = req.header(k.as_str(), v.as_str());
        }
        req.call()
    };

    match response {
        Ok(res) => {
            let status_code = res.status().as_u16();
            let status_text = http_status_text(status_code);
            let status = format!("{status_code} {status_text}");

            let mut resp_headers: Vec<(String, String)> = Vec::new();
            for (name, value) in res.headers() {
                if let Ok(v) = value.to_str() {
                    resp_headers.push((name.to_string(), v.to_string()));
                }
            }

            let resp_body = res.into_body().read_to_string()?;

            // Store in cache if ttl > 0
            if ttl_seconds > 0 {
                let cached = CachedResponse {
                    url: final_url.clone(),
                    status_code,
                    status: status.clone(),
                    body: resp_body.clone(),
                    headers: resp_headers.clone(),
                    expires_at: Instant::now() + std::time::Duration::from_secs(ttl_seconds as u64),
                };
                put_cached(key, &cached);
            }

            let heap = eval.heap();
            let headers_dict = heap.alloc(AllocDict(
                resp_headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            ));
            let resp = heap.alloc(AllocStruct([
                ("url", heap.alloc(final_url.as_str())),
                ("status_code", heap.alloc(status_code as i32)),
                ("status", heap.alloc(status.as_str())),
                ("body", heap.alloc(resp_body.as_str())),
                ("headers", headers_dict),
            ]));
            Ok(resp)
        }
        Err(e) => Err(anyhow::anyhow!("HTTP request failed: {e}")),
    }
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn http_status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

#[starlark::starlark_module]
pub fn http_module(builder: &mut GlobalsBuilder) {
    fn get<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("GET", url, headers, "", Value::new_none(), params, ttl_seconds, eval)
    }

    fn post<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("POST", url, headers, body, json_body, params, ttl_seconds, eval)
    }

    fn put<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("PUT", url, headers, body, json_body, params, ttl_seconds, eval)
    }

    fn delete<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("DELETE", url, headers, "", Value::new_none(), params, ttl_seconds, eval)
    }

    fn patch<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("PATCH", url, headers, body, json_body, params, ttl_seconds, eval)
    }
}

pub fn build_http_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(http_module)
        .build()
}
