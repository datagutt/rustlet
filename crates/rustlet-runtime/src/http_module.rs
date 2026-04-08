use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use base64::Engine;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::DictRef;
use starlark::values::none::NoneType;
use starlark::values::tuple::TupleRef;
use starlark::values::Value;

use crate::json_module::starlark_to_serde;
use crate::starlark_response::StarlarkResponse;

thread_local! {
    static CURRENT_APP_ID: RefCell<String> = const { RefCell::new(String::new()) };
}

pub(crate) fn set_request_context(id: &str) {
    CURRENT_APP_ID.with(|slot| {
        let app_id = id.split('/').next().unwrap_or(id).to_string();
        *slot.borrow_mut() = app_id;
    });
}

fn current_app_id() -> String {
    CURRENT_APP_ID.with(|slot| slot.borrow().clone())
}

struct CachedResponse {
    url: String,
    status_code: u16,
    status: String,
    encoding: String,
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
                encoding: entry.encoding.clone(),
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
        if cache.len() > 256 {
            let now = Instant::now();
            cache.retain(|_, v| v.expires_at > now);
        }
        cache.insert(
            key,
            CachedResponse {
                url: resp.url.clone(),
                status_code: resp.status_code,
                status: resp.status.clone(),
                encoding: resp.encoding.clone(),
                body: resp.body.clone(),
                headers: resp.headers.clone(),
                expires_at: resp.expires_at,
            },
        );
    }
}

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

fn extract_auth(value: Value) -> anyhow::Result<Option<(String, String)>> {
    if value.is_none() {
        return Ok(None);
    }
    let tuple = TupleRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("auth must be a tuple of username and password"))?;
    if tuple.len() != 2 {
        return Err(anyhow::anyhow!("expected two values for auth params tuple"));
    }
    let username = tuple.content()[0]
        .unpack_str()
        .ok_or_else(|| anyhow::anyhow!("auth username must be a string"))?
        .to_string();
    let password = tuple.content()[1]
        .unpack_str()
        .ok_or_else(|| anyhow::anyhow!("auth password must be a string"))?
        .to_string();
    Ok(Some((username, password)))
}

fn build_request_body<'v>(
    body: &str,
    json_body: Value<'v>,
    form_body: Value<'v>,
    form_encoding: &str,
) -> anyhow::Result<(Vec<u8>, Option<String>)> {
    if !body.is_empty() {
        return Ok((body.as_bytes().to_vec(), None));
    }

    if !json_body.is_none() {
        let json = serde_json::to_vec(&starlark_to_serde(json_body)?)
            .map_err(|e| anyhow::anyhow!("JSON encode error: {e}"))?;
        return Ok((json, Some("application/json".to_string())));
    }

    let fields = extract_string_dict(form_body)?;
    if fields.is_empty() {
        return Ok((Vec::new(), None));
    }

    match form_encoding {
        "" | "application/x-www-form-urlencoded" => {
            let encoded = fields
                .iter()
                .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            Ok((
                encoded.into_bytes(),
                Some("application/x-www-form-urlencoded".to_string()),
            ))
        }
        "multipart/form-data" => {
            let boundary = "rustlet-boundary";
            let mut body = Vec::new();
            for (key, value) in fields {
                body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{key}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(value.as_bytes());
                body.extend_from_slice(b"\r\n");
            }
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            Ok((
                body,
                Some(format!("multipart/form-data; boundary={boundary}")),
            ))
        }
        _ => Err(anyhow::anyhow!("unknown form encoding: {form_encoding}")),
    }
}

fn alloc_cached_response<'v>(
    cached: &CachedResponse,
    eval: &mut Evaluator<'v, '_, '_>,
) -> Value<'v> {
    eval.heap().alloc(StarlarkResponse {
        url: cached.url.clone(),
        status_code: cached.status_code,
        status: cached.status.clone(),
        encoding: cached.encoding.clone(),
        body: cached.body.clone(),
        headers: cached.headers.clone(),
    })
}

