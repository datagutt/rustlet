use starlark::environment::GlobalsBuilder;

#[starlark::starlark_module]
pub fn strings_module(builder: &mut GlobalsBuilder) {
    fn pad(
        text: &str,
        length: i32,
        #[starlark(default = "start")] align: &str,
        #[starlark(default = " ")] char: &str,
    ) -> anyhow::Result<String> {
        match align {
            "start" => Ok(pad_string(text, char, length, false)),
            "end" => Ok(pad_string(text, char, length, true)),
            other => Err(anyhow::anyhow!(
                "invalid strings.pad align {other:?}, expected \"start\" or \"end\""
            )),
        }
    }

    fn truncate(
        text: &str,
        length: i32,
        #[starlark(default = "…")] ellipsis: &str,
    ) -> anyhow::Result<String> {
        Ok(truncate_string(text, ellipsis, length))
    }
}

pub fn build_strings_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(strings_module)
        .build()
}

fn pad_string(text: &str, pad: &str, desired: i32, align_end: bool) -> String {
    let text_runes = text.chars().collect::<Vec<_>>();
    if desired <= 0 || text_runes.len() >= desired as usize {
        return text.to_owned();
    }

    let desired = desired.clamp(0, 512) as usize;
    let padding_needed = desired.saturating_sub(text_runes.len());
    let pad_runes = if pad.is_empty() {
        vec![' ']
    } else {
        pad.chars().collect::<Vec<_>>()
    };

    let mut padding = Vec::with_capacity(padding_needed);
    while padding.len() < padding_needed {
        padding.extend_from_slice(&pad_runes);
    }
    padding.truncate(padding_needed);

    if align_end {
        padding.into_iter().chain(text_runes).collect()
    } else {
        text_runes.into_iter().chain(padding).collect()
    }
}

fn truncate_string(text: &str, ellipsis: &str, desired: i32) -> String {
    let desired = desired.max(0) as usize;
    let text_runes = text.chars().collect::<Vec<_>>();
    if text_runes.len() <= desired {
        return text.to_owned();
    }
    if desired == 0 {
        return String::new();
    }

    let ellipsis_runes = if ellipsis.is_empty() {
        vec!['…']
    } else {
        ellipsis.chars().collect::<Vec<_>>()
    };

    if ellipsis_runes.len() >= desired {
        return ellipsis_runes.into_iter().take(desired).collect();
    }

    text_runes
        .into_iter()
        .take(desired - ellipsis_runes.len())
        .chain(ellipsis_runes)
        .collect()
}
