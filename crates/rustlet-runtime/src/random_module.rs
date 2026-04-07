use std::cell::Cell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;

thread_local! {
    static RNG_STATE: Cell<u64> = const { Cell::new(0x9e37_79b9_7f4a_7c15) };
}

fn next_state(mut state: u64) -> u64 {
    if state == 0 {
        state = 0x9e37_79b9_7f4a_7c15;
    }
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    if state == 0 {
        0x2545_f491_4f6c_dd1d
    } else {
        state
    }
}

fn set_seed(seed: u64) {
    RNG_STATE.with(|cell| cell.set(next_state(seed)));
}

fn next_u64() -> u64 {
    RNG_STATE.with(|cell| {
        let next = next_state(cell.get());
        cell.set(next);
        next
    })
}

fn secure_u64() -> anyhow::Result<u64> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("secure random failed: {e}"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn sample_below(bound: u64, secure: bool) -> anyhow::Result<u64> {
    if bound == 0 {
        return Ok(0);
    }

    let zone = u64::MAX - u64::MAX % bound;
    loop {
        let value = if secure { secure_u64()? } else { next_u64() };
        if value < zone {
            return Ok(value % bound);
        }
    }
}

pub(crate) fn seed_for_execution(id: &str) {
    let bucket = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 15;

    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    bucket.hash(&mut hasher);
    set_seed(hasher.finish());
}

#[starlark::starlark_module]
pub fn random_module(builder: &mut GlobalsBuilder) {
    /// Returns a random integer in [min, max] (inclusive).
    fn number(
        min: i64,
        max: i64,
        #[starlark(default = false)] secure: bool,
    ) -> anyhow::Result<i64> {
        if min < 0 {
            return Err(anyhow::anyhow!("min has to be 0 or greater"));
        }
        if max < min {
            return Err(anyhow::anyhow!("max is less than min"));
        }
        if min == max {
            return Ok(min);
        }

        let range = (max as u64).wrapping_sub(min as u64).wrapping_add(1);
        let offset = sample_below(range, secure)?;
        Ok(min + offset as i64)
    }

    fn seed(seed: i64) -> anyhow::Result<NoneType> {
        set_seed(seed as u64);
        Ok(NoneType)
    }

    fn float() -> anyhow::Result<f64> {
        let value = next_u64() >> 11;
        Ok((value as f64) / ((1u64 << 53) as f64))
    }
}

pub fn build_random_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(random_module)
        .build()
}
