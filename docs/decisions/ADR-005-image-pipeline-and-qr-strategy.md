# ADR-005: EXIF-Stripping Re-Encoded Media & Permanent Vector/Raster QR Codes

- **Status**: Accepted (2026-11-23)
- **Context**: User-uploaded images can carry XSS payloads (SVG), huge camera files, or EXIF GPS/device metadata; QR Codes are printed physically on tables and must never break when menus change.
- **Decision**: Accept only magic-byte-verified JPEG/PNG/WebP images within 32×32..6000×6000 px, decode and re-encode bounded variants (`512`/`192` PNG for logos; `800`/`400` JPEG on cream `#FAF7F2` for dishes) under a 2-permit semaphore (`spawn_blocking`), and strip all metadata. Encode QR Codes pointing strictly to the immutable public URL `/c/{slug}` (locked after first publication) using `EcLevel::M`, 4-module quiet zone, and crisp SVG + >=1024px PNG outputs.
- **Consequences**:
  - Eliminates SVG XSS and EXIF privacy leaks while bounding CPU/RAM spikes.
  - Printed QR Codes remain permanently scannable across arbitrary price and menu updates.
