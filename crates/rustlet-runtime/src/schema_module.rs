use std::fmt;

use allocative::Allocative;
use serde_json::{json, Map, Value as JsonValue};
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::starlark_simple_value;
use starlark::values::list::ListRef;
use starlark::values::none::NoneType;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use crate::json_module::starlark_to_serde;

// Custom Starlark schema values that serialize to Pixlet-compatible JSON.
//
// Each schema constructor returns a `StarlarkSchemaField`. `Schema(...)` returns a
// `StarlarkSchemaSchema` holding a list of fields. Both implement `get_attr` for
// Pixlet-style attribute access (e.g. `field.id`, `schema.version`).
//
// CLI `schema` command serializes the result of `get_schema()` to JSON that matches
// Pixlet's output shape: top-level {version, schema, notifications} with each field
// emitting only the JSON keys that Pixlet includes.

#[derive(Debug, Clone, Allocative)]
pub struct SchemaField {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub default: Option<String>,
    pub secret: Option<bool>,
    #[allocative(skip)]
    pub options: Option<Vec<SchemaOption>>,
    pub source: Option<String>,
    pub handler_name: Option<String>,
    pub palette: Option<Vec<String>>,
    pub client_id: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Allocative)]
pub struct SchemaOption {
    pub text: String,
    pub value: String,
}

impl SchemaField {
    fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        map.insert("type".to_string(), JsonValue::String(self.kind.clone()));
        map.insert("id".to_string(), JsonValue::String(self.id.clone()));
        if !self.name.is_empty() {
            map.insert("name".to_string(), JsonValue::String(self.name.clone()));
        }
        if !self.description.is_empty() {
            map.insert(
                "description".to_string(),
                JsonValue::String(self.description.clone()),
            );
        }
        if !self.icon.is_empty() {
            map.insert("icon".to_string(), JsonValue::String(self.icon.clone()));
        }
        if let Some(default) = &self.default {
            if !default.is_empty() {
                map.insert("default".to_string(), JsonValue::String(default.clone()));
            }
        }
        if let Some(secret) = self.secret {
            if secret {
                map.insert("secret".to_string(), JsonValue::Bool(true));
            }
        }
        if let Some(options) = &self.options {
            let opts: Vec<JsonValue> = options
                .iter()
                .map(|o| {
                    json!({
                        "display": o.text,
                        "text": o.text,
                        "value": o.value,
                    })
                })
                .collect();
            map.insert("options".to_string(), JsonValue::Array(opts));
        }
        if let Some(source) = &self.source {
            map.insert("source".to_string(), JsonValue::String(source.clone()));
        }
        if let Some(handler) = &self.handler_name {
            map.insert("handler".to_string(), JsonValue::String(handler.clone()));
        }
        if let Some(palette) = &self.palette {
            map.insert(
                "palette".to_string(),
                JsonValue::Array(palette.iter().cloned().map(JsonValue::String).collect()),
            );
        }
        if let Some(client_id) = &self.client_id {
            map.insert(
                "client_id".to_string(),
                JsonValue::String(client_id.clone()),
            );
        }
        if let Some(endpoint) = &self.authorization_endpoint {
            map.insert(
                "authorization_endpoint".to_string(),
                JsonValue::String(endpoint.clone()),
            );
        }
        if let Some(scopes) = &self.scopes {
            map.insert(
                "scopes".to_string(),
                JsonValue::Array(scopes.iter().cloned().map(JsonValue::String).collect()),
            );
        }
        JsonValue::Object(map)
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkSchemaField {
    pub inner: SchemaField,
}

starlark_simple_value!(StarlarkSchemaField);

impl fmt::Display for StarlarkSchemaField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", type_for_kind(&self.inner.kind), self.inner.id)
    }
}

fn type_for_kind(kind: &str) -> &'static str {
    match kind {
        "text" => "Text",
        "onoff" => "Toggle",
        "dropdown" => "Dropdown",
        "radio" => "Radio",
        "location" => "Location",
        "locationbased" => "LocationBased",
        "datetime" => "DateTime",
        "oauth2" => "OAuth2",
        "photoselect" => "PhotoSelect",
        "typeahead" => "Typeahead",
        "color" => "Color",
        "generated" => "Generated",
        "notification" => "Notification",
        _ => "Field",
    }
}

