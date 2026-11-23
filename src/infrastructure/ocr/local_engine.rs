use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use super::preprocess::preprocess_image_for_ocr;
use super::{MAX_OCR_PDF_PAGES, OcrExtractionResult};
use crate::domain::ocr::OcrSourceKind;
use crate::error::{AppError, AppResult};

const PDFTOTEXT_TIMEOUT: Duration = Duration::from_secs(15);
const PDFTOPPM_TIMEOUT: Duration = Duration::from_secs(30);
const TESSERACT_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs image preprocessing + Tesseract OCR inside an isolated temporary directory.
pub async fn extract_from_image(raw_bytes: Vec<u8>) -> AppResult<OcrExtractionResult> {
    let processed_png = tokio::task::spawn_blocking(move || preprocess_image_for_ocr(&raw_bytes))
        .await
        .map_err(|e| AppError::Internal(format!("ocr preprocess join failed: {e}")))??;

    let tmp_dir = tempfile::tempdir()?;
    let img_path = tmp_dir.path().join("menu_page.png");
    tokio::fs::write(&img_path, &processed_png).await?;

    let (langs, mut warnings) = select_tesseract_languages().await?;
    let raw_text = run_tesseract(&img_path, &langs).await?;

    if count_alnum(&raw_text) < 12 {
        warnings.push("ocr.warn_low_text".to_string());
    }

    Ok(OcrExtractionResult {
        source_kind: OcrSourceKind::Image,
        raw_text,
        page_count: 1,
        engine_info: format!("tesseract ({langs})"),
        warning_keys: warnings,
    })
}

/// Hybrid PDF menu extractor:
/// 1. Attempts fast vector text extraction via `pdftotext -layout` (100% accurate on digital PDFs).
/// 2. Automatically falls back to `pdftoppm` + preprocessing + `tesseract` for scanned image PDFs.
pub async fn extract_from_pdf(raw_bytes: Vec<u8>) -> AppResult<OcrExtractionResult> {
    let tmp_dir = tempfile::tempdir()?;
    let pdf_path = tmp_dir.path().join("menu.pdf");
    tokio::fs::write(&pdf_path, &raw_bytes).await?;

    // Fast path: vector PDF text extraction preserving visual columns.
    if let Ok((text, pages)) = run_pdftotext(&pdf_path).await
        && count_alnum(&text) >= 40
    {
        return Ok(OcrExtractionResult {
            source_kind: OcrSourceKind::PdfText,
            raw_text: text,
            page_count: pages.max(1),
            engine_info: "pdftotext (-layout)".to_string(),
            warning_keys: Vec::new(),
        });
    }

    // Scanned / image-only PDF fallback: rasterize up to MAX_OCR_PDF_PAGES at 220 DPI and OCR each page.
    let prefix = tmp_dir.path().join("page");
    let page_files = run_pdftoppm(&pdf_path, &prefix).await?;
    if page_files.is_empty() {
        return Err(AppError::Validation("error.ocr_pdf_failed"));
    }

    let (langs, mut warnings) = select_tesseract_languages().await?;
    warnings.push("ocr.warn_scanned_pdf".to_string());

    let mut combined = String::new();
    let mut processed_pages = 0u32;

    for page_path in page_files.into_iter().take(MAX_OCR_PDF_PAGES as usize) {
        let raw_page = tokio::fs::read(&page_path).await?;
        let prepped = tokio::task::spawn_blocking(move || preprocess_image_for_ocr(&raw_page))
            .await
            .map_err(|e| AppError::Internal(format!("pdf page preprocess join failed: {e}")))??;
        let clean_path = tmp_dir.path().join(format!("clean_{processed_pages}.png"));
        tokio::fs::write(&clean_path, &prepped).await?;

        let page_text = run_tesseract(&clean_path, &langs).await?;
        if !combined.is_empty() {
            combined.push_str("\n\n");
        }
        combined.push_str(&page_text);
        processed_pages += 1;
    }

    if count_alnum(&combined) < 12 {
        warnings.push("ocr.warn_low_text".to_string());
    }

    Ok(OcrExtractionResult {
        source_kind: OcrSourceKind::PdfOcr,
        raw_text: combined,
        page_count: processed_pages.max(1),
        engine_info: format!("pdftoppm + tesseract ({langs})"),
        warning_keys: warnings,
    })
}

