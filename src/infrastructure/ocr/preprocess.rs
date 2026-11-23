use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, GrayImage, ImageEncoder, ImageFormat, Luma};

use crate::error::{AppError, AppResult};

const MIN_OCR_DIM: u32 = 48;
const MAX_OCR_DIM: u32 = 8000;

/// Preprocesses a menu photo for higher Tesseract OCR accuracy:
/// 1. Strict magic-byte format check (JPEG/PNG/WebP) + dimension bounds.
/// 2. Optimal resolution scaling (upscales low-res photos, bounds oversized 4K phone captures).
/// 3. Grayscale conversion + 2nd–98th percentile contrast stretching.
/// 4. Mild edge sharpening + lossless PNG output (stripping EXIF metadata).
pub fn preprocess_image_for_ocr(raw_bytes: &[u8]) -> AppResult<Vec<u8>> {
    let format = image::guess_format(raw_bytes)
        .map_err(|_| AppError::Validation("error.ocr_unsupported_file"))?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err(AppError::Validation("error.ocr_unsupported_file"));
    }

    let decoded = image::load_from_memory_with_format(raw_bytes, format)
        .map_err(|_| AppError::Validation("error.ocr_unsupported_file"))?;

    let (w, h) = decoded.dimensions();
    if !(MIN_OCR_DIM..=MAX_OCR_DIM).contains(&w) || !(MIN_OCR_DIM..=MAX_OCR_DIM).contains(&h) {
        return Err(AppError::Validation("error.ocr_invalid_dimensions"));
    }

    let resized = normalize_resolution(&decoded, w, h);
    let gray = resized.to_luma8();
    let stretched = contrast_stretch(&gray);
    let sharpened = mild_sharpen(&stretched);

    encode_gray_png(&sharpened)
}

fn normalize_resolution(img: &DynamicImage, w: u32, h: u32) -> DynamicImage {
    if w < 1200 && h < 1600 {
        let target_w = (w.saturating_mul(2)).min(2000);
        let target_h = (h.saturating_mul(2)).min(2600);
        img.resize(target_w, target_h, FilterType::Lanczos3)
    } else if w > 2400 || h > 3200 {
        img.resize(2400, 3200, FilterType::Lanczos3)
    } else {
        img.clone()
    }
}

fn contrast_stretch(gray: &GrayImage) -> GrayImage {
    let mut hist = [0u64; 256];
    let total = u64::from(gray.width()) * u64::from(gray.height());
    if total == 0 {
        return gray.clone();
    }
    for p in gray.pixels() {
        hist[usize::from(p.0[0])] += 1;
    }

    let low_cut = total / 50; // 2nd percentile
    let high_cut = total - low_cut; // 98th percentile

    let mut acc = 0u64;
    let mut lo = 0u8;
    let mut hi = 255u8;
    for (val, count) in hist.iter().enumerate() {
        acc += *count;
        if acc >= low_cut {
            lo = val as u8;
            break;
        }
    }
    acc = 0;
    for (val, count) in hist.iter().enumerate() {
        acc += *count;
        if acc >= high_cut {
            hi = val as u8;
            break;
        }
    }

    if hi <= lo.saturating_add(10) {
        return gray.clone();
    }

    let span = u32::from(hi - lo);
    let mut out = GrayImage::new(gray.width(), gray.height());
    for (x, y, p) in gray.enumerate_pixels() {
        let v = p.0[0];
        let mapped = if v <= lo {
            0
        } else if v >= hi {
            255
        } else {
            ((u32::from(v - lo) * 255) / span) as u8
        };
        out.put_pixel(x, y, Luma([mapped]));
    }
    out
}

fn mild_sharpen(gray: &GrayImage) -> GrayImage {
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return gray.clone();
    }
    let mut out = gray.clone();
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let center = i32::from(gray.get_pixel(x, y).0[0]);
            let up = i32::from(gray.get_pixel(x, y - 1).0[0]);
            let down = i32::from(gray.get_pixel(x, y + 1).0[0]);
            let left = i32::from(gray.get_pixel(x - 1, y).0[0]);
            let right = i32::from(gray.get_pixel(x + 1, y).0[0]);
            // Mild 4-neighbor sharpening kernel: 5*c - (u + d + l + r), blended 50% with original.
            let sharp = 5 * center - (up + down + left + right);
            let blended = (center + sharp) / 2;
            out.put_pixel(x, y, Luma([blended.clamp(0, 255) as u8]));
        }
    }
    out
}

fn encode_gray_png(img: &GrayImage) -> AppResult<Vec<u8>> {
    let mut out = Vec::with_capacity((img.width() * img.height() / 4) as usize);
    let encoder = PngEncoder::new(Cursor::new(&mut out));
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|e| AppError::Internal(format!("ocr png encode failed: {e}")))?;
    Ok(out)
}
