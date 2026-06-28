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
use crate::starlark_file::StarlarkFile;

// Handler return types, mirroring pixlet's `HandlerReturnType` iota
// (.reference/pixlet/schema/schema.go:16-19). Exposed to Starlark as
// `schema.HandlerType.{Schema,Options,String,Field}`.
pub const RETURN_SCHEMA: i8 = 0;
pub const RETURN_OPTIONS: i8 = 1;
pub const RETURN_STRING: i8 = 2;
pub const RETURN_FIELD: i8 = 3;

/// Map a handler-bearing field's kind to its handler return type. pixlet derives
/// this from the field type (`switch schemaField.Type` in schema.go:189-200), so
/// the field kind is the single source of truth rather than a stored copy.
pub fn return_type_for_kind(kind: &str) -> i8 {
    match kind {
        "typeahead" | "locationbased" => RETURN_OPTIONS,
        "generated" => RETURN_SCHEMA,
        "oauth2" | "oauth1" => RETURN_STRING,
        _ => RETURN_STRING,
    }
}

/// Encode a schema handler's Starlark return value into the wire form pixlet's
/// `CallSchemaHandler` produces for each return type
/// (.reference/pixlet/runtime/applet.go:361-413).
pub fn encode_handler_result(return_type: i8, value: Value<'_>) -> anyhow::Result<String> {
    match return_type {
        RETURN_OPTIONS => {
            // A list of `schema.Option(...)` values -> [{display, text, value}].
            let opts = collect_options(value)?.unwrap_or_default();
            let arr: Vec<JsonValue> = opts
                .iter()
                .map(|o| {
                    json!({
                        "display": o.text,
                        "text": o.text,
                        "value": o.value,
                    })
                })
                .collect();
            Ok(serde_json::to_string(&JsonValue::Array(arr))?)
        }
        RETURN_SCHEMA => {
            let schema = value
                .downcast_ref::<StarlarkSchemaSchema>()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "generated handler must return a schema.Schema, got {}",
                        value.get_type()
                    )
                })?;
            Ok(serde_json::to_string(&schema.to_json())?)
        }
        // pixlet's AsString: return the raw string body, not a JSON-quoted value.
        RETURN_STRING => Ok(value
            .unpack_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| value.to_str())),
        // ReturnField / unknown: best-effort via the generic starlark->json bridge.
        _ => {
            let as_json = starlark_to_serde(value)?;
            Ok(serde_json::to_string(&as_json)?)
        }
    }
}

/// Valid schema field types, mirroring pixlet's `oneof` validation tag
/// (.reference/pixlet/schema/schema.go:40).
const SCHEMA_FIELD_TYPES: &[&str] = &[
    "color",
    "datetime",
    "dropdown",
    "generated",
    "location",
    "locationbased",
    "onoff",
    "radio",
    "text",
    "typeahead",
    "oauth2",
    "oauth1",
    "png",
    "notification",
];

/// Normalize a hex color to pixlet's form: lowercase, strip one optional leading
/// `#`, require exactly 3 or 6 hex digits, re-prefix `#`. Mirrors pixlet's
/// `normalizeHexColor` (schema/color.go:19) — notably it does NOT expand the
/// 3-digit form, so emitted JSON stays byte-compatible with pixlet.
fn normalize_hex_color(hex: &str) -> anyhow::Result<String> {
    let lower = hex.to_ascii_lowercase();
    let h = lower.strip_prefix('#').unwrap_or(&lower);
    if h.len() != 3 && h.len() != 6 {
        return Err(anyhow::anyhow!(
            "expected 3 or 6 hex chars but found {}",
            h.len()
        ));
    }
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!("expected hex chars a-f,0-9 but found {h}"));
    }
    Ok(format!("#{h}"))
}

