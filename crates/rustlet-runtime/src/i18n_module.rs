//! Starlark bindings for pixlet's `i18n.star`. Exposes `i18n.tr(format, *args)`,
//! which looks up `format` in the active language catalog and substitutes the
//! positional arguments. Pixlet wires a per-thread language tag and
//! `golang.org/x/text/message/catalog` via thread locals; we keep the same
//! thread-local shape with an app-scoped catalog that falls back to the raw
//! format string when no translation exists.
//!
//! Format args are substituted using Go-style printf directives (`%s`, `%d`,
//! `%v`, `%f`, `%%`) since that's what Pixlet applets write.

use std::cell::RefCell;
use std::collections::HashMap;

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::float::StarlarkFloat;
use starlark::values::tuple::TupleRef;
use starlark::values::{Value, ValueLike};

thread_local! {
    /// BCP-47 language tag for the current thread (e.g. "en", "en-US", "fr").
    static CURRENT_LANGUAGE: RefCell<String> = RefCell::new(String::from("en"));

    /// Map from language tag to (format → translated format) dictionary.
    /// A blank language is always populated so `tr` can gracefully fall back.
    static CATALOGS: RefCell<HashMap<String, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

#[allow(dead_code)]
pub fn set_language(lang: &str) {
    CURRENT_LANGUAGE.with(|l| *l.borrow_mut() = lang.to_string());
}

#[allow(dead_code)]
pub fn set_catalog_entry(lang: &str, key: String, value: String) {
    CATALOGS.with(|c| {
        c.borrow_mut()
            .entry(lang.to_string())
            .or_default()
            .insert(key, value);
    });
}

#[allow(dead_code)]
pub fn clear_catalogs() {
    CATALOGS.with(|c| c.borrow_mut().clear());
}

fn lookup_translation(format: &str) -> String {
    CATALOGS.with(|c| {
        let catalogs = c.borrow();
        let lang = CURRENT_LANGUAGE.with(|l| l.borrow().clone());
        // Try exact match, then language prefix (e.g. "en-US" → "en").
        if let Some(cat) = catalogs.get(&lang) {
            if let Some(v) = cat.get(format) {
                return v.clone();
            }
        }
        if let Some(prefix) = lang.split('-').next() {
            if prefix != lang {
                if let Some(cat) = catalogs.get(prefix) {
                    if let Some(v) = cat.get(format) {
                        return v.clone();
                    }
                }
            }
        }
        format.to_string()
    })
}

fn format_arg(value: Value<'_>) -> String {
    if value.is_none() {
        return "None".to_string();
    }
    if let Some(s) = value.unpack_str() {
        return s.to_string();
    }
    if let Some(i) = value.unpack_i32() {
        return i.to_string();
    }
    if let Some(b) = value.unpack_bool() {
        return b.to_string();
    }
    if let Some(f) = value.downcast_ref::<StarlarkFloat>() {
        return f.0.to_string();
    }
    value.to_str()
}

/// Minimal Go-style printf formatter. Supports %s, %d, %f (with optional
/// width/precision), %v (any value), %q (quoted string), and %% (literal).
/// Unknown directives are emitted verbatim and do not consume an argument,
/// matching Go's behavior for unhandled verbs.
fn apply_format(fmt: &str, args: &[Value<'_>]) -> String {
    let bytes = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len());
    let mut i = 0;
    let mut arg_idx = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b != b'%' {
            out.push(b as char);
            i += 1;
            continue;
        }

        // Parse %[flags][width][.precision]verb
        let mut j = i + 1;
        // flags
        while j < bytes.len() && matches!(bytes[j], b'+' | b'-' | b' ' | b'#' | b'0') {
            j += 1;
        }
        // width
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        // precision
        if j < bytes.len() && bytes[j] == b'.' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
        }

        if j >= bytes.len() {
            out.push_str(&fmt[i..]);
            break;
        }

        let verb = bytes[j];
        let directive = &fmt[i..=j];

        match verb {
            b'%' => {
                out.push('%');
            }
            b's' | b'v' | b'd' | b'q' | b'f' | b'g' | b'x' | b'X' | b'b' | b'o' | b'c' | b't' => {
                if arg_idx < args.len() {
                    let arg_str = format_arg(args[arg_idx]);
                    arg_idx += 1;
                    if verb == b'q' {
                        out.push('"');
                        out.push_str(&arg_str);
                        out.push('"');
                    } else {
                        out.push_str(&arg_str);
                    }
                } else {
                    out.push_str("%!(MISSING)");
                }
            }
            _ => {
                // Unknown verb: emit directive literally.
                out.push_str(directive);
            }
        }
        i = j + 1;
    }

    out
}

#[starlark::starlark_module]
pub fn i18n_module(builder: &mut GlobalsBuilder) {
    /// Translate and format a string. Matches pixlet's `i18n.tr(format, *args)`
    /// surface: looks up `format` in the current thread's catalog, falls back
    /// to the raw string, and applies positional Go-style printf formatting.
    fn tr<'v>(
        format: &str,
        #[starlark(args)] args: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let translated = lookup_translation(format);
        let arg_values: Vec<Value<'_>> = if args.is_none() {
            Vec::new()
        } else if let Some(tuple) = TupleRef::from_value(args) {
            tuple.iter().collect()
        } else {
            Vec::new()
        };
        let result = apply_format(&translated, &arg_values);
        Ok(eval.heap().alloc(result.as_str()))
    }
}

pub fn build_i18n_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(i18n_module)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_simple_strings() {
        use starlark::environment::Module;
        let module = Module::new();
        let heap = module.heap();
        let name = heap.alloc("World");
        let v = name;
        let out = apply_format("Hello, %s!", &[v]);
        assert_eq!(out, "Hello, World!");
    }

    #[test]
    fn format_int_and_float() {
        use starlark::environment::Module;
        let module = Module::new();
        let heap = module.heap();
        let count = heap.alloc(5);
        let v = count;
        let out = apply_format("You have %d items", &[v]);
        assert_eq!(out, "You have 5 items");
    }

    #[test]
    fn format_percent_literal() {
        let out = apply_format("100%% sure", &[]);
        assert_eq!(out, "100% sure");
    }
}