fn do_request<'v>(
    method: &str,
    url: &str,
    headers: Value<'v>,
    body: &str,
    json_body: Value<'v>,
    form_body: Value<'v>,
    form_encoding: &str,
    auth: Value<'v>,
    params: Value<'v>,
    ttl_seconds: i32,
    eval: &mut Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    let mut final_url = url.to_string();
    let query_params = extract_string_dict(params)?;
    if !query_params.is_empty() {
        let sep = if final_url.contains('?') { "&" } else { "?" };
        let qs = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        final_url = format!("{final_url}{sep}{qs}");
    }

    let mut header_pairs = extract_string_dict(headers)?;
    if ttl_seconds >= 0 {
        header_pairs.push((
            "X-Tidbyt-Cache-Seconds".to_string(),
            ttl_seconds.to_string(),
        ));
    }
    let app_id = current_app_id();
    if !app_id.is_empty() {
        header_pairs.push(("X-Tidbyt-App".to_string(), app_id));
    }

    if let Some((username, password)) = extract_auth(auth)? {
        let token =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        header_pairs.push(("Authorization".to_string(), format!("Basic {token}")));
    }

    let (payload, content_type) = build_request_body(body, json_body, form_body, form_encoding)?;
    let payload_key = String::from_utf8_lossy(&payload).into_owned();

    let key = cache_key(method, &final_url, &header_pairs, &payload_key);
    if ttl_seconds > 0 {
        if let Some(cached) = get_cached(key) {
            return Ok(alloc_cached_response(&cached, eval));
        }
    }

    let has_content_type = header_pairs
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    if let Some(content_type) = content_type {
        if !has_content_type {
            header_pairs.push(("Content-Type".to_string(), content_type));
        }
    }

    let has_body = !payload.is_empty();
    let response = if has_body {
        let mut req = match method {
            "POST" => ureq::post(&final_url),
            "PUT" => ureq::put(&final_url),
            "PATCH" => ureq::patch(&final_url),
            _ => ureq::post(&final_url),
        };
        for (name, value) in &header_pairs {
            req = req.header(name, value);
        }
        req.send(&payload)
    } else {
        let mut req = match method {
            "GET" => ureq::get(&final_url),
            "DELETE" => ureq::delete(&final_url),
            "OPTIONS" => ureq::options(&final_url),
            "HEAD" => ureq::head(&final_url),
            _ => ureq::get(&final_url),
        };
        for (name, value) in &header_pairs {
            req = req.header(name, value);
        }
        req.call()
    };

    match response {
        Ok(res) => {
            let status_code = res.status().as_u16();
            let status = format!("{status_code} {}", http_status_text(status_code));

            let mut header_map: HashMap<String, Vec<String>> = HashMap::new();
            for (name, value) in res.headers() {
                if let Ok(value) = value.to_str() {
                    header_map
                        .entry(canonical_header_name(name.as_str()))
                        .or_default()
                        .push(value.to_string());
                }
            }

            let mut resp_headers = header_map
                .iter()
                .map(|(name, values)| (name.clone(), values.join(",")))
                .collect::<Vec<_>>();
            resp_headers.sort_by(|a, b| a.0.cmp(&b.0));

            let encoding = resp_headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("transfer-encoding"))
                .or_else(|| {
                    resp_headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
                })
                .map(|(_, value)| value.clone())
                .unwrap_or_default();

            let resp_body = res.into_body().read_to_string()?;

            if ttl_seconds > 0 {
                let cached = CachedResponse {
                    url: final_url.clone(),
                    status_code,
                    status: status.clone(),
                    encoding: encoding.clone(),
                    body: resp_body.clone(),
                    headers: resp_headers.clone(),
                    expires_at: Instant::now() + std::time::Duration::from_secs(ttl_seconds as u64),
                };
                put_cached(key, &cached);
            }

            Ok(eval.heap().alloc(StarlarkResponse {
                url: final_url,
                status_code,
                status,
                encoding,
                body: resp_body,
                headers: resp_headers,
            }))
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
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{:02X}", b)),
        }
    }
    encoded
}

fn canonical_header_name(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut canonical = String::new();
                    canonical.push(first.to_ascii_uppercase());
                    canonical.extend(chars.map(|c| c.to_ascii_lowercase()));
                    canonical
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
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
        do_request(
            "GET",
            url,
            headers,
            "",
            Value::new_none(),
            Value::new_none(),
            "",
            Value::new_none(),
            params,
            ttl_seconds,
            eval,
        )
    }

    fn post<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] form_body: Value<'v>,
        #[starlark(default = "")] form_encoding: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = NoneType)] auth: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request(
            "POST",
            url,
            headers,
            body,
            json_body,
            form_body,
            form_encoding,
            auth,
            params,
            ttl_seconds,
            eval,
        )
    }

    fn put<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] form_body: Value<'v>,
        #[starlark(default = "")] form_encoding: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = NoneType)] auth: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request(
            "PUT",
            url,
            headers,
            body,
            json_body,
            form_body,
            form_encoding,
            auth,
            params,
            ttl_seconds,
            eval,
        )
    }

    fn delete<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request(
            "DELETE",
            url,
            headers,
            "",
            Value::new_none(),
            Value::new_none(),
            "",
            Value::new_none(),
            params,
            ttl_seconds,
            eval,
        )
    }

    fn patch<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] form_body: Value<'v>,
        #[starlark(default = "")] form_encoding: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        #[starlark(default = NoneType)] auth: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request(
            "PATCH",
            url,
            headers,
            body,
            json_body,
            form_body,
            form_encoding,
            auth,
            params,
            ttl_seconds,
            eval,
        )
    }

    fn options<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = 0)] ttl_seconds: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request(
            "OPTIONS",
            url,
            headers,
            "",
            Value::new_none(),
            Value::new_none(),
            "",
            Value::new_none(),
            params,
            ttl_seconds,
            eval,
        )
    }

    fn status_text(code: i32) -> anyhow::Result<String> {
        Ok(http_status_text(code as u16).to_string())
    }
}

pub fn build_http_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(http_module)
        .build()
}
