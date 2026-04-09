use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::none::NoneType;
use starlark::values::Value;

use crate::execution_context::current_app_id;

const DEFAULT_EXPIRATION_SECONDS: i32 = 60;

#[derive(Clone)]
struct CacheEntry {
    value: String,
    expires_at: Instant,
}

#[derive(Default)]
pub struct InMemoryCache {
    entries: HashMap<String, CacheEntry>,
}

impl InMemoryCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&mut self, key: &str) -> Option<String> {
        if let Some(entry) = self.entries.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.value.clone());
            }
        }
        self.entries.remove(key);
        None
    }

    fn set(&mut self, key: String, value: String, ttl_seconds: i32) {
        self.entries.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + Duration::from_secs(ttl_seconds as u64),
            },
        );
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

static CACHE: LazyLock<Mutex<Option<InMemoryCache>>> = LazyLock::new(|| Mutex::new(None));

pub fn init_cache(cache: Option<InMemoryCache>) {
    if let Ok(mut slot) = CACHE.lock() {
        *slot = cache;
    }
}

fn scoped_cache_key(key: &str) -> String {
    format!("pixlet:{}:{key:?}", current_app_id())
}

#[starlark::starlark_module]
pub fn cache_module(builder: &mut GlobalsBuilder) {
    fn get<'v>(key: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let mut cache = CACHE
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        let Some(cache) = cache.as_mut() else {
            return Ok(Value::new_none());
        };
        Ok(match cache.get(&scoped_cache_key(key)) {
            Some(value) => eval.heap().alloc(value),
            None => Value::new_none(),
        })
    }

    fn set(
        key: &str,
        value: &str,
        #[starlark(default = 0)] ttl_seconds: i32,
    ) -> anyhow::Result<NoneType> {
        if ttl_seconds < 0 {
            return Err(anyhow::anyhow!("ttl_seconds cannot be negative"));
        }
        let mut cache = CACHE
            .lock()
            .map_err(|_| anyhow::anyhow!("cache lock poisoned"))?;
        let Some(cache) = cache.as_mut() else {
            return Ok(NoneType);
        };
        let ttl = if ttl_seconds == 0 {
            DEFAULT_EXPIRATION_SECONDS
        } else {
            ttl_seconds
        };
        cache.set(scoped_cache_key(key), value.to_string(), ttl);
        Ok(NoneType)
    }
}

pub fn build_cache_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(cache_module)
        .build()
}