#[starlark_value(type = "SchemaField")]
impl<'v> StarlarkValue<'v> for StarlarkSchemaField {
    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(
            attribute,
            "id" | "name" | "desc" | "icon" | "default" | "secret" | "options" | "type"
        )
    }

    fn dir_attr(&self) -> Vec<String> {
        ["id", "name", "desc", "icon", "default", "secret", "type"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "id" => Some(heap.alloc(self.inner.id.as_str())),
            "name" => Some(heap.alloc(self.inner.name.as_str())),
            "desc" => Some(heap.alloc(self.inner.description.as_str())),
            "icon" => Some(heap.alloc(self.inner.icon.as_str())),
            "default" => match self.inner.kind.as_str() {
                "onoff" => Some(Value::new_bool(
                    self.inner.default.as_deref() == Some("true"),
                )),
                _ => Some(heap.alloc(self.inner.default.as_deref().unwrap_or(""))),
            },
            "secret" => Some(Value::new_bool(self.inner.secret.unwrap_or(false))),
            "type" => Some(heap.alloc(self.inner.kind.as_str())),
            _ => None,
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkSchemaSchema {
    pub version: String,
    #[allocative(skip)]
    pub fields: Vec<SchemaField>,
    #[allocative(skip)]
    pub handler_names: Vec<String>,
}

starlark_simple_value!(StarlarkSchemaSchema);

impl fmt::Display for StarlarkSchemaSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Schema(...)")
    }
}

impl StarlarkSchemaSchema {
    pub fn to_json(&self) -> JsonValue {
        let schema_arr: Vec<JsonValue> = self.fields.iter().map(|f| f.to_json()).collect();
        json!({
            "version": self.version,
            "schema": schema_arr,
            "notifications": Vec::<JsonValue>::new(),
        })
    }
}

#[starlark_value(type = "Schema")]
impl<'v> StarlarkValue<'v> for StarlarkSchemaSchema {
    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "version")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec!["version".to_string()]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "version" => Some(heap.alloc(self.version.as_str())),
            _ => None,
        }
    }
}

fn alloc_field<'v>(heap: &'v Heap, field: SchemaField) -> Value<'v> {
    heap.alloc(StarlarkSchemaField { inner: field })
}

fn collect_options(value: Value<'_>) -> anyhow::Result<Option<Vec<SchemaOption>>> {
    if value.is_none() {
        return Ok(None);
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("options must be a list, got {}", value.get_type()))?;
    let mut opts = Vec::with_capacity(list.len());
    for item in list.iter() {
        if let Some(f) = item.downcast_ref::<StarlarkSchemaField>() {
            opts.push(SchemaOption {
                text: f.inner.name.clone(),
                value: f.inner.default.clone().unwrap_or_default(),
            });
            continue;
        }
        let json_val = starlark_to_serde(item)?;
        if let JsonValue::Object(obj) = json_val {
            let text = obj
                .get("display")
                .or_else(|| obj.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = obj
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            opts.push(SchemaOption { text, value });
        }
    }
    Ok(Some(opts))
}

fn collect_string_list(value: Value<'_>) -> anyhow::Result<Option<Vec<String>>> {
    if value.is_none() {
        return Ok(None);
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("expected list of strings"))?;
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        if let Some(s) = item.unpack_str() {
            out.push(s.to_string());
        } else {
            return Err(anyhow::anyhow!("expected string in list"));
        }
    }
    Ok(Some(out))
}

fn handler_function_name(value: Value<'_>) -> Option<String> {
    if value.is_none() {
        return None;
    }
    // Handler can be a function or a string. Try to extract a name.
    if let Some(s) = value.unpack_str() {
        return Some(s.to_string());
    }
    // Starlark Function has no stable name API in the bindings we expose;
    // fall back to the type display.
    Some(value.to_str())
}

