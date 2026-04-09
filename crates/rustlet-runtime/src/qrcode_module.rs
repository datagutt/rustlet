use qrcode::{Color as QrColor, EcLevel, QrCode, Version};
use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;
use starlark::values::Value;
use tiny_skia::{Pixmap, PremultipliedColorU8};

use crate::starlark_bytes::StarlarkBytes;
use crate::starlark_color::StarlarkColor;

#[starlark::starlark_module]
pub fn qrcode_module(builder: &mut GlobalsBuilder) {
    fn generate<'v>(
        url: &str,
        size: &str,
        #[starlark(default = NoneType)] color: Value<'v>,
        #[starlark(default = NoneType)] background: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let version = match size {
            "small" => Version::Normal(1),
            "medium" => Version::Normal(2),
            "large" => Version::Normal(3),
            _ => return Err(anyhow::anyhow!("size must be small, medium, or large")),
        };

        let code = QrCode::with_version(url.as_bytes(), version, EcLevel::L)
            .map_err(|e| anyhow::anyhow!("failed to generate QR code: {e}"))?;
        let dark = parse_color(color, (255, 255, 255, 255))?;
        let light = parse_color(background, (0, 0, 0, 0))?;
        let png = render_qrcode_png(&code, dark, light)?;
        Ok(eval.heap().alloc(StarlarkBytes { data: png }))
    }
}

pub fn build_qrcode_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(qrcode_module)
        .build()
}

fn parse_color(value: Value, default: (u8, u8, u8, u8)) -> anyhow::Result<(u8, u8, u8, u8)> {
    if value.is_none() {
        return Ok(default);
    }
    let color = StarlarkColor::color_from_value(value)?
        .ok_or_else(|| anyhow::anyhow!("color cannot be None"))?;
    let color = color.to_color_u8();
    Ok((color.red(), color.green(), color.blue(), color.alpha()))
}

fn render_qrcode_png(
    code: &QrCode,
    dark: (u8, u8, u8, u8),
    light: (u8, u8, u8, u8),
) -> anyhow::Result<Vec<u8>> {
    let width = code.width() as u32;
    let mut pixmap =
        Pixmap::new(width, width).ok_or_else(|| anyhow::anyhow!("invalid QR dimensions"))?;
    let dark = PremultipliedColorU8::from_rgba(dark.0, dark.1, dark.2, dark.3)
        .ok_or_else(|| anyhow::anyhow!("invalid dark QR color"))?;
    let light = PremultipliedColorU8::from_rgba(light.0, light.1, light.2, light.3)
        .ok_or_else(|| anyhow::anyhow!("invalid background QR color"))?;
    let colors = code.to_colors();

    for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
        *pixel = if colors[index] == QrColor::Dark {
            dark
        } else {
            light
        };
    }

    pixmap
        .encode_png()
        .map_err(|e| anyhow::anyhow!("failed to encode QR PNG: {e}"))
}
