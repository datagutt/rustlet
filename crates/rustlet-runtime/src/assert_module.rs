use std::cmp::Ordering;

use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;
use starlark::values::Value;

fn assertion_error(default: String, message: Value) -> starlark::Error {
    if let Some(message) = message.unpack_str() {
        starlark::Error::new_other(anyhow::anyhow!("{message}"))
    } else {
        starlark::Error::new_other(anyhow::anyhow!("{default}"))
    }
}

#[starlark::starlark_module]
pub fn assert_module(builder: &mut GlobalsBuilder) {
    fn eq<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if left.equals(right)? {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.eq failed: {left} != {right}"),
            message,
        ))
    }

    fn ne<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if !left.equals(right)? {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.ne failed: {left} == {right}"),
            message,
        ))
    }

    fn lt<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if left.compare(right)? == Ordering::Less {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.lt failed: {left} !< {right}"),
            message,
        ))
    }

    fn le<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if left.compare(right)? != Ordering::Greater {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.le failed: {left} !<= {right}"),
            message,
        ))
    }

    fn gt<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if left.compare(right)? == Ordering::Greater {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.gt failed: {left} !> {right}"),
            message,
        ))
    }

    fn ge<'v>(
        left: Value<'v>,
        right: Value<'v>,
        #[starlark(default = NoneType)] message: Value<'v>,
    ) -> starlark::Result<bool> {
        if left.compare(right)? != Ordering::Less {
            return Ok(true);
        }
        Err(assertion_error(
            format!("assert.ge failed: {left} !>= {right}"),
            message,
        ))
    }
}

pub fn build_assert_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(assert_module)
        .build()
}
