use ureq::http;

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::{AllocDict, DictRef};
use starlark::values::structs::AllocStruct;
use starlark::values::Value;
use starlark::values::none::NoneType;

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

fn do_request<'v>(
    method: &str,
    url: &str,
    headers: Value<'v>,
    body: &str,
    json_body: Value<'v>,
    params: Value<'v>,
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
        if !body.is_empty() {
            req.send(body.as_bytes())
        } else {
            let json_str = json_body.unpack_str().unwrap_or("");
            req.header("Content-Type", "application/json")
                .send(json_str.as_bytes())
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
        Ok(res) => build_response(res, &final_url, eval),
        Err(e) => Err(anyhow::anyhow!("HTTP request failed: {e}")),
    }
}

fn build_response<'v>(
    res: http::Response<ureq::Body>,
    url: &str,
    eval: &mut Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    let status_code = res.status().as_u16();
    let status_text = http_status_text(status_code);

    let mut resp_headers: Vec<(String, String)> = Vec::new();
    for (name, value) in res.headers() {
        if let Ok(v) = value.to_str() {
            resp_headers.push((name.to_string(), v.to_string()));
        }
    }

    let body_str = res.into_body().read_to_string()?;

    let heap = eval.heap();
    let headers_dict = heap.alloc(AllocDict(
        resp_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str())),
    ));

    let resp = heap.alloc(AllocStruct([
        ("url", heap.alloc(url)),
        ("status_code", heap.alloc(status_code as i32)),
        ("status", heap.alloc(format!("{status_code} {status_text}").as_str())),
        ("body", heap.alloc(body_str.as_str())),
        ("headers", headers_dict),
    ]));
    Ok(resp)
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
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("GET", url, headers, "", Value::new_none(), params, eval)
    }

    fn post<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("POST", url, headers, body, json_body, params, eval)
    }

    fn put<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("PUT", url, headers, body, json_body, params, eval)
    }

    fn delete<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("DELETE", url, headers, "", Value::new_none(), params, eval)
    }

    fn patch<'v>(
        url: &str,
        #[starlark(default = NoneType)] params: Value<'v>,
        #[starlark(default = NoneType)] headers: Value<'v>,
        #[starlark(default = "")] body: &str,
        #[starlark(default = NoneType)] json_body: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        do_request("PATCH", url, headers, body, json_body, params, eval)
    }
}

pub fn build_http_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(http_module)
        .build()
}
