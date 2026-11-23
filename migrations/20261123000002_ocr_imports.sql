-- OCR menu import staging records (strictly tenant-scoped by establishment_id)

CREATE TABLE IF NOT EXISTS ocr_imports (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    source_filename TEXT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('image', 'pdf_text', 'pdf_ocr', 'text')),
    status TEXT NOT NULL DEFAULT 'review' CHECK (status IN ('review', 'applied', 'discarded')),
    engine_info TEXT NOT NULL DEFAULT '',
    page_count INTEGER NOT NULL DEFAULT 1,
    raw_text TEXT NOT NULL DEFAULT '',
    parsed_json TEXT NOT NULL,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    items_detected INTEGER NOT NULL DEFAULT 0,
    items_imported INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    applied_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_ocr_imports_est_created
    ON ocr_imports(establishment_id, created_at DESC);