async fn select_tesseract_languages() -> AppResult<(String, Vec<String>)> {
    let mut cmd = Command::new("tesseract");
    cmd.arg("--list-langs")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| AppError::Internal("tesseract --list-langs timed out".into()))?
        .map_err(|e| {
            tracing::error!(error = %e, "tesseract binary unavailable");
            AppError::Validation("error.ocr_engine_unavailable")
        })?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let available: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(' '))
        .collect();

    let preferred = ["por", "eng", "spa"];
    let mut chosen: Vec<&str> = preferred
        .iter()
        .copied()
        .filter(|lang| available.contains(lang))
        .collect();

    let mut warnings = Vec::new();
    if !chosen.contains(&"por") {
        warnings.push("ocr.warn_limited_langs".to_string());
    }
    if chosen.is_empty() {
        chosen.push("eng");
    }

    Ok((chosen.join("+"), warnings))
}

async fn run_tesseract(image_path: &Path, langs: &str) -> AppResult<String> {
    let mut cmd = Command::new("tesseract");
    cmd.arg(image_path)
        .arg("stdout")
        .arg("-l")
        .arg(langs)
        .arg("--oem")
        .arg("1")
        .arg("--psm")
        .arg("6")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(TESSERACT_TIMEOUT, cmd.output())
        .await
        .map_err(|_| AppError::Validation("error.ocr_timeout"))?
        .map_err(|e| {
            tracing::error!(error = %e, "failed to spawn tesseract");
            AppError::Validation("error.ocr_engine_unavailable")
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(stderr = %stderr, "tesseract exited with error");
        return Err(AppError::Validation("error.ocr_failed"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn run_pdftotext(pdf_path: &Path) -> AppResult<(String, u32)> {
    let mut cmd = Command::new("pdftotext");
    cmd.arg("-layout")
        .arg("-enc")
        .arg("UTF-8")
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg(MAX_OCR_PDF_PAGES.to_string())
        .arg(pdf_path)
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(PDFTOTEXT_TIMEOUT, cmd.output())
        .await
        .map_err(|_| AppError::Validation("error.ocr_timeout"))?
        .map_err(|_| AppError::Validation("error.ocr_engine_unavailable"))?;

    if !output.status.success() {
        return Err(AppError::Validation("error.ocr_pdf_failed"));
    }

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let pages = (text.matches('\x0c').count() as u32).max(1);
    Ok((text.replace('\x0c', "\n"), pages))
}

async fn run_pdftoppm(pdf_path: &Path, prefix: &Path) -> AppResult<Vec<std::path::PathBuf>> {
    let mut cmd = Command::new("pdftoppm");
    cmd.arg("-png")
        .arg("-r")
        .arg("220")
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg(MAX_OCR_PDF_PAGES.to_string())
        .arg(pdf_path)
        .arg(prefix)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(PDFTOPPM_TIMEOUT, cmd.output())
        .await
        .map_err(|_| AppError::Validation("error.ocr_timeout"))?
        .map_err(|_| AppError::Validation("error.ocr_engine_unavailable"))?;

    if !output.status.success() {
        return Err(AppError::Validation("error.ocr_pdf_failed"));
    }

    let parent = prefix
        .parent()
        .ok_or_else(|| AppError::Internal("invalid pdftoppm temp prefix".into()))?;
    let mut files = Vec::new();
    for entry in std::fs::read_dir(parent)?.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("png")
            && p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("page-"))
        {
            files.push(p);
        }
    }
    files.sort();
    Ok(files)
}

fn count_alnum(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphanumeric()).count()
}
