use std::io::Cursor;

use image::{ImageEncoder, Luma};
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};

use crate::error::{AppError, AppResult};

/// Generates a crisp, high-contrast vector SVG QR Code (`#111111` on `#FFFFFF`, quiet zone included).
pub fn generate_svg(url: &str) -> AppResult<String> {
    let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::M)
        .map_err(|e| AppError::Internal(format!("qr encode error: {e}")))?;

    let svg_doc = code
        .render()
        .min_dimensions(320, 320)
        .quiet_zone(true)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#FFFFFF"))
        .build();

    Ok(svg_doc)
}

/// Returns the `<svg ...>...</svg>` fragment without the XML prolog for clean HTML5 embedding.
pub fn generate_inline_svg(url: &str) -> AppResult<String> {
    let raw = generate_svg(url)?;
    let stripped = raw
        .split_once("?>")
        .map(|(_, rest)| rest)
        .unwrap_or(&raw)
        .to_string();
    Ok(stripped)
}

/// Generates a high-resolution (>= 1024x1024) PNG QR Code with crisp edges and white quiet zone.
pub fn generate_png(url: &str) -> AppResult<Vec<u8>> {
    let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::M)
        .map_err(|e| AppError::Internal(format!("qr encode error: {e}")))?;

    let img = code
        .render::<Luma<u8>>()
        .min_dimensions(1024, 1024)
        .quiet_zone(true)
        .dark_color(Luma([17u8]))
        .light_color(Luma([255u8]))
        .build();

    let (w, h) = img.dimensions();
    let mut out = Vec::with_capacity(16 * 1024);
    let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut out));
    encoder
        .write_image(&img, w, h, image::ExtendedColorType::L8)
        .map_err(|e| AppError::Internal(format!("qr png encode error: {e}")))?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_high_contrast_svg_and_png() {
        let url = "https://qrcardapio.com.br/c/cafe-da-praca";
        let svg = generate_svg(url).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("#111111"));
        assert!(svg.contains("#FFFFFF"));

        let inline = generate_inline_svg(url).unwrap();
        assert!(inline.starts_with("<svg"));

        let png = generate_png(url).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = image::load_from_memory(&png).unwrap();
        assert!(decoded.width() >= 1024 && decoded.height() >= 1024);
    }
}
