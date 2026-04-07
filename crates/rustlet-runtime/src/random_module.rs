use std::time::{SystemTime, UNIX_EPOCH};

use starlark::environment::GlobalsBuilder;

#[starlark::starlark_module]
pub fn random_module(builder: &mut GlobalsBuilder) {
    /// Returns a random integer in [min, max] (inclusive).
    fn number(min: i32, max: i32) -> anyhow::Result<i32> {
        if min > max {
            return Err(anyhow::anyhow!(
                "min ({min}) must be <= max ({max})"
            ));
        }
        if min == max {
            return Ok(min);
        }

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        // xorshift64 seeded from current time nanos
        let mut state = seed as u64;
        if state == 0 {
            state = 0xDEAD_BEEF;
        }
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;

        let range = (max as i64 - min as i64 + 1) as u64;
        let result = min as i64 + (state % range) as i64;
        Ok(result as i32)
    }
}

pub fn build_random_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(random_module)
        .build()
}
