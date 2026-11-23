use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageEncoder, ImageFormat, RgbImage, Rgba};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::domain::publication::{MediaAsset, MediaKind};
use crate::error::{AppError, AppResult};
use crate::infrastructure::storage::{atomic_write_file, resolve_safe_subpath};

pub const MAX_LOGO_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PRODUCT_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
const MIN_DIMENSION: u32 = 32;
const MAX_DIMENSION: u32 = 6000;

/// Processes, validates, strips metadata from, and stores uploaded brand logos and product photos.
#[derive(Clone)]
pub struct AssetProcessor {
    media_root: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl AssetProcessor {
    pub fn new(media_root: PathBuf) -> Self {
        Self {
            media_root,
            semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    pub async fn process_and_store(
        &self,
        establishment_id: &str,
        kind: MediaKind,
        raw_bytes: Vec<u8>,
    ) -> AppResult<(MediaAsset, Vec<u8>, Vec<u8>)> {
        let max_bytes = match kind {
            MediaKind::Logo => MAX_LOGO_UPLOAD_BYTES,
            MediaKind::ProductImage => MAX_PRODUCT_UPLOAD_BYTES,
        };
        if raw_bytes.is_empty() {
            return Err(AppError::Validation("error.invalid_image"));
        }
        if raw_bytes.len() > max_bytes {
            return Err(AppError::Validation("error.image_too_large"));
        }

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::Internal("asset semaphore closed".into()))?;

        let est_id = establishment_id.to_string();
        let processed = tokio::task::spawn_blocking(move || encode_variants(kind, &raw_bytes))
            .await
            .map_err(|e| AppError::Internal(format!("image worker join error: {e}")))??;

        let est_dir = resolve_safe_subpath(&self.media_root, &est_id)?;
        let tmp_dir = est_dir.join(".tmp");

        atomic_write_file(
            &tmp_dir,
            &est_dir.join(&processed.filename_large),
            &processed.large_bytes,
        )?;
        atomic_write_file(
            &tmp_dir,
            &est_dir.join(&processed.filename_thumb),
            &processed.thumb_bytes,
        )?;

        let asset = MediaAsset {
            id: Uuid::now_v7().to_string(),
            establishment_id: est_id,
            kind,
            content_hash: processed.content_hash,
            mime_type: processed.mime_type.to_string(),
            width: i64::from(processed.width),
            height: i64::from(processed.height),
            byte_size: processed.large_bytes.len() as i64,
            filename_large: processed.filename_large,
            filename_thumb: processed.filename_thumb,
            created_at: Utc::now().to_rfc3339(),
        };

        Ok((asset, processed.large_bytes, processed.thumb_bytes))
    }

    pub fn asset_file_path(&self, establishment_id: &str, filename: &str) -> AppResult<PathBuf> {
        let est_dir = resolve_safe_subpath(&self.media_root, establishment_id)?;
        resolve_safe_subpath(&est_dir, filename)
    }

    pub fn media_root(&self) -> &Path {
        &self.media_root
    }

    pub fn remove_files(&self, asset: &MediaAsset) {
        if let Ok(large) = self.asset_file_path(&asset.establishment_id, &asset.filename_large) {
            let _ = std::fs::remove_file(large);
        }
        if let Ok(thumb) = self.asset_file_path(&asset.establishment_id, &asset.filename_thumb) {
            let _ = std::fs::remove_file(thumb);
        }
    }
}

struct EncodedVariants {
    content_hash: String,
    mime_type: &'static str,
    width: u32,
    height: u32,
    filename_large: String,
    filename_thumb: String,
    large_bytes: Vec<u8>,
    thumb_bytes: Vec<u8>,
}

fn encode_variants(kind: MediaKind, raw_bytes: &[u8]) -> AppResult<EncodedVariants> {
    let format =
        image::guess_format(raw_bytes).map_err(|_| AppError::Validation("error.invalid_image"))?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err(AppError::Validation("error.invalid_image"));
    }

    let decoded = image::load_from_memory_with_format(raw_bytes, format)
        .map_err(|_| AppError::Validation("error.invalid_image"))?;

    let (w, h) = decoded.dimensions();
    if !(MIN_DIMENSION..=MAX_DIMENSION).contains(&w)
        || !(MIN_DIMENSION..=MAX_DIMENSION).contains(&h)
    {
        return Err(AppError::Validation("error.invalid_image"));
    }

    match kind {
        MediaKind::Logo => {
            let large_img = resize_bound(&decoded, 512);
            let thumb_img = resize_bound(&decoded, 192);
            let (lw, lh) = large_img.dimensions();
            let large_bytes = encode_png(&large_img)?;
            let thumb_bytes = encode_png(&thumb_img)?;
            let hash16 = short_sha256(&large_bytes);
            Ok(EncodedVariants {
                filename_large: format!("{hash16}_512.png"),
                filename_thumb: format!("{hash16}_192.png"),
                content_hash: hash16,
                mime_type: "image/png",
                width: lw,
                height: lh,
                large_bytes,
                thumb_bytes,
            })
        }
        MediaKind::ProductImage => {
            let large_img = resize_bound(&decoded, 800);
            let thumb_img = resize_bound(&decoded, 400);
            let (lw, lh) = large_img.dimensions();
            let large_bytes = encode_jpeg_on_cream(&large_img, 82)?;
            let thumb_bytes = encode_jpeg_on_cream(&thumb_img, 80)?;
            let hash16 = short_sha256(&large_bytes);
            Ok(EncodedVariants {
                filename_large: format!("{hash16}_800.jpg"),
                filename_thumb: format!("{hash16}_400.jpg"),
                content_hash: hash16,
                mime_type: "image/jpeg",
                width: lw,
                height: lh,
                large_bytes,
                thumb_bytes,
            })
        }
    }
}

fn resize_bound(img: &DynamicImage, max_edge: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_edge && h <= max_edge {
        img.clone()
    } else {
        img.resize(max_edge, max_edge, FilterType::Lanczos3)
    }
}

fn encode_png(img: &DynamicImage) -> AppResult<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = Vec::new();
    let encoder = PngEncoder::new(Cursor::new(&mut out));
    encoder
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| AppError::Internal(format!("png encode error: {e}")))?;
    Ok(out)
}