/// Validate a schema against pixlet's struct-tag rules
/// (.reference/pixlet/schema/schema.go:40-60). Catches malformed schemas locally
/// instead of deferring to the backend. Notifications are validated too: pixlet
/// tags `Sounds` as `required_for=notification` and each `SchemaSound` requires
/// id/title/path (schema.go:48,73-77).
pub fn validate_schema(schema: &StarlarkSchemaSchema) -> anyhow::Result<()> {
    for f in &schema.fields {
        let id = &f.id;
        let kind = f.kind.as_str();

        if !SCHEMA_FIELD_TYPES.contains(&kind) {
            return Err(anyhow::anyhow!(
                "schema field {id:?} has unknown type {kind:?}"
            ));
        }
        if f.id.is_empty() {
            return Err(anyhow::anyhow!(
                "schema field of type {kind:?} is missing a required id"
            ));
        }
        if f.id.contains('$') {
            return Err(anyhow::anyhow!(
                "schema field id {id:?} must not contain '$'"
            ));
        }
        if matches!(
            kind,
            "datetime"
                | "dropdown"
                | "location"
                | "locationbased"
                | "onoff"
                | "radio"
                | "text"
                | "typeahead"
                | "png"
        ) && f.name.is_empty()
        {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type {kind:?} requires a name"
            ));
        }
        if kind == "generated" && !f.icon.is_empty() {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type generated must not set an icon"
            ));
        }
        if matches!(kind, "dropdown" | "onoff" | "radio")
            && f.default.as_deref().unwrap_or("").is_empty()
        {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type {kind:?} requires a default"
            ));
        }
        if matches!(kind, "dropdown" | "radio") && f.options.as_deref().unwrap_or(&[]).is_empty() {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type {kind:?} requires options"
            ));
        }
        if kind == "generated" && f.source.as_deref().unwrap_or("").is_empty() {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type generated requires a source"
            ));
        }
        if matches!(kind, "generated" | "locationbased" | "typeahead" | "oauth2")
            && f.handler_name.as_deref().unwrap_or("").is_empty()
        {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type {kind:?} requires a handler"
            ));
        }
        if kind == "oauth2" {
            if f.client_id.as_deref().unwrap_or("").is_empty() {
                return Err(anyhow::anyhow!(
                    "schema field {id:?} of type oauth2 requires a client_id"
                ));
            }
            if f.authorization_endpoint.as_deref().unwrap_or("").is_empty() {
                return Err(anyhow::anyhow!(
                    "schema field {id:?} of type oauth2 requires an authorization_endpoint"
                ));
            }
            if f.scopes.as_deref().unwrap_or(&[]).is_empty() {
                return Err(anyhow::anyhow!(
                    "schema field {id:?} of type oauth2 requires scopes"
                ));
            }
        }
        if f.secret == Some(true) && kind != "text" {
            return Err(anyhow::anyhow!(
                "schema field {id:?} of type {kind:?} must not set secret (allowed only for text)"
            ));
        }
        if let Some(opts) = &f.options {
            for (i, o) in opts.iter().enumerate() {
                if o.text.is_empty() {
                    return Err(anyhow::anyhow!(
                        "schema field {id:?} option {i} requires text"
                    ));
                }
                if o.value.is_empty() {
                    return Err(anyhow::anyhow!(
                        "schema field {id:?} option {i} requires a value"
                    ));
                }
            }
        }
    }
    for n in &schema.notifications {
        let id = &n.id;
        if n.id.is_empty() {
            return Err(anyhow::anyhow!(
                "schema notification is missing a required id"
            ));
        }
        if n.id.contains('$') {
            return Err(anyhow::anyhow!(
                "schema notification id {id:?} must not contain '$'"
            ));
        }
        if n.sounds.is_empty() {
            return Err(anyhow::anyhow!(
                "schema notification {id:?} requires at least one sound"
            ));
        }
        for (i, s) in n.sounds.iter().enumerate() {
            if s.id.is_empty() {
                return Err(anyhow::anyhow!(
                    "schema notification {id:?} sound {i} requires an id"
                ));
            }
            if s.title.is_empty() {
                return Err(anyhow::anyhow!(
                    "schema notification {id:?} sound {i} requires a title"
                ));
            }
            if s.path.is_empty() {
                return Err(anyhow::anyhow!(
                    "schema notification {id:?} sound {i} requires a path"
                ));
            }
        }
    }
    Ok(())
}

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

/// A playable sound attached to a notification. Mirrors pixlet's `SchemaSound`
/// (.reference/pixlet/schema/schema.go:73-77): all three fields are required and
/// `path` is derived from a `file` object's `.Path`.
#[derive(Debug, Clone, Allocative)]
pub struct SchemaSound {
    pub id: String,
    pub title: String,
    pub path: String,
}

/// A notification declaration. pixlet's `Notification` embeds a `SchemaField`
/// (so it serializes like one: `type:"notification"` plus id/name/desc/icon and a
/// `sounds` array) and carries a `Builder` callback that is not serialized
/// (.reference/pixlet/schema/notification.go:11-15). We store the builder by its
/// handler name (same convention as field handlers, plan 009).
#[derive(Debug, Clone, Allocative)]
pub struct Notification {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub sounds: Vec<SchemaSound>,
    pub builder_handler: Option<String>,
}

