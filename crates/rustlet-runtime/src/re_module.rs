use regex::Regex;
use starlark::environment::GlobalsBuilder;
use starlark::values::tuple::AllocTuple;

#[starlark::starlark_module]
pub fn re_module(builder: &mut GlobalsBuilder) {
    fn findall<'v>(
        pattern: &str,
        text: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<starlark::values::Value<'v>> {
        let re = Regex::new(pattern).map_err(|e| anyhow::anyhow!("invalid regex: {e}"))?;
        let matches = re
            .find_iter(text)
            .map(|m| eval.heap().alloc(m.as_str()))
            .collect::<Vec<_>>();
        Ok(eval.heap().alloc(AllocTuple(matches)))
    }
}

pub fn build_re_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(re_module)
        .build()
}
