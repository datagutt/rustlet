use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    dict::AllocDict, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkResponse {
    pub url: String,
    pub status_code: u16,
    pub status: String,
    #[allocative(skip)]
    pub encoding: String,
    #[allocative(skip)]
    pub body: String,
    #[allocative(skip)]
    pub headers: Vec<(String, String)>,
}

starlark_simple_value!(StarlarkResponse);

impl fmt::Display for StarlarkResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<response {}>", self.status_code)
    }
}

#[starlark_value(type = "response")]
impl<'v> StarlarkValue<'v> for StarlarkResponse {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(response_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(
            attribute,
            "url" | "status_code" | "status" | "headers" | "encoding"
        )
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "url".into(),
            "status_code".into(),
            "status".into(),
            "headers".into(),
            "encoding".into(),
        ]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "url" => Some(heap.alloc(self.url.as_str())),
            "status_code" => Some(heap.alloc(self.status_code as i32)),
            "status" => Some(heap.alloc(self.status.as_str())),
            "headers" => Some(heap.alloc(AllocDict(
                self.headers.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            ))),
            "encoding" => Some(heap.alloc(self.encoding.as_str())),
            _ => None,
        }
    }
}

#[starlark::starlark_module]
fn response_methods(builder: &mut MethodsBuilder) {
    fn body<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let resp = this
            .downcast_ref::<StarlarkResponse>()
            .ok_or_else(|| anyhow::anyhow!("expected response"))?;
        Ok(eval.heap().alloc(resp.body.as_str()))
    }

    fn json<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let resp = this
            .downcast_ref::<StarlarkResponse>()
            .ok_or_else(|| anyhow::anyhow!("expected response"))?;
        json_to_starlark(&resp.body, eval.heap())
    }
}

fn json_to_starlark<'v>(s: &str, heap: &'v Heap) -> anyhow::Result<Value<'v>> {
    let parsed: serde_json::Value =
        serde_json::from_str(s).map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;
    Ok(json_value_to_starlark(&parsed, heap))
}

fn json_value_to_starlark<'v>(val: &serde_json::Value, heap: &'v Heap) -> Value<'v> {
    match val {
        serde_json::Value::Null => Value::new_none(),
        serde_json::Value::Bool(b) => heap.alloc(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                heap.alloc(i as i32)
            } else {
                heap.alloc(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => heap.alloc(s.as_str()),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value<'v>> = arr
                .iter()
                .map(|v| json_value_to_starlark(v, heap))
                .collect();
            heap.alloc(items)
        }
        serde_json::Value::Object(obj) => {
            let entries: Vec<(&str, Value<'v>)> = obj
                .iter()
                .map(|(k, v)| (k.as_str(), json_value_to_starlark(v, heap)))
                .collect();
            heap.alloc(AllocDict(entries))
        }
    }
}