impl Notification {
    fn to_json(&self) -> JsonValue {
        let mut map = Map::new();
        // pixlet embeds SchemaField, so the entry carries the field shape.
        map.insert(
            "type".to_string(),
            JsonValue::String("notification".to_string()),
        );
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
        if !self.sounds.is_empty() {
            let sounds: Vec<JsonValue> = self
                .sounds
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "title": s.title,
                        "path": s.path,
                    })
                })
                .collect();
            map.insert("sounds".to_string(), JsonValue::Array(sounds));
        }
        JsonValue::Object(map)
    }
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
pub struct StarlarkSound {
    pub inner: SchemaSound,
}

starlark_simple_value!(StarlarkSound);

impl fmt::Display for StarlarkSound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sound({})", self.inner.id)
    }
}

#[starlark_value(type = "Sound")]
impl<'v> StarlarkValue<'v> for StarlarkSound {
    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "id" | "title" | "path")
    }

    fn dir_attr(&self) -> Vec<String> {
        ["id", "title", "path"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "id" => Some(heap.alloc(self.inner.id.as_str())),
            "title" => Some(heap.alloc(self.inner.title.as_str())),
            "path" => Some(heap.alloc(self.inner.path.as_str())),
            _ => None,
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkNotification {
    pub inner: Notification,
}

starlark_simple_value!(StarlarkNotification);

impl fmt::Display for StarlarkNotification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Notification({})", self.inner.id)
    }
}

#[starlark_value(type = "Notification")]
impl<'v> StarlarkValue<'v> for StarlarkNotification {
    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "id" | "name" | "desc" | "icon")
    }

    fn dir_attr(&self) -> Vec<String> {
        ["id", "name", "desc", "icon"]
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
    #[allocative(skip)]
    pub notifications: Vec<Notification>,
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
        let mut map = Map::new();
        map.insert(
            "version".to_string(),
            JsonValue::String(self.version.clone()),
        );
        map.insert("schema".to_string(), JsonValue::Array(schema_arr));
        // pixlet tags Notifications `omitempty`, so omit the key entirely when
        // there are none (schema.go:33) rather than emitting an empty array.
        if !self.notifications.is_empty() {
            let notifs: Vec<JsonValue> = self.notifications.iter().map(|n| n.to_json()).collect();
            map.insert("notifications".to_string(), JsonValue::Array(notifs));
        }
        JsonValue::Object(map)
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
    let list =
        ListRef::from_value(value).ok_or_else(|| anyhow::anyhow!("expected list of strings"))?;
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

fn collect_sounds(value: Value<'_>) -> anyhow::Result<Vec<SchemaSound>> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("sounds must be a list, got {}", value.get_type()))?;
    let mut out = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        if item.is_none() {
            continue;
        }
        let s = item.downcast_ref::<StarlarkSound>().ok_or_else(|| {
            anyhow::anyhow!(
                "expected sounds to be a list of Sound but found: {} (at index {i})",
                item.get_type()
            )
        })?;
        out.push(s.inner.clone());
    }
    Ok(out)
}

fn collect_notifications(value: Value<'_>) -> anyhow::Result<Vec<Notification>> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("notifications must be a list, got {}", value.get_type()))?;
    let mut out = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        if item.is_none() {
            continue;
        }
        let n = item.downcast_ref::<StarlarkNotification>().ok_or_else(|| {
            anyhow::anyhow!(
                "expected notifications to be a list of Notification but found: {} (at index {i})",
                item.get_type()
            )
        })?;
        out.push(n.inner.clone());
    }
    Ok(out)
}

fn handler_function_name(value: Value<'_>) -> Option<String> {
    if value.is_none() {
        return None;
    }
    // Handler can be a function or a string. Try to extract a name.
    if let Some(s) = value.unpack_str() {
        return Some(s.to_string());
    }
    // A `def` value's signature is its module-qualified name in this starlark
    // version (e.g. "app.search"). We want the bare function name: it matches
    // pixlet's handler key and the module global used to re-resolve the handler
    // via `module.get(name)`.
    if let Some(spec) = value.parameters_spec() {
        let sig = spec.signature();
        return Some(sig.rsplit('.').next().unwrap_or(&sig).to_string());
    }
    Some(value.to_str())
}