#[starlark::starlark_module]
pub fn schema_module(builder: &mut GlobalsBuilder) {
    fn Schema<'v>(
        #[starlark(default = "1")] version: &str,
        #[starlark(default = NoneType)] fields: Value<'v>,
        #[starlark(default = NoneType)] handlers: Value<'v>,
        #[starlark(default = NoneType)] _notifications: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        if version != "1" {
            return Err(anyhow::anyhow!(
                "only schema version 1 is supported, not: {version}"
            ));
        }

        let mut schema_fields = Vec::new();
        if !fields.is_none() {
            let list = ListRef::from_value(fields)
                .ok_or_else(|| anyhow::anyhow!("fields must be a list"))?;
            for item in list.iter() {
                if item.is_none() {
                    continue;
                }
                let f = item
                    .downcast_ref::<StarlarkSchemaField>()
                    .ok_or_else(|| anyhow::anyhow!(
                        "expected schema field in fields list, got {}",
                        item.get_type()
                    ))?;
                schema_fields.push(f.inner.clone());
            }
        }

        let mut handler_names = Vec::new();
        if !handlers.is_none() {
            if let Some(list) = ListRef::from_value(handlers) {
                for item in list.iter() {
                    if let Some(name) = handler_function_name(item) {
                        handler_names.push(name);
                    }
                }
            }
        }

        Ok(eval.heap().alloc(StarlarkSchemaSchema {
            version: version.to_string(),
            fields: schema_fields,
            handler_names,
        }))
    }

    fn Toggle<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = false)] default: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "onoff".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(if default { "true".to_string() } else { "false".to_string() }),
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Text<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = "")] default: &str,
        #[starlark(default = false)] secret: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "text".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(default.to_string()),
                secret: Some(secret),
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Option<'v>(
        display: &str,
        value: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        // Pixlet models options as a separate struct. We piggy-back on SchemaField using
        // `name` for display text and `default` for the value so `collect_options` can read them.
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "option".to_string(),
                id: String::new(),
                name: display.to_string(),
                description: String::new(),
                icon: String::new(),
                default: Some(value.to_string()),
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Dropdown<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        default: &str,
        options: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let opts = collect_options(options)?;
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "dropdown".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(default.to_string()),
                secret: None,
                options: opts,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Radio<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        default: &str,
        options: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let opts = collect_options(options)?;
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "radio".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(default.to_string()),
                secret: None,
                options: opts,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Location<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "location".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn LocationBased<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let handler_name = handler_function_name(handler);
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "locationbased".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn DateTime<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "datetime".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn OAuth2<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        client_id: &str,
        authorization_endpoint: &str,
        #[starlark(default = NoneType)] scopes: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let handler_name = handler_function_name(handler);
        let scopes = collect_string_list(scopes)?;
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "oauth2".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name,
                palette: None,
                client_id: Some(client_id.to_string()),
                authorization_endpoint: Some(authorization_endpoint.to_string()),
                scopes,
            },
        ))
    }

    fn PhotoSelect<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "png".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Typeahead<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        handler: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let handler_name = handler_function_name(handler);
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "typeahead".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Color<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        default: &str,
        #[starlark(default = NoneType)] palette: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let palette = collect_string_list(palette)?;
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "color".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(default.to_string()),
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Generated<'v>(
        source: &str,
        handler: Value<'v>,
        id: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let handler_name = handler_function_name(handler);
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "generated".to_string(),
                id: id.to_string(),
                name: String::new(),
                description: String::new(),
                icon: String::new(),
                default: None,
                secret: None,
                options: None,
                source: Some(source.to_string()),
                handler_name,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Notification<'v>(
        id: &str,
        name: &str,
        desc: &str,
        icon: &str,
        #[starlark(default = NoneType)] _sounds: Value<'v>,
        #[starlark(default = NoneType)] _builder: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "notification".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: None,
                secret: None,
                options: None,
                source: None,
                handler_name: None,
                palette: None,
                client_id: None,
                authorization_endpoint: None,
                scopes: None,
            },
        ))
    }

    fn Handler<'v>(
        function: Value<'v>,
        #[starlark(default = "")] _handler_type: &str,
        _eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        // Pixlet wraps function references in a Handler value. We pass the function through
        // so callers can identify it later if needed; handler type is validated by field.
        Ok(function)
    }

    fn Sound<'v>(
        id: &str,
        title: &str,
        path: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        use starlark::values::structs::AllocStruct;
        let heap = eval.heap();
        Ok(heap.alloc(AllocStruct([
            ("id", heap.alloc(id)),
            ("title", heap.alloc(title)),
            ("path", heap.alloc(path)),
        ])))
    }
}

pub fn build_schema_globals() -> starlark::environment::Globals {
    let mut builder = GlobalsBuilder::new();
    schema_module(&mut builder);
    builder.build()
}
