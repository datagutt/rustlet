use qrcode::bits::Bits;
use qrcode::{Color as QrColor, EcLevel, QrCode, Version};
use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;
use starlark::values::Value;
use tiny_skia::{Pixmap, PremultipliedColorU8};

use crate::render_module::render_is_2x;
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

        let code = build_pixlet_qrcode(url.as_bytes(), version)
            .map_err(|e| anyhow::anyhow!("failed to generate QR code: {e}"))?;
        let dark = parse_color(color, (255, 255, 255, 255))?;
        let light = parse_color(background, (0, 0, 0, 0))?;
        let scale = if render_is_2x() { 2 } else { 1 };
        let png = render_qrcode_png(&code, dark, light, scale)?;
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
    scale: u32,
) -> anyhow::Result<Vec<u8>> {
    let module_width = code.width() as u32;
    let width = module_width * scale.max(1);
    let mut pixmap =
        Pixmap::new(width, width).ok_or_else(|| anyhow::anyhow!("invalid QR dimensions"))?;
    let dark = PremultipliedColorU8::from_rgba(dark.0, dark.1, dark.2, dark.3)
        .ok_or_else(|| anyhow::anyhow!("invalid dark QR color"))?;
    let light = PremultipliedColorU8::from_rgba(light.0, light.1, light.2, light.3)
        .ok_or_else(|| anyhow::anyhow!("invalid background QR color"))?;
    let colors = code.to_colors();

    for y in 0..width {
        let module_y = (y / scale.max(1)) as usize;
        for x in 0..width {
            let module_x = (x / scale.max(1)) as usize;
            let index = module_y * module_width as usize + module_x;
            pixmap.pixels_mut()[(y * width + x) as usize] = if colors[index] == QrColor::Dark {
                dark
            } else {
                light
            };
        }
    }

    pixmap
        .encode_png()
        .map_err(|e| anyhow::anyhow!("failed to encode QR PNG: {e}"))
}

fn build_pixlet_qrcode(data: &[u8], version: Version) -> qrcode::types::QrResult<QrCode> {
    let mut bits = Bits::new(version);
    bits.push_byte_data(data)?;
    bits.push_terminator(EcLevel::L)?;
    QrCode::with_bits(bits, EcLevel::L)
}