#[starlark::starlark_module]
pub fn schema_module(builder: &mut GlobalsBuilder) {
    fn Schema<'v>(
        #[starlark(default = "1")] version: &str,
        #[starlark(default = NoneType)] fields: Value<'v>,
        #[starlark(default = NoneType)] handlers: Value<'v>,
        #[starlark(default = NoneType)] notifications: Value<'v>,
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
                // pixlet rejects a Notification placed in `fields`; it must be
                // passed via `notifications=` (.reference/pixlet/schema/module.go:123-128).
                if item.downcast_ref::<StarlarkNotification>().is_some() {
                    return Err(anyhow::anyhow!(
                        "notifications must be passed to schema.Schema via notifications=, not in fields"
                    ));
                }
                let f = item.downcast_ref::<StarlarkSchemaField>().ok_or_else(|| {
                    anyhow::anyhow!(
                        "expected schema field in fields list, got {}",
                        item.get_type()
                    )
                })?;
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

        let notifications = collect_notifications(notifications)?;

        Ok(eval.heap().alloc(StarlarkSchemaSchema {
            version: version.to_string(),
            fields: schema_fields,
            handler_names,
            notifications,
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
                default: Some(if default {
                    "true".to_string()
                } else {
                    "false".to_string()
                }),
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
        let default = normalize_hex_color(default)
            .map_err(|e| anyhow::anyhow!("malformed default color: {e}"))?;
        let palette = collect_string_list(palette)?
            .map(|list| {
                list.iter()
                    .enumerate()
                    .map(|(i, c)| {
                        normalize_hex_color(c).map_err(|e| {
                            anyhow::anyhow!("malformed palette color at index {i}: {e}")
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .transpose()?;
        Ok(alloc_field(
            eval.heap(),
            SchemaField {
                kind: "color".to_string(),
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                default: Some(default),
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
        #[starlark(default = NoneType)] sounds: Value<'v>,
        #[starlark(default = NoneType)] builder: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sounds = collect_sounds(sounds)?;
        let builder_handler = handler_function_name(builder);
        Ok(eval.heap().alloc(StarlarkNotification {
            inner: Notification {
                id: id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                icon: icon.to_string(),
                sounds,
                builder_handler,
            },
        }))
    }

    fn Handler<'v>(
        function: Value<'v>,
        #[starlark(default = NoneType)] handler_type: Value<'v>,
        _eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        // pixlet's schema.Handler pairs a function with an explicit return type
        // (schema.HandlerType.*). We validate the type is a known int so apps
        // using this form load, but return the bare function: the field
        // constructor that consumes it derives the return type from its field
        // kind (mirrors pixlet's switch on Type).
        if let Some(t) = handler_type.unpack_i32() {
            if !(0..=3).contains(&t) {
                return Err(anyhow::anyhow!(
                    "invalid schema.Handler type {t}; expected 0..=3 (schema.HandlerType.*)"
                ));
            }
        } else if !handler_type.is_none() && handler_type.unpack_str().is_none() {
            return Err(anyhow::anyhow!(
                "schema.Handler type must be an int (schema.HandlerType.*)"
            ));
        }
        Ok(function)
    }

    fn Sound<'v>(
        id: &str,
        title: &str,
        #[starlark(default = NoneType)] file: Value<'v>,
        #[starlark(default = "")] path: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        // pixlet's signature is Sound(id, title, file) and reads path from the
        // file object (.reference/pixlet/schema/sound.go:38-44). We accept a bare
        // `path=` string as a convenience fallback, but `file=` is the canonical
        // form.
        let resolved_path = if !file.is_none() {
            let f = file.downcast_ref::<StarlarkFile>().ok_or_else(|| {
                anyhow::anyhow!("Sound file must be a file value, got {}", file.get_type())
            })?;
            f.path.clone()
        } else if !path.is_empty() {
            path.to_string()
        } else {
            return Err(anyhow::anyhow!("schema.Sound requires a file= (or path=)"));
        };
        Ok(eval.heap().alloc(StarlarkSound {
            inner: SchemaSound {
                id: id.to_string(),
                title: title.to_string(),
                path: resolved_path,
            },
        }))
    }
}

pub fn build_schema_globals() -> starlark::environment::Globals {
    let mut builder = GlobalsBuilder::new();
    schema_module(&mut builder);
    builder.build()
}
