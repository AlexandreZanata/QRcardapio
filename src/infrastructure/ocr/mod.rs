pub mod local_engine;
pub mod preprocess;

use std::sync::Arc;

use image::ImageFormat;
use tokio::sync::Semaphore;

use crate::domain::ocr::OcrSourceKind;
use crate::error::{AppError, AppResult};

pub const MAX_OCR_IMAGE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_OCR_PDF_BYTES: usize = 15 * 1024 * 1024;
pub const MAX_OCR_PDF_PAGES: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrUploadKind {
    Image,
    Pdf,
}

#[derive(Debug, Clone)]
pub struct OcrExtractionResult {
    pub source_kind: OcrSourceKind,
    pub raw_text: String,
    pub page_count: u32,
    pub engine_info: String,
    pub warning_keys: Vec<String>,
}

/// Detects upload kind strictly from file magic bytes (never trusts user file extensions or MIME headers).
pub fn detect_upload_kind(bytes: &[u8]) -> AppResult<OcrUploadKind> {
    if bytes.is_empty() {
        return Err(AppError::Validation("error.ocr_empty_file"));
    }
    // PDF header may appear within first few bytes (occasionally preceded by BOM/whitespace).
    let head = &bytes[..bytes.len().min(16)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        if bytes.len() > MAX_OCR_PDF_BYTES {
            return Err(AppError::Validation("error.ocr_file_too_large"));
        }
        return Ok(OcrUploadKind::Pdf);
    }

    if let Ok(fmt) = image::guess_format(bytes)
        && matches!(
            fmt,
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
        )
    {
        if bytes.len() > MAX_OCR_IMAGE_BYTES {
            return Err(AppError::Validation("error.ocr_file_too_large"));
        }
        return Ok(OcrUploadKind::Image);
    }

    Err(AppError::Validation("error.ocr_unsupported_file"))
}

/// Bounded OCR extraction service enforcing single-job concurrency so Tesseract/Poppler
/// memory stays strictly controlled on 1-vCPU / 1-GB RAM VPS hosts.
#[derive(Clone)]
pub struct OcrService {
    semaphore: Arc<Semaphore>,
}

impl Default for OcrService {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrService {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn extract_file(&self, raw_bytes: Vec<u8>) -> AppResult<OcrExtractionResult> {
        let kind = detect_upload_kind(&raw_bytes)?;
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AppError::Internal("ocr semaphore closed".into()))?;

        match kind {
            OcrUploadKind::Image => local_engine::extract_from_image(raw_bytes).await,
            OcrUploadKind::Pdf => local_engine::extract_from_pdf(raw_bytes).await,
        }
    }
}
