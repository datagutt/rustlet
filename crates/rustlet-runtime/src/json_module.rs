use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::{AllocDict, DictRef};
use starlark::values::list::ListRef;
use starlark::values::{Heap, Value};

#[starlark::starlark_module]
pub fn json_module(builder: &mut GlobalsBuilder) {
    fn encode(value: Value) -> anyhow::Result<String> {
        value_to_json(value)
    }

    fn decode<'v>(
        s: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let parsed: serde_json::Value =
            serde_json::from_str(s).map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;
        json_to_starlark(&parsed, eval.heap())
    }
}

pub fn build_json_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(json_module)
        .build()
}

fn value_to_json(value: Value) -> anyhow::Result<String> {
    let json = starlark_to_serde(value)?;
    serde_json::to_string(&json).map_err(|e| anyhow::anyhow!("JSON encode error: {e}"))
}

fn starlark_to_serde(value: Value) -> anyhow::Result<serde_json::Value> {
    if value.is_none() {
        return Ok(serde_json::Value::Null);
    }

    if let Some(b) = value.unpack_bool() {
        return Ok(serde_json::Value::Bool(b));
    }

    if let Some(i) = value.unpack_i32() {
        return Ok(serde_json::Value::Number(i.into()));
    }

    if let Some(s) = value.unpack_str() {
        return Ok(serde_json::Value::String(s.to_string()));
    }

    if let Some(list) = ListRef::from_value(value) {
        let arr: Result<Vec<serde_json::Value>, _> =
            list.iter().map(starlark_to_serde).collect();
        return Ok(serde_json::Value::Array(arr?));
    }

    if let Some(dict) = DictRef::from_value(value) {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k
                .unpack_str()
                .ok_or_else(|| anyhow::anyhow!("JSON dict keys must be strings"))?;
            map.insert(key.to_string(), starlark_to_serde(v)?);
        }
        return Ok(serde_json::Value::Object(map));
    }

    Err(anyhow::anyhow!(
        "cannot JSON-encode value of type {}",
        value.get_type()
    ))
}

fn json_to_starlark<'v>(
    json: &serde_json::Value,
    heap: &'v Heap,
) -> anyhow::Result<Value<'v>> {
    match json {
        serde_json::Value::Null => Ok(Value::new_none()),
        serde_json::Value::Bool(b) => Ok(Value::new_bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(heap.alloc(i as i32))
            } else if let Some(f) = n.as_f64() {
                Ok(heap.alloc(f as i32))
            } else {
                Err(anyhow::anyhow!("unsupported JSON number: {n}"))
            }
        }
        serde_json::Value::String(s) => Ok(heap.alloc(s.as_str())),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<Value<'v>>, _> = arr
                .iter()
                .map(|v| json_to_starlark(v, heap))
                .collect();
            Ok(heap.alloc(items?))
        }
        serde_json::Value::Object(map) => {
            let entries: Result<Vec<(&str, Value<'v>)>, _> = map
                .iter()
                .map(|(k, v)| json_to_starlark(v, heap).map(|val| (k.as_str(), val)))
                .collect();
            Ok(heap.alloc(AllocDict(entries?)))
        }
    }
}