fn encode_jpeg_on_cream(img: &DynamicImage, quality: u8) -> AppResult<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);
    // Composite alpha onto warm off-white (#FAF7F2) to avoid black transparency artifacts.
    const BG_R: u32 = 250;
    const BG_G: u32 = 247;
    const BG_B: u32 = 242;
    for (x, y, Rgba([r, g, b, a])) in rgba.enumerate_pixels() {
        let alpha = u32::from(*a);
        let inv = 255 - alpha;
        let out_r = ((u32::from(*r) * alpha + BG_R * inv) / 255) as u8;
        let out_g = ((u32::from(*g) * alpha + BG_G * inv) / 255) as u8;
        let out_b = ((u32::from(*b) * alpha + BG_B * inv) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([out_r, out_g, out_b]));
    }
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("jpeg encode error: {e}")))?;
    Ok(out)
}

fn short_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let full = hex::encode(hasher.finalize());
    full[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgba8(w, h);
        encode_png(&img).unwrap()
    }

    #[tokio::test]
    async fn processes_logo_and_product_image_stripping_metadata() {
        let dir = TempDir::new().unwrap();
        let processor = AssetProcessor::new(dir.path().to_path_buf());
        let raw = sample_png_bytes(1200, 900);

        let (logo, large, thumb) = processor
            .process_and_store("est-1", MediaKind::Logo, raw.clone())
            .await
            .unwrap();
        assert_eq!(logo.mime_type, "image/png");
        assert!(logo.width <= 512 && logo.height <= 512);
        assert!(!large.is_empty() && !thumb.is_empty());

        let (prod, plarge, pthumb) = processor
            .process_and_store("est-1", MediaKind::ProductImage, raw)
            .await
            .unwrap();
        assert_eq!(prod.mime_type, "image/jpeg");
        assert!(prod.width <= 800 && prod.height <= 800);
        assert!(!plarge.is_empty() && !pthumb.is_empty());
    }

    #[tokio::test]
    async fn rejects_svg_and_undersized_images() {
        let dir = TempDir::new().unwrap();
        let processor = AssetProcessor::new(dir.path().to_path_buf());

        let svg =
            b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>".to_vec();
        assert!(
            processor
                .process_and_store("est-1", MediaKind::Logo, svg)
                .await
                .is_err()
        );

        let tiny = sample_png_bytes(16, 16);
        assert!(
            processor
                .process_and_store("est-1", MediaKind::ProductImage, tiny)
                .await
                .is_err()
        );
    }
}
